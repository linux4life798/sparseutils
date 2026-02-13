use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use drill_press::{ScanError, SegmentType, SparseFile};

const MIN_STRING_LEN: usize = 4;
const READ_BUF_SIZE: usize = 8 * 1024;
const MAX_UTF8_CARRY: usize = 3;

#[derive(Parser, Debug)]
#[command(name = "sparsestrings")]
struct Cli {
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// Incremental UTF-8 scanner that accumulates printable runs and emits
/// strings when separators are encountered.
struct StringScanner {
    candidate: String,
    char_count: usize,
    /// Partial UTF-8 bytes carried across input chunk boundaries.
    ///
    /// Bounded to at most one incomplete UTF-8 scalar (3 bytes).
    carry: Vec<u8>,
}

impl StringScanner {
    /// Creates a fresh scanner state for one input stream/file.
    const fn new() -> Self {
        Self {
            candidate: String::new(),
            char_count: 0,
            carry: Vec::new(),
        }
    }

    /// Emits the current candidate only if it meets the minimum length.
    fn flush_candidate(&mut self) {
        if self.char_count >= MIN_STRING_LEN {
            println!("{}", self.candidate);
        }
        self.candidate.clear();
        self.char_count = 0;
    }

    fn process_valid_text(&mut self, valid: &str) {
        for ch in valid.chars() {
            if ch.is_control() {
                // Control bytes terminate printable runs.
                self.flush_candidate();
            } else {
                self.candidate.push(ch);
                self.char_count += 1;
            }
        }
    }

    fn process_data(&mut self, data: &[u8]) {
        let mut index = 0_usize;
        while index < data.len() {
            match std::str::from_utf8(&data[index..]) {
                Ok(valid) => {
                    self.process_valid_text(valid);
                    break;
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid_slice = &data[index..index + valid_up_to];
                        if let Ok(valid) = std::str::from_utf8(valid_slice) {
                            self.process_valid_text(valid);
                        }
                    }
                    index += valid_up_to;

                    if let Some(error_len) = err.error_len() {
                        // Invalid UTF-8 bytes act as separators.
                        self.flush_candidate();
                        // Skip past invalid UTF-8
                        index += error_len;
                    } else {
                        debug_assert!(index <= data.len());
                        // The index is within the last 3 bytes.
                        debug_assert!(data.len() - index <= MAX_UTF8_CARRY);
                        // Keep incomplete UTF-8 for the next chunk processing
                        self.carry.extend_from_slice(&data[index..]);
                        debug_assert!(self.carry.len() <= MAX_UTF8_CARRY);
                        // End this processing for incomplete, but possibly
                        // valid, trailing multi-byte UTF-8.
                        break;
                    }
                }
            }
        }
    }

    /// Feeds raw bytes into the scanner, preserving incomplete UTF-8 between chunks.
    ///
    /// `carry` does not grow without bound. `Utf8Error::error_len() == None`
    /// only occurs on unexpected end-of-input and leaves at most 1..=3 trailing
    /// bytes to carry into the next read.
    fn push_bytes(&mut self, chunk: &[u8]) {
        if self.carry.is_empty() {
            self.process_data(chunk);
            return;
        }

        let mut data = Vec::with_capacity(self.carry.len() + chunk.len());
        data.extend_from_slice(&self.carry);
        data.extend_from_slice(chunk);
        self.carry.clear();
        self.process_data(&data);
    }

    /// Forces a separator at sparse-hole boundaries.
    fn hole_break(&mut self) {
        self.carry.clear();
        self.flush_candidate();
    }

    /// Flushes trailing state at end-of-input.
    fn finish(&mut self) {
        self.carry.clear();
        self.flush_candidate();
    }
}

/// Sequential buffered scan used for streams and as sparse-detection fallback.
fn scan_reader<R: Read>(reader: &mut R, scanner: &mut StringScanner) -> io::Result<()> {
    let mut read_buf = [0_u8; READ_BUF_SIZE];

    loop {
        let bytes_read = reader.read(&mut read_buf)?;
        if bytes_read == 0 {
            break;
        }
        scanner.push_bytes(&read_buf[..bytes_read]);
    }

    Ok(())
}

fn scan_regular_file_sparse(
    file: &mut File,
    scanner: &mut StringScanner,
    segments: &[drill_press::Segment],
) -> io::Result<()> {
    for segment in segments {
        if segment.range.is_empty() {
            continue;
        }

        match segment.segment_type {
            SegmentType::Hole => scanner.hole_break(),
            SegmentType::Data => {
                let len = segment.range.end - segment.range.start;
                file.seek(SeekFrom::Start(segment.range.start))?;
                let limited = (&mut *file).take(len);
                let mut reader = BufReader::new(limited);
                scan_reader(&mut reader, scanner)?;
            }
        }
    }

    Ok(())
}

fn scan_regular_file_sequential(file: &mut File, scanner: &mut StringScanner) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    scan_reader(&mut reader, scanner)
}

fn sparse_error_brief(err: &ScanError) -> String {
    match err {
        ScanError::UnsupportedPlatform => "unsupported platform".to_string(),
        ScanError::UnsupportedFileSystem => "unsupported filesystem sparse API".to_string(),
        ScanError::IO(ioe) => ioe.to_string(),
    }
}

/// Scans a single CLI path, where "-" means stdin.
fn scan_path(path: &Path) -> io::Result<()> {
    let mut scanner = StringScanner::new();

    if path == Path::new("-") {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        scan_reader(&mut handle, &mut scanner)?;
        scanner.finish();
        return Ok(());
    }

    let mut file = File::open(path)?;
    let is_regular_file = file.metadata()?.is_file();

    if is_regular_file {
        match file.scan_chunks() {
            Ok(segments) => scan_regular_file_sparse(&mut file, &mut scanner, &segments)?,
            Err(err) => {
                eprintln!(
                    "sparsestrings: {}: sparse scan failed ({}); falling back to sequential scan",
                    path.display(),
                    sparse_error_brief(&err),
                );
                scan_regular_file_sequential(&mut file, &mut scanner)?;
            }
        }
    } else {
        let mut reader = BufReader::new(file);
        scan_reader(&mut reader, &mut scanner)?;
    }

    scanner.finish();
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut had_error = false;

    for path in &cli.files {
        if let Err(err) = scan_path(path) {
            eprintln!("sparsestrings: {}: {}", path.display(), err);
            had_error = true;
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
