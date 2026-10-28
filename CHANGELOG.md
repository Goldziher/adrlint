# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

- A single Markdown parse per ADR (`src/adr/`), with front matter, headings and section spans, list
  items, code blocks, tables, links, images and byte offsets.
- `MADR01`–`MADR08` structural rules, ported from madrlint: mandatory sections, empty sections,
  chosen-option and rationale wording, heading levels, nesting and section order.
- `MADR12a`–`MADR12d` local link checks, `MADR21` front matter validation and `MADR31` asterisk list
  markers. External link checking is the remaining MADR rule and stays opt-in.
- rumdl 0.3 linked in-process for `MD###` findings, and `adrlint format` with `--check`. Markdown
  findings are errors by default and can be downgraded per rule.
- `ADR50` / `ADR51` immutability rules and `adrlint lock` / `adrlint unlock`.
- `adrlint.lock` hashes a canonical content serialization, not raw or formatted bytes; list markers,
  heading style, line wrapping, CRLF and reordered front matter keys do not break a lock.
- `adrlint lint` now reports structural and locking findings alongside the directory rules.
- `[lock]` configuration with `enabled`, `path` and `statuses`.
- `[markdown]` configuration with `enabled` and per-rule `off`/`warn`/`error` settings. `MD013` and
  `MD033` are off; `MD004` is pinned to `*` so formatting cannot produce a MADR31 violation.

### Changed

- `adrlint init` writes the `[lock]` table and a stable starter configuration.
- `adrlint lock --check` is the strict CI form; `adrlint lock` lints first and refuses to freeze an
  invalid ADR.
