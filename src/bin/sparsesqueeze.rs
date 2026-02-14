use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use drill_press::SparseFile;
use sparseutils::sparse_io::{sparse_error_brief, visit_sparse_segments};

const ZERO_BUF_SIZE: usize = 8 * 1024;

#[derive(Parser, Debug)]
#[command(name = "sparsesqueeze")]
struct Cli {
    #[arg(short = 'n', long = "nulls", default_value_t = 4)]
    nulls: usize,

    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn copy_reader_to_writer<R: Read + ?Sized, W: Write + ?Sized>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<()> {
    io::copy(reader, writer)?;
    Ok(())
}

fn write_n_null_bytes<W: Write + ?Sized>(writer: &mut W, count: usize) -> io::Result<()> {
    if count == 0 {
        return Ok(());
    }

    let zero_buf = [0_u8; ZERO_BUF_SIZE];
    let mut remaining = count;

    while remaining > 0 {
        let chunk = remaining.min(zero_buf.len());
        writer.write_all(&zero_buf[..chunk])?;
        remaining -= chunk;
    }

    Ok(())
}

struct SparseWriteContext<'a, W: Write + ?Sized> {
    out: &'a mut W,
    nulls_per_hole: usize,
    last_hole_end: Option<u64>,
}

fn squeeze_regular_file_sparse<W: Write + ?Sized>(
    file: &mut File,
    out: &mut W,
    nulls_per_hole: usize,
    segments: &[drill_press::Segment],
) -> io::Result<()> {
    let mut context = SparseWriteContext {
        out,
        nulls_per_hole,
        last_hole_end: None,
    };

    visit_sparse_segments(
        file,
        segments,
        &mut context,
        |context, reader| {
            context.last_hole_end = None;
            copy_reader_to_writer(reader, context.out)
        },
        |context, start, len| {
            let is_contiguous_hole = context.last_hole_end == Some(start);
            if !is_contiguous_hole {
                write_n_null_bytes(context.out, context.nulls_per_hole)?;
            }
            context.last_hole_end = Some(start + len);
            Ok(())
        },
    )
}

fn squeeze_regular_file_sequential<W: Write + ?Sized>(
    file: &mut File,
    out: &mut W,
) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut reader = BufReader::new(file);
    copy_reader_to_writer(&mut reader, out)
}

fn squeeze_path<W: Write + ?Sized>(
    path: &Path,
    out: &mut W,
    nulls_per_hole: usize,
) -> io::Result<()> {
    if path == Path::new("-") {
        let stdin = io::stdin();
        let mut handle = stdin.lock();
        return copy_reader_to_writer(&mut handle, out);
    }

    let mut file = File::open(path)?;
    let is_regular_file = file.metadata()?.is_file();

    if is_regular_file {
        match file.scan_chunks() {
            Ok(segments) => squeeze_regular_file_sparse(&mut file, out, nulls_per_hole, &segments)?,
            Err(err) => {
                eprintln!(
                    "sparsesqueeze: {}: sparse scan failed ({}); falling back to sequential scan",
                    path.display(),
                    sparse_error_brief(&err),
                );
                squeeze_regular_file_sequential(&mut file, out)?;
            }
        }
    } else {
        let mut reader = BufReader::new(file);
        copy_reader_to_writer(&mut reader, out)?;
    }

    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut had_error = false;

    let stdout = io::stdout();
    let mut out = stdout.lock();

    for path in &cli.files {
        if let Err(err) = squeeze_path(path, &mut out, cli.nulls) {
            eprintln!("sparsesqueeze: {}: {}", path.display(), err);
            had_error = true;
        }
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
