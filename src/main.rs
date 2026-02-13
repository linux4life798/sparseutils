use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
#[cfg(target_os = "linux")]
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

use clap::Parser;

const MIN_STRING_LEN: usize = 4;
const READ_BUF_SIZE: usize = 8 * 1024;

#[derive(Parser, Debug)]
#[command(name = "stringspeed")]
struct Cli {
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

/// Incremental UTF-8 scanner that accumulates printable runs and emits
/// strings when separators are encountered.
struct StringScanner {
    candidate: String,
    char_count: usize,
    /// Partial UTF-8 bytes carried across chunk boundaries.
    carry: Vec<u8>,
}

impl StringScanner {
    /// Creates a fresh scanner state for one input stream/file.
    fn new() -> Self {
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
        } else {
            self.candidate.clear();
        }
        self.char_count = 0;
    }

    fn process_valid_text(&mut self, valid: &str) {
        for ch in valid.chars() {
            if !ch.is_control() {
                self.candidate.push(ch);
                self.char_count += 1;
            } else {
                // Control bytes terminate printable runs.
                self.flush_candidate();
            }
        }
    }

    /// Feeds raw bytes into the scanner, preserving incomplete UTF-8 between chunks.
    fn push_bytes(&mut self, chunk: &[u8]) {
        let mut data = Vec::with_capacity(self.carry.len() + chunk.len());
        data.extend_from_slice(&self.carry);
        data.extend_from_slice(chunk);
        self.carry.clear();

        let mut index = 0_usize;
        while index < data.len() {
            match std::str::from_utf8(&data[index..]) {
                Ok(valid) => {
                    self.process_valid_text(valid);
                    index = data.len();
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid = std::str::from_utf8(&data[index..index + valid_up_to])
                            .expect("slice must be valid UTF-8");
                        self.process_valid_text(valid);
                    }
                    index += valid_up_to;

                    if let Some(error_len) = err.error_len() {
                        // Invalid UTF-8 bytes act as separators.
                        self.flush_candidate();
                        index += error_len;
                    } else {
                        // Keep incomplete UTF-8 for the next chunk.
                        self.carry.extend_from_slice(&data[index..]);
                        index = data.len();
                    }
                }
            }
        }
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

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SparseScanResult {
    Scanned,
    Unsupported,
}

#[cfg(target_os = "linux")]
/// Small wrapper around Linux `lseek` so we can query SEEK_DATA/SEEK_HOLE.
fn lseek_linux(fd: std::os::fd::RawFd, offset: u64, whence: libc::c_int) -> io::Result<u64> {
    let offset = i64::try_from(offset)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "offset overflow"))?;
    let result = unsafe { libc::lseek(fd, offset as libc::off_t, whence) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as u64)
    }
}

#[cfg(target_os = "linux")]
/// Errors that mean sparse probing is unavailable on this kernel/filesystem.
fn sparse_unsupported(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(libc::EINVAL | libc::ENOTSUP | libc::ENOSYS)
    )
}

#[cfg(target_os = "linux")]
/// Sparse-aware scanner for regular files on Linux.
///
/// Walks data extents via SEEK_DATA/SEEK_HOLE and reads only allocated regions.
fn scan_sparse_file(file: &File, scanner: &mut StringScanner) -> io::Result<SparseScanResult> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(SparseScanResult::Scanned);
    }

    let fd = file.as_raw_fd();
    let mut pos = 0_u64;
    let mut next_data = match lseek_linux(fd, 0, libc::SEEK_DATA) {
        Ok(off) => Some(off),
        Err(err) if sparse_unsupported(&err) => return Ok(SparseScanResult::Unsupported),
        Err(err) if err.raw_os_error() == Some(libc::ENXIO) => {
            scanner.hole_break();
            return Ok(SparseScanResult::Scanned);
        }
        Err(err) => return Err(err),
    };

    let mut read_buf = [0_u8; READ_BUF_SIZE];

    while pos < len {
        let data_off = if let Some(off) = next_data.take() {
            off
        } else {
            match lseek_linux(fd, pos, libc::SEEK_DATA) {
                Ok(off) => off,
                Err(err) if err.raw_os_error() == Some(libc::ENXIO) => {
                    scanner.hole_break();
                    break;
                }
                Err(err) => return Err(err),
            }
        };

        if data_off >= len {
            break;
        }

        if data_off > pos {
            // We skipped a hole; treat it as a separator.
            scanner.hole_break();
        }

        let hole_off = match lseek_linux(fd, data_off, libc::SEEK_HOLE) {
            Ok(off) => off.min(len),
            Err(err) if err.raw_os_error() == Some(libc::ENXIO) => len,
            Err(err) => return Err(err),
        };

        let mut extent_pos = data_off;
        while extent_pos < hole_off {
            let remaining = (hole_off - extent_pos) as usize;
            let to_read = remaining.min(read_buf.len());
            let read = file.read_at(&mut read_buf[..to_read], extent_pos)?;
            if read == 0 {
                break;
            }
            scanner.push_bytes(&read_buf[..read]);
            extent_pos += read as u64;
        }

        pos = extent_pos;
        if extent_pos < hole_off {
            break;
        }
    }

    Ok(SparseScanResult::Scanned)
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

    if file.metadata()?.is_file() {
        #[cfg(target_os = "linux")]
        {
            match scan_sparse_file(&file, &mut scanner)? {
                SparseScanResult::Scanned => {}
                SparseScanResult::Unsupported => {
                    // Sparse probing unsupported here; fall back to sequential scan.
                    file.seek(SeekFrom::Start(0))?;
                    let mut reader = BufReader::new(file);
                    scan_reader(&mut reader, &mut scanner)?;
                }
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let mut reader = BufReader::new(file);
            scan_reader(&mut reader, &mut scanner)?;
        }
    } else {
        let mut reader = BufReader::new(file);
        scan_reader(&mut reader, &mut scanner)?;
    }

    scanner.finish();
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let mut had_error = false;

    for path in &cli.files {
        if let Err(err) = scan_path(path) {
            eprintln!("stringspeed: {}: {}", path.display(), err);
            had_error = true;
        }
    }

    if had_error {
        std::process::exit(1);
    }
}
