# stringspeed

`stringspeed` is a `strings`-style tool optimized for sparse files, avoiding the
long slowdowns traditional `strings` can hit when traversing very large hole regions.

## What it does

- Accepts one or more positional file paths.
- Supports `-` to read from standard input.
- Prints runs of at least 4 UTF-8 characters.
- Treats control characters and invalid UTF-8 as separators.
- Treats sparse hole regions as separators (equivalent to runs of NUL bytes).

## Sparse-file behavior

When possible, `stringspeed` skips sparse hole regions instead of scanning long runs of
zeros.

- Hole regions are treated as separators (same practical effect as NUL bytes).
- If hole-skipping is unavailable, `stringspeed` prints a short note to stderr and
  continues with normal sequential scanning.
- Streams and non-regular files are scanned sequentially.

This keeps scanning efficient on sparse files without getting stuck walking large hole
regions byte-by-byte.
