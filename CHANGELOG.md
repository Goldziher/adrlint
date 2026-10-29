# Changelog

All notable changes to this project are documented in this file.

## Unreleased

### Added

- Zero-configuration discovery: with no paths, no `[adr].paths`, no configured `[adr].dir` and no
  `docs/adr`, `lint`, `format` and `lock` search the project for ADR logs, so `adrs/<area>/` folders
  and a service's own nested `adrs/` work out of the box. A configured `dir` that is missing is still
  an error, and finding no log at all is reported.
- `[adr].recursive`, so configured roots are searched for nested logs without passing `--recursive`.
- A Lefthook example in the README.
- `adrlint lint --fix` (with `--force`): applies rumdl's fixes plus the unambiguous MADR heading-level
  fix for `Consequences` and `Confirmation`, respects the immutability lock like `format`, then lints
  again and reports what remains.
- Richer diagnostics: end positions, a short `help` hint and a `fixable` flag on findings, a quoted
  source line with an underline in text output, `endLine`/`endColumn` in GitHub annotations, and
  `end_line`, `end_column`, `help` and `fixable` in JSON (all existing fields are unchanged).
- A closing summary line on stderr in text mode: errors, warnings, files checked and fixable count.
- Opt-in external link checking, `MADR11`: `[lint].check_external_links` or
  `lint --check-external-links` (`--no-check-external-links` overrides the configuration) requests
  each `http(s)` link once per run, HEAD then GET, with a 10 s timeout. Findings are warnings unless
  `[rules] MADR11` says otherwise; `429` is not reported, and `format` and `lock` never use the network.
- Hardening tests with synthetic monorepos: nested logs at several depths, ignored build output,
  symlink loops, unreadable and non-UTF-8 files, very large ADRs, CRLF, front matter edge cases and
  thousands of directories.

### Changed

- `format` and `lint --fix` rewrite numbered ADRs only; a README or template inside an ADR directory
  is still linted but never reformatted, and its findings are not marked fixable.

- Directories are listed and ADRs checked in parallel, each file is read once and the Markdown rule
  set is built once per worker thread; output is sorted, so it is byte-identical across runs.
  Inputs that reach the same directory are merged before linting, so each file is checked once.
- Discovery walks once: the directories found for zero-config runs are reused, and the directories
  of overlapping recursive roots are merged. A path the walk cannot read is a stderr warning instead of an abort.
- An ADR larger than 16 MiB is reported as `ADR52` instead of being parsed.
- Discovery walks the project in parallel and honours `.gitignore` (plus hidden directories,
  `target` and `node_modules`), so a monorepo root takes about two seconds instead of ten.

### Fixed

- `lint --fix` and `format` plan every file before writing any: a file that cannot be read or a
  symlinked ADR means no file is written, while a fix refused on a locked ADR is reported and the
  other files are still written. A file edited after it was planned is reported and not rewritten,
  and a rewritten file keeps its permissions. `lint --fix` rewrites only numbered ADRs, never a
  README or template inside a log.
- A file a hook passes that the commit deletes lints the log it leaves instead of aborting the run.
- A recursive root with no ADR log under it, passed on the command line or configured with
  `[adr].recursive`, is reported instead of passing with nothing checked.
- `lint --fix` and `format` given only files (as a hook passes its staged files) rewrite only those
  files, never a sibling ADR with unstaged edits, and a symlinked ADR stops the run before any write.
- A mistyped path is an error again; only a missing `.md` file (one a commit deletes) is skipped.
- A path that exists is read literally even if it holds `[`, and GitHub annotations escape `:` and `,`
  in file names. Text and GitHub spans end at the end of the line's content.
- `lint --fix` exits 1 when it refused to change a locked ADR.
- `MADR42` exempts an image (`png`, `jpg`, `jpeg`, `gif`, `svg`, `webp`) that an ADR in the same
  log links to or embeds; an unreferenced image is still reported.
- `lock` honours `--no-color` when it prints the errors that stop it.
- A manifest from another canonical version no longer reports `ADR51` for missing files: a stale
  manifest verifies nothing until `adrlint lock --all` refreshes it.
- A `[rules]` key naming a rumdl code (`MD009 = "off"`) is no longer reported as matching no rule,
  since it does apply; a `[markdown.rules]` key is checked against rumdl's actual rule codes, so
  `MD999` is reported.
- `adr.dir`, `adr.paths` and `lock.path` reject a Windows drive (`C:\x`) or UNC (`\\host\share`) path
  on every platform, with the same check the lock manifest's keys use.
- Text output replaces control characters in the quoted source line, the message, the help and
  the path with U+FFFD, so an escape sequence or carriage return in an ADR cannot rewrite the
  terminal or forge a CI log line.
- Worker threads get a 16 MiB stack, so a deeply nested ADR cannot overflow one. A single file on a
  single core runs on a worker too, and a thread that cannot be started is an error, not a panic.
- `lock` refuses an ADR that resolves outside the project root (a log reached through a symlink)
  instead of writing a manifest key that every later run rejects.
- Findings and fix messages print paths relative to the working directory when they lie under
  it, so GitHub annotations resolve and Windows output carries no `\\?\` prefix.
- A configured glob such as `adrs/*` works in a project whose path contains `[`, `]`, `*` or `?`.
- `format` and `lint --fix` refuse a fix to a locked ADR only when the fix itself changes its
  canonical content: an ADR that already drifted can still be reflowed, and a manifest from another
  canonical version no longer lets a content-changing fix through without `--force`.

- A log made only of three-digit (`001-x.md`) or five-digit (`00001-x.md`) names is discovered,
  so its files reach `MADR44` instead of being skipped.
- A date-prefixed document (`2026-07-01-design.md`) no longer makes its directory an ADR log, so
  `--recursive .` stops reporting plans and specs as ADRs.
- A Markdown file passed explicitly that is not an ADR and sits outside any ADR log (`adrs/README.md`,
  an ADR template) is skipped, so pre-commit and Lefthook hooks that pass every changed file under
  `adrs/` do not fail on the index or the template.

## 0.2.1 - 2026-10-29

### Fixed

- `--recursive` no longer treats the traversal root as an ADR log, so `adrlint lint --recursive .`
  does not report the repository README or `.adrlint.toml` as foreign ADR files. A directory is
  scanned only when it directly holds a numbered ADR; the foreign-file rule still applies inside a
  discovered log.

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
