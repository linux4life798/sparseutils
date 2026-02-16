# sparseutils

[![Rust CI](https://github.com/linux4life798/sparseutils/actions/workflows/rust-ci.yml/badge.svg?branch=main)](https://github.com/linux4life798/sparseutils/actions/workflows/rust-ci.yml)

Small command-line utilities for working with sparse files, such as mammoth Chrome core dumps.

```bash
cargo install sparseutils
```

## Utilities

- [`sparsestrings`](./SPARSESTRINGS.md): `strings`-compatible scanner that skips sparse holes to find text quickly.
- [`sparsesqueeze`](./SPARSESQUEEZE.md): `cat`-compatible stream copier that removes sparse holes for downstream tools.

## Examples

* Create a large example sparse file with embedded text:

  ```bash
  truncate -s 1T sparse.bin
  echo "Beginning" | dd of=sparse.bin bs=1 seek=0 conv=notrunc status=none
  echo "End"       | dd of=sparse.bin bs=1 seek=1T conv=notrunc status=none
  ```

  *Running `strings` directly on this file can take a long time before reaching data near the end.*

* Extract strings from a large core dump or sparse file:

  ```bash
  sparsestrings sparse.bin

  # We can make the normal strings work, if we use sparsesqueeze!
  sparsesqueeze sparse.bin | strings
  ```

* Prepare a large core dump or sparse file for analysis with a hex editor:

  ```bash
  sparsesqueeze sparse.bin >noholes.bin
  ```
