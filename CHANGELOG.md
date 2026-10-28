# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

- `adrlint lint` with directory rules `MADR41a`, `MADR41b`, `MADR42`, `MADR43` and `MADR44`, text,
  JSON and GitHub output, and exit codes `0`/`1`/`2`.
- `adrlint init` to write `.adrlint.toml`, create the ADR directory and add the first record.
- `adrlint new "<title>"` to create the next sequential ADR without overwriting an existing file.
- Layered `.adrlint.toml` configuration with `[adr]` keys `dir`, `start` and `allow_gaps`.
- npm, PyPI and Homebrew wrappers, a composite GitHub Action, and pre-commit hooks.
