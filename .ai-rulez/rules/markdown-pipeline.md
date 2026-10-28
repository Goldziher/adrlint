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

- `markdown::parse` owns the only parse: pulldown-cmark with offset iterators enabled, producing a
  document model plus byte ranges.
- Front matter is stripped and parsed as YAML before the Markdown parse; its line count shifts every
  following offset, so the shift is applied once in the model rather than in each rule.
- Offsets are **byte** offsets. Slicing a multi-byte file at a character index panics.
- The model exposes headings with level, text, line and byte span; sections between headings; list
  items with their marker; links with destination and line; and the raw front matter.

### rumdl integration

rumdl is linked in-process through `rumdl_lib`; it is not shelled out to and there is no runtime
download.

- `adrlint lint` runs rumdl over the same source and converts each diagnostic into an adrlint finding
  carrying rumdl's own `MD###` code, so users can suppress it with the code they already know.
- `adrlint format` applies rumdl fixes and MADR fixes to one buffer. Formatting is idempotent: a
  second run changes nothing.
- Markdown findings are disabled wholesale with `[markdown].enabled = false`; individual rules are
  configured with `[markdown.rules]`.
- Defaults are opinionated: MD013 (line length) is off because ADR prose is reflowed by authors, and
  MD033 (inline HTML) is off because decision records legitimately embed HTML tables. Do not silently
  change a rumdl default; write the override in the generated config.
- rumdl's MSRV is adrlint's MSRV. Bumping rumdl can alter formatting, but locking hashes content
  rather than formatted bytes, so a formatter bump never invalidates a lock.

### Diagnostic merging

- Deduplicate on `(path, byte offset, code)`, then sort by path, offset and code.
- A finding carries a stable rule code, severity, message, path, line, column and optional fix.
- `lint` never writes. Fixes are applied only by `format`, after the full finding set is computed.
- Never locate a construct by searching the rendered text. Regex over a node's text, once the parse
  has handed it over, is fine; finding structure by regex is not.

### Links

External link checking is opt-in (`[lint].check_external_links = false` by default): it needs network
access and makes a lint run non-deterministic. When enabled, requests run in parallel with a timeout
and a stable result order. Local paths, root-relative paths and anchor links are always checked
offline; anchors are matched against slugs generated from the parsed heading tree, not guessed.

### Tests

- Parse fixtures cover front matter, ATX and setext headings, nested lists, fenced code with blank
  lines, tables, reference links, images and HTML.
- A round-trip test asserts that format is idempotent over every fixture.
- A lock test asserts that reflowing a paragraph and changing list markers leaves the hash unchanged,
  while changing one word or a link destination changes it.
