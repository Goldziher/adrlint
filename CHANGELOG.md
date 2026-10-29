# Changelog

All notable changes to this project are documented in this file.

## 0.2.0 - 2026-10-29

### Added

- `--recursive` for `lint`, `format` and `lock`, so ADR logs nested under namespaces
  (`adrs/team-a/`, `product/adrs/api/`) are found without listing every directory.
- Files, directories and glob patterns are accepted as paths by every command. A file is checked
  with its parent directory so numbering stays intact.
- `[adr].paths` configures the default input set as directories or globs, falling back to `adr.dir`.
- A documented library surface and root re-exports (`parse_adr`, `check_document`,
  `check_markdown`, `format_markdown`, `canonical_hash`, `load`) for consumers such as poly.
- A Scoop manifest, published by the release workflow to `Goldziher/scoop-bucket`.

### Changed

- Windows releases are built natively as `x86_64-pc-windows-msvc`; the GNU cross-build could not
  link because rumdl depends on `tikv-jemallocator` on every non-msvc target.

## 0.1.0 - 2026-10-29

### Added

- A single Markdown parse per ADR (`src/adr/`), with front matter, headings and section spans, list
  items, code blocks, tables, links, images and byte offsets.
- `MADR01`–`MADR08` structural rules, ported from madrlint: mandatory sections, empty sections,
  chosen-option and rationale wording, heading levels, nesting and section order.
- `MADR12a`–`MADR12d` local link checks, `MADR21` front matter validation and `MADR31` asterisk list
  markers. External link checking is the remaining MADR rule and stays opt-in.
- rumdl 0.3 linked in-process for `MD###` findings, and `adrlint format` with `--check`. Markdown
  findings are errors by default and can be downgraded per rule.
- `ADR50` / `ADR51` immutability rules, `ADR52` for an unreadable or non-UTF-8 ADR, and
  `adrlint lock` / `adrlint unlock`.
- `adrlint.lock` hashes a canonical content serialization, not raw or formatted bytes; list markers,
  heading style, line wrapping, CRLF, code-fence info strings and reordered front matter keys do not
  break a lock.
- `adrlint lint` now reports structural and locking findings alongside the directory rules.
- `[lock]` configuration with `enabled`, `path` and `statuses`.
- `[markdown]` configuration with `enabled` and per-rule `off`/`warn`/`error` settings. `MD013` and
  `MD033` are off; `MD004` is pinned to `*` so formatting cannot produce a MADR31 violation.
- `[rules]` severity overrides for MADR and ADR codes, matched by longest prefix, so a set such as
  `MADR03` covers every variant and an explicit `MADR03b` still wins.

- A pinned ADR corpus (madrlint, adr-manager, log4brains) and a generated MADR matrix, with
  corpus-wide tests for parsing, formatting idempotence, hash stability and truncated input.
- `[rules]` and `[markdown.rules]` keys that match no implemented rule are reported once at startup.
- Configuration layers merge per key: global file, ancestors outward, nearest project file last.

### Changed

- `adrlint format` refuses a fix that changes a locked ADR's canonical content unless `--force` is
  passed; formatting-only fixes are still written.
- `adrlint init` writes the `[lock]` table and a stable starter configuration.
- Writes are atomic and refuse symlinks; `adrlint new` rejects control characters and over-long
  slugs, and `adrlint init` escapes its directory in the rendered TOML.
- A non-UTF-8 or unreadable ADR is reported as `ADR52` and no longer aborts the run.
- Release builds are gated on formatting, clippy, tests and `cargo deny`; every action is pinned to a
  commit SHA and tag-derived values reach shell scripts only through `env`.
- The npm and pip wrappers re-verify a cached binary and cap the download size; the Action extracts
  only the expected binary.
- `adrlint lock --check` is the strict CI form; `adrlint lock` lints first and refuses to freeze an
  invalid ADR.
