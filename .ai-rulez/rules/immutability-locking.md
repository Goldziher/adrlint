---
type: Decision
title: Immutability and Locking
x-ai-rulez:
  kind: rule
  id: immutability-locking
  metadata:
    priority: critical
---

## Immutability and Locking

An accepted ADR is a historical record. Editing it in place destroys the audit trail that makes ADRs
useful, so `adrlint` can enforce immutability at lint time rather than relying on review discipline.

### The lock file

`adrlint.lock` is a sorted TOML manifest committed next to the ADRs:

```toml
version = 1
canonical = 1

[adrs."docs/adr/0001-use-rust.md"]
number = 1
status = "accepted"
blake3 = "…"
```

- **Presence is opt-in.** No lock file means no enforcement. `adrlint lock` creates it.
- **Entry-scoped.** `lint` verifies the entries that exist; an ADR added later is not a violation
  until it is locked.
- **Default scope is `accepted` and `rejected`.** `adrlint lock` refuses to lock `proposed` unless
  `--all` is passed. A draft is expected to change.
- **Keyed by path, carrying the ADR number.** A moved file is reported as a missing entry plus an
  unlocked file, not silently accepted as an edit.

### Hash content, not formatting

The hash is computed over a deterministic serialization of the parsed document — never over the bytes
on disk, and never over a formatted rendering. A renderer still encodes formatting choices (list
markers, heading style, line wrapping), so the input is the document model instead:

**Included:** front matter (canonical key order, typed values), the heading tree and its levels,
paragraph text with soft wraps collapsed, hard breaks, list nesting and item text, code block language
and exact body, table content, links (text, destination, title), images, and raw HTML.

**Excluded:** list markers, heading style (ATX/setext), emphasis markers, link style, blank-line
counts, trailing whitespace, table alignment, fence style, and line wrapping.

`adrlint lock` builds the model in memory and never writes. `adrlint lint` does the same for each
locked file before comparing. Consequences:

- `adrlint format`, rumdl formatting and a manual reflow never break a lock.
- A CRLF checkout and an LF checkout produce the same hash.
- Changing a word, a heading, a link target or a code sample does break it. A hard line break is
  content; a soft wrap is not.
- Front matter is content: changing `status` requires an unlock.

The lock stores the `canonical` version that produced it. When the serializer changes, `lint` reports
a lock-version warning and asks for `adrlint lock` rather than failing every ADR — a release must
never turn every consumer's build red. `adrlint lock --check` is the strict form used in CI.

### Unlocking

`adrlint unlock <path> --reason <text>` removes one entry and prints the reason for the commit
message. It does not edit the ADR. The subsequent `git diff` of `adrlint.lock` is the audit trail:
the removal and the re-lock are reviewable on their own.

Superseding a decision normally means writing a new ADR that references the old one, leaving the old
entry locked. Editing the old ADR's `status` or adding a `superseded-by` link requires an unlock,
because that is a change to a historical document.

### Rules

- `ADR50` — a locked ADR's canonical hash differs from the manifest.
- `ADR51` — a locked ADR is missing from disk.

Both are errors and cannot be fixed by `adrlint format`.

### Guidelines

- Never write a lock entry for a file that fails `MADR01`–`MADR44`; locking an invalid ADR freezes the
  defect. `adrlint lock` lints first and refuses on errors.
- Keep the manifest sorted by path and the format stable; it is reviewed like source.
- Do not add per-entry timestamps. They churn the file without adding provenance that git does not
  already carry.
- Hashing is a gate, not a signature. It proves the file is unchanged, not who approved it; approval
  provenance belongs to code review or signed commits.
