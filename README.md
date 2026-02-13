# stringspeed

`stringspeed` is a simplified `strings`-style tool.

## What it does

- Accepts one or more positional file paths.
- Supports `-` to read from standard input.
- Prints runs of at least 4 UTF-8 characters.
- Treats control characters and invalid UTF-8 as separators.

## Sparse-file behavior (Linux)

For regular files on Linux, `stringspeed` tries to use sparse extent discovery with
`SEEK_DATA` / `SEEK_HOLE` and only reads allocated data ranges.

- Hole regions are treated as separators (same practical effect as runs of zero bytes).
- If sparse extent discovery is not supported by the filesystem/kernel, it falls back
  to normal buffered sequential scanning.
- Streams and non-regular files are scanned sequentially.

This keeps scanning efficient on sparse files without getting stuck walking large hole
regions byte-by-byte.

## Future portability note

Sparse extent APIs vary by OS:

- Linux: `lseek(SEEK_DATA/SEEK_HOLE)`
- macOS/BSD: similar `SEEK_DATA/SEEK_HOLE` support with platform-specific behavior
- Windows: `FSCTL_QUERY_ALLOCATED_RANGES`

A future cross-platform implementation would likely need platform-specific backends.
