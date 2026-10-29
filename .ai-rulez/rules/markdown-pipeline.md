---
type: Decision
title: Markdown Pipeline
x-ai-rulez:
  kind: rule
  id: markdown-pipeline
  metadata:
    priority: high
---

## Markdown Pipeline

A document is parsed once and every consumer works from that parse. MADR rules, rumdl integration,
formatting and lock hashing must not re-read the file or disagree about what a heading is.

### Parsing

- `adr::parse` (`src/adr/parse.rs`) owns the only parse: pulldown-cmark with offset iterators enabled,
  producing a document model plus byte ranges. `markdown::` only wraps rumdl.
- Front matter is stripped and parsed as YAML before the Markdown parse; its line count shifts every
  following offset, so the shift is applied once in the model rather than in each rule.
- Offsets are **byte** offsets. Slicing a multi-byte file at a character index panics.
- The model exposes headings with level, text, line and byte span; sections between headings; list
  items with their marker; links with destination and line; and the raw front matter.

### rumdl integration

rumdl is linked in-process through `rumdl_lib`; it is not shelled out to and there is no runtime
download.

- `adrlint lint` runs rumdl over the same source and converts each diagnostic into an adrlint finding
  carrying rumdl's own `MD###` code, so users can suppress it with the code they already know. An
  enabled Markdown rule is an error by default; `[markdown.rules] <CODE> = "warn"` downgrades it.
- `adrlint format` applies rumdl fixes to one buffer. Formatting is idempotent: a second run changes
  nothing. A fix that changes the canonical content of a locked ADR is refused unless `--force` is
  passed, so formatting cannot manufacture an `ADR50`.
- Markdown findings are disabled wholesale with `[markdown].enabled = false`; individual rules are
  configured with `[markdown.rules]`.
- Defaults are opinionated: MD013 (line length) is off because ADR prose is reflowed by authors, and
  MD033 (inline HTML) is off because decision records legitimately embed HTML tables. MD004 is pinned
  to `*` so the formatter cannot choose a marker MADR31 rejects; the pin is hard-coded in
  `Engine::new` (`src/markdown/rumdl.rs`), not written into the generated config. Do not silently
  change a rumdl default; write the override in the generated config.
- rumdl's MSRV is adrlint's MSRV. Bumping rumdl can alter formatting, but locking hashes content
  rather than formatted bytes, so a formatter bump never invalidates a lock.

### Diagnostic merging

- Sort by path, line, column and code. Rule families emit disjoint code sets, so a duplicate
  `(path, line, code)` is a rule bug rather than something to hide by de-duplicating.
- A finding carries a stable rule code, severity, message, path, line and column. MADR/ADR findings
  start at column 1 and `diagnostics::locate` (`src/diagnostics.rs`) moves the column to the first
  non-whitespace character of the line and adds an end span; rumdl supplies its own column.
- `lint` writes only with `--fix`. Fixes are applied by `format` and `lint --fix`, after the full
  finding set is computed.
- Never locate a construct by searching the rendered text. Regex over a node's text, once the parse
  has handed it over, is fine; finding structure by regex is not.

### Links

External link checking (`MADR11`) is opt-in, through `[lint].check_external_links = true` or
`lint --check-external-links`, and its findings default to warning. It is off by default because it
needs network access and makes a lint run non-deterministic. Local paths, root-relative paths and
anchor links are always checked offline; anchors are matched against slugs generated from the parsed
heading tree, not guessed. A protocol-relative destination (`//host`) is external, never a
root-relative path.

### Tests

- Parse fixtures cover front matter, ATX and setext headings, nested lists, fenced code with blank
  lines, tables, reference links, images and HTML.
- A round-trip test asserts that format is idempotent over every fixture.
- A lock test asserts that reflowing a paragraph and changing list markers leaves the hash unchanged,
  while changing one word or a link destination changes it.
