# sparsesqueeze

`sparsesqueeze` behaves like `cat`, but for sparse regular files it replaces each hole
segment with `n` NUL bytes instead of outputting the full hole contents.

## What it does

- Accepts one or more positional file paths.
- Supports `-` to read from standard input.
- Copies non-sparse inputs byte-for-byte (`cat` behavior).
- For sparse regular files, writes data segments normally and writes `n` zero bytes per hole.
- `-n, --nulls <N>` controls hole replacement bytes and defaults to `4` (including `0`).

## Fallback behavior

If sparse-hole scanning is unavailable for a regular file, `sparsesqueeze` prints a short
note to stderr and falls back to normal sequential copying for that file.

## Example

```bash
cargo run --bin sparsesqueeze -- -n 8 sparse.img > squeezed.bin
```
