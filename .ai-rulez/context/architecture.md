---
type: Concept
title: Architecture
x-ai-rulez:
  kind: context
  id: architecture
  metadata:
    priority: high
---

## Project Architecture

adrlint is a Rust CLI linter and formatter for Architecture Decision Records written in Markdown
following the [MADR](https://github.com/adr/madr) specification. It is distributed through crates.io,
npm, PyPI and Homebrew, using the same binary-wrapper pattern as the other Goldziher Rust CLIs.

The tool is opinionated in four ways:

1. **MADR structure** — the mandatory sections, heading levels, ordering and decision-outcome wording
   defined by the MADR template.
2. **Markdown quality** — [rumdl](https://github.com/rvben/rumdl) supplies Markdown linting and
   formatting in the same process, so an ADR is judged as Markdown and as an ADR in one run.
3. **Sequential numbering** — an ADR directory is a single ordered log: names are `NNNN-slug.md`,
   numbers start at a configured base, and they do not skip or collide.
4. **Immutability** — accepted ADRs are hashed into `adrlint.lock`; a later edit fails until the file
   is explicitly unlocked, so a decision cannot be rewritten in place by accident.

### Source Code (`src/`)

The 0.1.0 scaffold implements `cli`, `config`, `discovery`, `diagnostics`, `report`, `rules` and
`template`. The remaining modules below are the target layout and land with the rule families they
serve; do not create empty placeholders for them.

- `main.rs` — entry point and process exit codes
- `cli.rs` — clap command and flag definitions
- `config/` — layered `.adrlint.toml` discovery and resolution
  - `file.rs` — the deserialized shapes
  - `manager.rs` — discovery, caching and per-file layering
- `adr/` — the ADR domain
  - `discovery.rs` — finding ADR files and reading their numeric prefix
  - `document.rs` — the parsed document: front matter, headings, sections
  - `front_matter.rs` — YAML front matter extraction
  - `numbering.rs` — filename parsing and sequentiality
- `markdown/` — the Markdown layer
  - `parse.rs` — one pulldown-cmark parse per document, offsets retained
  - `rumdl.rs` — adapting rumdl diagnostics and fixes into adrlint findings
  - `format.rs` — applying formatting edits
- `rules/` — the rule engine
  - `mod.rs` — `Rule` trait, registry, severity resolution
  - `madr/` — rules ported from madrlint (`MADR01`–`MADR44`)
  - `adrlint/` — rules specific to adrlint (`ADR50`–`ADR51`)
- `lock.rs` — `adrlint.lock` creation, verification and unlocking
- `diagnostics.rs` — finding model, severity and ordering
- `report.rs` — text, JSON and GitHub annotations output
- `ui.rs` — terminal colour and symbols

### Distribution

- `npm-package/` — npm wrapper (`adrlint`) that downloads the release binary
- `pip-package/` — PyPI wrapper (`adrlint`) with the same binary-download strategy
- `.github/workflows/publish.yaml` — native per-platform release build matrix
- `scripts/update-homebrew-formula.sh` — regenerates the Homebrew formula from release checksums
- Root `action.yml` — composite GitHub Action that installs and runs `adrlint lint`

### Testing

- Unit tests are co-located in source files under `#[cfg(test)]`.
- Integration tests live in `tests/`.
- `fixtures/adr/` holds valid and invalid MADR corpora, including the samples from `adr/madrlint`
  used as a conformance corpus for the ported rules.
- `tests/integration_test/repos_cache/` is gitignored; network-dependent checks are `#[ignore]`d.

### Configuration Files

- `.adrlint.toml` — per-project configuration
- `~/.config/adrlint/config.toml` — global user configuration
- `adrlint.lock` — the immutability manifest

### Build Artifacts

- `target/` — Cargo build output (gitignored)
- `.polylint/` — poly cache (gitignored)
