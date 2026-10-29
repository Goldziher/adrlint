---
type: Decision
title: Sequentiality and Numbering
x-ai-rulez:
  kind: rule
  id: sequentiality-numbering
  metadata:
    priority: critical
---

## Sequentiality and Numbering

An ADR directory is an append-only, ordered log. The number is the identity of the decision, not a
label of convenience, so numbering rules are errors and are never auto-fixed.

### Invariants

- File names are `NNNN-brief-description.md`: exactly four digits, a hyphen, an ASCII kebab-case slug
  (letters, digits and single hyphens), then `.md`. Upper-case acronyms are allowed.
- The first ADR is `0000` or `0001`, selected once by `[adr].start` and then fixed.
- Numbers are contiguous. `MADR41b` reports the missing number and the file it should precede.
- Numbers are unique. `MADR43` reports every file in a collision, not just the later one.
- Only ADR Markdown files live in the directory. `MADR42` reports anything else that is not an
  allowed asset (images referenced by an ADR).
- Ordering uses the numeric prefix, so zero padding is load-bearing: `0010` sorts after `0009`.

### Never renumber

Renumbering rewrites every reference and every link, and changes the identity of decisions that may
already be cited externally. When a decision is withdrawn, keep its file and set `status: withdrawn`
(or `rejected`); do not delete it and leave a gap. A gap is the linter's signal that history was
edited, and the fix is to restore the record, not to close the gap.

`[adr].allow_gaps` exists only for imported histories that legitimately start with holes. It is off
by default and turning it on should be a documented, one-time migration.

### Namespaces

A repository may hold several independent logs, for example `adrs/team-a/`, `adrs/team-b/` and
`product/adrs/api/`. Numbering is checked per ADR directory, never across them: two namespaces may
both start at `0001`. Globs and `--recursive` select the logs; the rules never merge them.

### Creating the next ADR

`adrlint new "<title>"` computes the next free number, then creates the file with `O_EXCL` so two
concurrent invocations cannot claim the same number. It writes the configured MADR template with the
title and today's date in the front matter. It never overwrites an existing file; a collision is
reported and the command exits non-zero.

`adrlint new` does not lock the file. A new ADR is `proposed` and editable; `adrlint lock` covers it
once it reaches `accepted`.

### Rules

- `MADR41a` — smallest number is neither `0000` nor the configured base.
- `MADR41b` — a number between two existing ADRs is missing.
- `MADR42` — a non-ADR file is present in the ADR directory.
- `MADR43` — two or more files share a number.
- `MADR44` — a file name does not match `NNNN-brief-description.md`.

### Guidelines

- Put the check in the directory model, not in each rule: one pass builds `Vec<AdrFile>` sorted by
  number, and every numbering rule reads that.
- Report a collision once per colliding file, with the same message and code, so suppression by code
  is predictable.
- Treat a file that does not parse a number as `MADR44` only; do not let it crash the numbering pass.
- Tests must cover `0000`, `0001`, a 3-digit name, a 5-digit name, an uppercase slug, a duplicate,
  a gap, and a non-Markdown asset.
