# sparseutils

[![Rust CI](https://github.com/linux4life798/sparseutils/actions/workflows/rust-ci.yml/badge.svg?branch=main)](https://github.com/linux4life798/sparseutils/actions/workflows/rust-ci.yml)

Small command-line utilities for working with sparse files and related workflows.

## Utilities

- [`sparsestrings`](./SPARSESTRINGS.md): `strings`-style scanner optimized for sparse files.
- [`sparsesqueeze`](./SPARSESQUEEZE.md): `cat`-style copier that reduces/removes sparse holes from the output data stream
  Use this tool if you want to simple see a view of the sparse file without the massive 0 holes.
