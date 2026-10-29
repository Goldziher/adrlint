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

The 0.2.1 release implements every module in the layout below. `markdown/` links rumdl in-process;
`adr/` owns the single parse; `rules/` is split by family; `lock.rs` owns content hashing.

- `main.rs` — entry point, command dispatch, atomic writes and exit codes
- `cli.rs` — clap command and flag definitions
- `config.rs` — layered `.adrlint.toml` and global configuration, rendering and validation
- `discovery.rs` — scanning an ADR directory, numeric prefixes and file-name validity
- `adr/mod.rs` — the document model: front matter, headings, blocks, links, list items
- `adr/parse.rs` — the single pulldown-cmark parse, byte offsets retained
- `adr/front_matter.rs` — YAML front matter extraction, BOM handling and parser bounds
- `markdown/mod.rs` — the Markdown layer facade
- `markdown/rumdl.rs` — rumdl rule set, diagnostics and fixes adapted to adrlint findings
- `rules.rs` — document and directory rule dispatch, severity overrides, code registry
- `rules/structure.rs` — `MADR01`–`MADR08`
- `rules/links.rs` — `MADR12` and slug generation
- `rules/metadata.rs` — `MADR21`
- `rules/style.rs` — `MADR31`
- `lock.rs` — canonical content hashing, `adrlint.lock`, `ADR50`/`ADR51`
- `diagnostics.rs` — finding model, severity and ordering
- `report.rs` — text, JSON and GitHub annotations output
- `template.rs` — the starter ADR and file-name slugs

### Distribution

- `npm-package/` — npm wrapper (`adrlint`) that downloads the release binary
- `pip-package/` — PyPI wrapper (`adrlint`) with the same binary-download strategy
- `.github/workflows/publish.yaml` — native per-platform release build matrix
- `scripts/update-homebrew-formula.sh` — regenerates the Homebrew formula from release checksums
- Root `action.yml` — composite GitHub Action that installs and runs `adrlint lint`

### Testing

- Unit tests are co-located in source files under `#[cfg(test)]`.
- Integration tests live in `tests/`.
- `fixtures/adr/` holds the valid and invalid MADR fixtures, one directory per rule code, plus the
  `adr/madrlint` samples used as the conformance corpus for the ported rules.
- `fixtures/corpus/` holds pinned real-world ADRs (madrlint, adr-manager, log4brains) and a generated
  matrix of MADR shapes. `tests/corpus.rs` exercises parsing, formatting, hash stability, truncated
  input and the CLI over the whole tree. Tests run offline; `scripts/fetch-corpus.sh` refreshes the
  vendored trees at their pinned commits.

### Configuration Files

- `.adrlint.toml` — per-project configuration
- `~/.config/adrlint/config.toml` — global user configuration
- `adrlint.lock` — the immutability manifest

### Build Artifacts

- `target/` — Cargo build output (gitignored)
- `.polylint/` — poly cache (gitignored)
