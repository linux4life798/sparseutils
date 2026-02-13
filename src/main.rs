use std::fs::File;
use std::io::{self, BufReader, Read};
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

fn is_string_char(ch: char) -> bool {
    !ch.is_control()
}

fn flush_candidate(candidate: &mut String, char_count: &mut usize) {
    if *char_count >= MIN_STRING_LEN {
        println!("{candidate}");
    }
    candidate.clear();
    *char_count = 0;
}

fn process_valid_text(valid: &str, candidate: &mut String, char_count: &mut usize) {
    for ch in valid.chars() {
        if is_string_char(ch) {
            candidate.push(ch);
            *char_count += 1;
        } else {
            flush_candidate(candidate, char_count);
        }
    }
}

fn scan_file(path: &Path) -> io::Result<()> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut read_buf = [0_u8; READ_BUF_SIZE];
    let mut carry = Vec::<u8>::new();
    let mut candidate = String::new();
    let mut char_count = 0_usize;

    loop {
        let bytes_read = reader.read(&mut read_buf)?;
        if bytes_read == 0 {
            break;
        }

        let chunk = &read_buf[..bytes_read];
        let mut data = Vec::with_capacity(carry.len() + chunk.len());
        data.extend_from_slice(&carry);
        data.extend_from_slice(chunk);
        carry.clear();

        let mut index = 0_usize;
        while index < data.len() {
            match std::str::from_utf8(&data[index..]) {
                Ok(valid) => {
                    process_valid_text(valid, &mut candidate, &mut char_count);
                    index = data.len();
                }
                Err(err) => {
                    let valid_up_to = err.valid_up_to();
                    if valid_up_to > 0 {
                        let valid = std::str::from_utf8(&data[index..index + valid_up_to])
                            .expect("slice must be valid UTF-8");
                        process_valid_text(valid, &mut candidate, &mut char_count);
                    }
                    index += valid_up_to;

                    if let Some(error_len) = err.error_len() {
                        flush_candidate(&mut candidate, &mut char_count);
                        index += error_len;
                    } else {
                        carry.extend_from_slice(&data[index..]);
                        index = data.len();
                    }
                }
            }
        }
    }

    flush_candidate(&mut candidate, &mut char_count);
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let mut had_error = false;

    for path in &cli.files {
        if let Err(err) = scan_file(path) {
            eprintln!("stringspeed: {}: {}", path.display(), err);
            had_error = true;
        }
    }

    if had_error {
        std::process::exit(1);
    }
}
