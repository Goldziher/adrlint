---
type: Concept
title: MADR Model
x-ai-rulez:
  kind: context
  id: madr-model
  metadata:
    priority: high
---

## MADR Model

The MADR template is the specification, not a suggestion. adrlint implements the rules of
[`adr/madrlint`](https://github.com/adr/madrlint) and the current
[MADR template](https://github.com/adr/madr/blob/develop/template/adr-template.md); when the two
disagree, the template wins and the divergence is documented in the rule.

### Document shape

An ADR is a Markdown file with optional YAML front matter and exactly one H1 title. The body is a
sequence of H2 sections:

| Section                       | Level | Required | Order                  |
| ----------------------------- | ----- | -------- | ---------------------- |
| Title                         | 1     | yes      | first                  |
| Context and Problem Statement | 2     | yes      | 1                      |
| Decision Drivers              | 2     | no       | 2                      |
| Considered Options            | 2     | yes      | 3                      |
| Decision Outcome              | 2     | yes      | 4                      |
| Pros and Cons of the Options  | 2     | no       | 5                      |
| More Information              | 2     | no       | 6                      |
| Consequences                  | 3     | no       | under Decision Outcome |
| Confirmation                  | 3     | no       | under Decision Outcome |

`Context`, `Context and Problem`, `Considered Alternatives`, `Explored Options`, `Information` and
the other aliases accepted by madrlint are accepted here too. The canonical order above is what
`MADR08b` enforces.

### Decision outcome

The first content in `Decision Outcome` must be a chosen option:

```markdown
Chosen option: "<option>", because <rationale>
```

`Chosen Option`, `Chosen Alternative` and `Selected Option` are accepted; a rationale introduced by
`because`, `Rationale`, `Reasoning`, `Reason`, `Justification`, `Motivation` or `As` must follow it.

### Rule IDs

- `MADR01`–`MADR08` — structural integrity, ported from madrlint unchanged
- `MADR11`–`MADR12` — external and local link validity
- `MADR21` — YAML front matter syntax
- `MADR31` — `*` as the list marker
- `MADR41`–`MADR44` — directory numbering, naming and foreign files
- `ADR50`–`ADR51` — adrlint-specific immutability rules
- `MD###` — rumdl Markdown rules, reported verbatim

An ID is public API: renaming one breaks suppressions in user configuration. Add a new ID instead of
changing an existing one. The sets (`MADR01a`, `MADR01b`, …) are separately disableable.

### Fixtures

`fixtures/adr/valid/` must pass every rule. `fixtures/adr/invalid/<rule-id>/` holds one file per
violation, so a rule change that breaks a fixture names the rule in the path. The madrlint
`madr-samples/` corpus is the conformance baseline for the ported rules.
