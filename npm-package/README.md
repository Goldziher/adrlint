# adrlint

[![npm](https://img.shields.io/npm/v/adrlint?style=flat-square&color=f59e0b&label=npm)](https://www.npmjs.com/package/adrlint)
[![License: MIT](https://img.shields.io/badge/License-MIT-f59e0b?style=flat-square)](https://github.com/Goldziher/adrlint/blob/main/LICENSE)

A fast Rust CLI that lints and formats [MADR](https://adr.github.io/madr/) architecture decision
records: structure, numbering, Markdown quality, and immutability of accepted decisions.

## Installation

```bash
pnpm add -g adrlint
```

The postinstall step downloads the pre-compiled binary for your platform and verifies it against the
release checksum manifest. Prebuilt archives exist for Linux (glibc) `x86_64`/`aarch64`, macOS
`x86_64`/`aarch64`, and Windows `x86_64`.

Anything else — musl/Alpine Linux, 32-bit, other architectures — has no prebuilt archive; install with
`cargo install adrlint` instead.

## Usage

```bash
adrlint init                    # write .adrlint.toml and docs/adr
adrlint lint                    # check every ADR
adrlint new "Use PostgreSQL"    # create the next sequential ADR
adrlint lint --format github    # GitHub annotations for CI
```

## Documentation

Rules, configuration, locking and the full CLI reference:
[https://github.com/Goldziher/adrlint](https://github.com/Goldziher/adrlint)

## License

MIT — see [LICENSE](https://github.com/Goldziher/adrlint/blob/main/LICENSE).
