---
type: Skill
title: Architecture
description: Use when changing adrlint's Rust modules, adding a rule family, or touching the release, packaging and corpus scripts. Maps the source layout and the distribution channels.
x-ai-rulez:
  kind: skill
  id: architecture
  metadata:
    name: architecture
---

## Project Architecture

adrlint is a Rust CLI linter and formatter for Architecture Decision Records written in Markdown
following the [MADR](https://github.com/adr/madr) specification. It is distributed through crates.io,
npm, PyPI, Homebrew and Scoop, using the same binary-wrapper pattern as the other Goldziher Rust CLIs.

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

One module per concern; every module in the layout is implemented.

- Entry and wiring — `main.rs` (dispatch, atomic writes, exit codes), `cli.rs` (clap), `config.rs`
  (layered `.adrlint.toml`, rendering, validation), `discovery.rs` (scanning an ADR directory).
- `adr/` — the document model and the **only** parse: `mod.rs` (front matter, headings, blocks,
  links, list items), `parse.rs` (one pulldown-cmark pass, byte offsets retained), `front_matter.rs`
  (YAML extraction, BOM handling, parser bounds).
- `markdown/` — the Markdown layer: `mod.rs` facade, `rumdl.rs` (rule set, diagnostics and fixes
  adapted to adrlint findings).
- `rules.rs` plus `rules/` split by family — `structure.rs` (`MADR01`–`MADR08`), `links.rs`
  (`MADR12`, slugs), `metadata.rs` (`MADR21`), `style.rs` (`MADR31`). `rules.rs` owns dispatch,
  severity overrides and the code registry.
- The rest — `lock.rs` (canonical hashing, `adrlint.lock`, `ADR50`/`ADR51`), `fix.rs` (structural
  fixes for `lint --fix`), `parallel.rs` (deterministic parallel map), `diagnostics.rs` (finding
  model, severity, ordering), `report.rs` (text, JSON, GitHub annotations), `template.rs` (starter
  ADR and file-name slugs).

### Distribution

- `npm-package/` and `pip-package/` — wrappers that download the release binary for the platform
  (the pip one caches it under `adrlint/downloader.py`).
- `.github/workflows/publish.yaml` — native per-platform release build matrix.
- `scripts/update-homebrew-formula.sh` and `scripts/update-scoop-manifest.sh` — regenerate the tap
  formula and the Scoop manifest from the release checksums.
- Root `action.yml` — composite GitHub Action that installs and runs `adrlint lint`; the install
  logic lives in `scripts/gh-action-*.sh` and annotations come from `problem-matcher.json`.

### Testing

- Unit tests are co-located in source files under `#[cfg(test)]`; integration tests live in `tests/`.
- `fixtures/adr/` holds the valid and invalid MADR fixtures, one directory per rule code.
- `fixtures/corpus/` holds pinned real-world ADRs (madrlint, adr-manager, log4brains) and a generated
  matrix of MADR shapes; the madrlint samples used as the conformance corpus for the ported rules are
  in `fixtures/corpus/madrlint`. `tests/corpus.rs` exercises parsing, formatting, hash stability,
  truncated input and the CLI over the whole tree. Tests run offline;
  `scripts/fetch-corpus.sh` refreshes the vendored trees at their pinned commits.

### Configuration Files

- `.adrlint.toml` — per-project configuration
- `~/.config/adrlint/config.toml` — global user configuration
- `adrlint.lock` — the immutability manifest

### Build Artifacts

- `target/` — Cargo build output (gitignored)
- `.polylint/` — poly cache (gitignored)
