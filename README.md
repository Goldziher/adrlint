<div align="center">

<img src="assets/banner.svg" alt="adrlint — decisions, checked" width="820">

**Lint the record, not just the prose.**

adrlint is a fast Rust linter and formatter for [MADR](https://adr.github.io/madr/) architecture
decision records. It checks the decision structure, the directory's sequential numbering, and the
Markdown itself — then keeps accepted decisions immutable by hashing their content.

MADR-native&nbsp;·&nbsp;rumdl-powered&nbsp;·&nbsp;sequential numbering&nbsp;·&nbsp;immutability&nbsp;·&nbsp;single binary&nbsp;·&nbsp;CI-ready

[![crates.io](https://img.shields.io/crates/v/adrlint?style=flat-square&color=f59e0b)](https://crates.io/crates/adrlint)
[![npm](https://img.shields.io/npm/v/adrlint?style=flat-square&color=f59e0b&label=npm)](https://www.npmjs.com/package/adrlint)
[![PyPI](https://img.shields.io/pypi/v/adrlint?style=flat-square&color=f59e0b)](https://pypi.org/project/adrlint/)
[![CI](https://img.shields.io/github/actions/workflow/status/Goldziher/adrlint/ci.yml?style=flat-square&label=CI)](https://github.com/Goldziher/adrlint/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-f59e0b?style=flat-square)](./LICENSE)

[Install](#installation)&nbsp;·&nbsp;[Quick start](#quick-start)&nbsp;·&nbsp;[Rules](#rules)&nbsp;·&nbsp;[Configuration](#configuration)&nbsp;·&nbsp;[Locking](#immutability-and-locking)&nbsp;·&nbsp;[GitHub Action](#github-action)

</div>

---

## Why adrlint

An ADR is only useful if a reader can trust it. That means four things have to hold, and a generic
Markdown linter checks none of them:

1. **The record follows MADR** — the mandatory sections exist, sit at the right heading level, and
   appear in the template's order.
2. **The directory is a log** — numbers are unique, contiguous, and never renumbered.
3. **The prose is clean Markdown** — so it renders the same everywhere and survives review.
4. **Accepted decisions do not change** — a later edit is a new decision, not a quiet rewrite.

adrlint is built to check all four in one run, from one parse, with one exit code.

## Status

adrlint is at **0.1.0** and under active construction. The implemented surface is:

- `adrlint lint` — `MADR01`–`MADR08` structural rules, `MADR12`/`MADR21`/`MADR31` integrity and
  style rules, `MADR41`–`MADR44` directory rules, `ADR50`/`ADR51` immutability checks, and rumdl's
  `MD###` Markdown rules
- `adrlint format` — rumdl's fixes, with `--check` for CI
- `adrlint init` — writes `.adrlint.toml` and a starter ADR
- `adrlint new "<title>"` — creates the next sequential ADR
- `adrlint lock` / `adrlint unlock` — the content-hash immutability manifest

External link checking (`MADR11`) is the remaining MADR rule; it is opt-in because it needs network
access. The rule IDs and configuration keys are stable from the first release; nothing in this
README is a placeholder name.

## Features

- **MADR rules** — the rules from [`adr/madrlint`](https://github.com/adr/madrlint), with stable IDs
- **Markdown quality** — [rumdl](https://github.com/rvben/rumdl) linked in-process, not shelled out to
- **Sequential numbering** — start at `0000` or `0001`, no gaps, no collisions, no renumbering
- **Immutability** — accepted ADRs locked by a content hash, not by file timestamps
- **Formatting** — one formatter for the Markdown, idempotent by construction
- **Gate-ready** — non-zero exit on errors, GitHub annotations, JSON output
- **Single binary** — no runtime, no downloaded grammar, no bundled script
- **Configurable** — layered `.adrlint.toml` with per-rule Markdown severities

## Installation

| Channel                | Command                                          |
| ---------------------- | ------------------------------------------------ |
| Homebrew (macOS/Linux) | `brew tap goldziher/tap && brew install adrlint` |
| Cargo (Rust)           | `cargo install adrlint`                          |
| npm (Node.js)          | `pnpm add -g adrlint`                            |
| pip (Python)           | `pip install adrlint`                            |

Prefer prebuilt binaries? [`cargo binstall adrlint`](https://github.com/cargo-bins/cargo-binstall)
downloads a release archive instead of compiling.

Run without installing:

```bash
pnpm dlx adrlint@latest lint
uvx adrlint lint
```

<details>
<summary><b>Build from source</b></summary>

```bash
git clone https://github.com/Goldziher/adrlint.git
cd adrlint
cargo install --path .
```

Requires Rust 1.94+ (the rumdl MSRV).

</details>

## Quick start

```bash
# Create .adrlint.toml, docs/adr, and the first ADR
adrlint init

# Lint the configured ADR directory
adrlint lint

# Create the next ADR
adrlint new "Use PostgreSQL for event storage"

# Format the Markdown, then verify it in CI
adrlint format
adrlint format --check

# Lint with GitHub annotations
adrlint lint --format github
```

`adrlint` exits `0` when there are no errors, `1` when a rule reports an error, and `2` when the
command could not run (bad arguments, unreadable path, invalid configuration).

## CLI

```text
adrlint [--config PATH] [--no-color] <COMMAND>

Commands:
  lint    Lint ADR files and the directory that holds them
  format  Format ADR files with rumdl
  init    Write a starter configuration and ADR directory
  new     Create the next sequential ADR
  lock    Record canonical content hashes for accepted ADRs
  unlock  Remove one ADR from the immutability manifest

Options:
  --config <PATH>   Path to the configuration file
  --no-color        Disable coloured output
```

### `adrlint lint`

```text
adrlint lint [PATH]... [--format text|json|github]
```

With no paths, `lint` uses `adr.dir` from the configuration. With paths, each directory is linted in
full and each file is resolved to its parent directory — so a pre-commit hook that passes only the
changed files still sees the whole directory.

### `adrlint format`

```text
adrlint format [PATH]... [--check]
```

Resolves paths exactly like `lint`, then applies rumdl's fixes to each `.md` file. Only files that
change are written, and each is printed. `--check` writes nothing and exits `1` when a file would
change, which is the form to run in CI. Formatting is idempotent: a second run changes nothing.

### `adrlint init`

```text
adrlint init [--dir docs/adr] [--start 0|1] [--force]
```

Writes `.adrlint.toml`, creates the ADR directory, and adds
`NNNN-record-architecture-decisions.md` when the directory is empty. `--force` overwrites an existing
configuration; it never overwrites an ADR.

### `adrlint new`

```text
adrlint new "<title>"
```

Creates the next number with `O_EXCL`, so two concurrent invocations cannot claim the same file. The
new ADR starts as `proposed` and is not locked.

### `adrlint lock`

```text
adrlint lock [PATH]... [--check] [--all]
```

Lints first and refuses to lock a directory with errors. By default it locks `accepted` and
`rejected` ADRs; `--all` includes drafts. `--check` verifies the manifest without writing and exits
`1` when it is out of date.

### `adrlint unlock`

```text
adrlint unlock <PATH> --reason "<why>"
```

Removes one entry. The reason is printed for the commit message and the lock-file diff is the audit
trail. The ADR itself is not modified.

## Rules

Every MADR rule except external link checking (`MADR11`, opt-in and network-dependent) is
implemented in 0.1.0, along with `ADR50`/`ADR51` and the rumdl `MD###` rules.

Rule IDs are public API. Suppressing or re-severitying one in `.adrlint.toml` must keep working across
releases; a new check gets a new ID.

### Structural integrity (`MADR01`–`MADR08`)

| ID        | Check                                                        |
| --------- | ------------------------------------------------------------ |
| `MADR01a` | The title (H1) is present                                    |
| `MADR01b` | `Context and Problem Statement` is present                   |
| `MADR01c` | `Considered Options` (or an accepted alias) is present       |
| `MADR01d` | `Decision Outcome` is present                                |
| `MADR02`  | No section is empty                                          |
| `MADR03a` | `Decision Outcome` states the chosen option                  |
| `MADR03b` | The chosen option is the first content in `Decision Outcome` |
| `MADR04`  | The chosen option is followed by a rationale                 |
| `MADR05`  | Only the title uses heading level 1                          |
| `MADR06`  | Headings use the template's levels                           |
| `MADR07a` | `Consequences` is a level-3 heading under `Decision Outcome` |
| `MADR07b` | `Confirmation` is a level-3 heading under `Decision Outcome` |
| `MADR08a` | The title is at the top of the file                          |
| `MADR08b` | Level-2 sections appear in template order                    |

### Links (`MADR11`–`MADR12`)

| ID        | Check                                          |
| --------- | ---------------------------------------------- |
| `MADR11`  | External links resolve (opt-in; needs network) |
| `MADR12a` | Local relative paths exist                     |
| `MADR12b` | Anchor links match a heading in the document   |
| `MADR12c` | Root-relative paths exist                      |
| `MADR12d` | Absolute filesystem paths are not used         |

### Metadata (`MADR21`)

| ID       | Check                            |
| -------- | -------------------------------- |
| `MADR21` | YAML front matter parses as YAML |

### Style (`MADR31`)

| ID       | Check                          |
| -------- | ------------------------------ |
| `MADR31` | `*` is used as the list marker |

### Directory (`MADR41`–`MADR44`)

| ID        | Check                                        |
| --------- | -------------------------------------------- |
| `MADR41a` | The first number is `0000` or `0001`         |
| `MADR41b` | No number is missing between two ADRs        |
| `MADR42`  | No non-ADR file sits in the ADR directory    |
| `MADR43`  | No two ADRs share a number                   |
| `MADR44`  | File names match `NNNN-brief-description.md` |

### adrlint (`ADR50`–`ADR51`)

| ID      | Check                                                   |
| ------- | ------------------------------------------------------- |
| `ADR50` | A locked ADR's content hash differs from `adrlint.lock` |
| `ADR51` | A locked ADR is missing from disk                       |

### Markdown (`MD###`)

rumdl's rules are reported verbatim, so a suppression you already know keeps working. Every enabled
Markdown rule is an error by default, so `lint` gates on Markdown quality without extra configuration;
`[markdown.rules] MD009 = "warn"` downgrades one rule. Opinionated defaults: line length (`MD013`) and
inline HTML (`MD033`) are off, because ADR prose is reflowed and decision records legitimately embed
HTML tables. `MD004` is pinned to `*`, matching `MADR31`, so `format` and `lint` agree.

## Configuration

`.adrlint.toml` is discovered from the current directory upward. The nearest file wins; the global
file at `dirs::config_dir()/adrlint/config.toml` is the fallback.

```toml
[adr]
dir = "docs/adr"
start = 1 # 0 or 1; used by init and new
allow_gaps = false # only for imported histories

[lock]
enabled = true
path = "adrlint.lock"
statuses = ["accepted", "rejected"]

[markdown]
enabled = true

[markdown.rules]
MD013 = "off"
MD033 = "off"
```

`[markdown].enabled = false` removes every rumdl finding and every rumdl fix. `[markdown.rules]` takes
`off`, `warn` or `error`, keyed by rumdl's own code: `off` removes both the finding and the fix, while
`warn` and `error` set the finding's severity. A rule you do not name keeps its default — the shipped
ones turn `MD013` and `MD033` off.

The `[lint]` and `[rules]` tables land with the rules that consume them:

```toml
[lint]
check_external_links = false

[rules]
MADR03 = "error"
MADR12c = "warn"
```

Unknown keys are errors, not silently ignored.

## Immutability and locking

An accepted ADR is a historical record. adrlint can enforce that with a committed manifest:

```toml
version = 1
canonical = 1

[adrs."docs/adr/0001-record-architecture-decisions.md"]
number = 1
status = "accepted"
blake3 = "…"
```

- Creating `adrlint.lock` opts in. No file, no enforcement.
- `adrlint lock` covers `accepted` and `rejected` ADRs; `--all` includes drafts.
- `adrlint lint` verifies the entries that exist; new, unlocked ADRs are fine.
- `adrlint unlock <path> --reason <text>` removes one entry. The lock-file diff is the audit trail.
- The hash is over a **semantic serialization**, not the bytes on disk. Formatting, line endings and
  paragraph reflow do not change it; a changed word, heading, link target or code sample does.
- `adrlint lock --check` is the strict CI form; `adrlint lint` reports stale locks as errors.
- The `canonical` field versions that serializer, so a tool upgrade warns instead of failing every
  consumer's build.

The manifest is committed. It is reviewed like source; `ADR50` and `ADR51` are the only findings a
change to it can produce.

## Sequentiality

```text
docs/adr/
├── 0001-record-architecture-decisions.md
├── 0002-use-postgresql.md
└── 0003-split-billing-from-identity.md
```

Numbers are identities, not labels. adrlint never renumbers and never deletes to close a gap. A
withdrawn decision keeps its file with `status: withdrawn`; a gap is the signal that history was
edited. `adrlint new` is the only writer that assigns a number.

## Pre-commit

```yaml
repos:
  - repo: https://github.com/Goldziher/adrlint
    rev: v0.1.0
    hooks:
      - id: adrlint-lint
      - id: adrlint
      - id: adrlint-lock
```

The default `files` pattern covers `adr/`, `adrs/`, `decisions/`, `doc/adr/` and `docs/adr/`. Override
it in your config when the directory is elsewhere.

## GitHub Action

```yaml
name: ADR lint
on: [pull_request]
jobs:
  adrlint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: Goldziher/adrlint@v0
        with:
          args: "lint --format github"
```

The action downloads the release binary matching the runner, verifies its checksum, caches it, and
registers a problem matcher so findings appear inline on the diff.

## How it works

1. **Discover** the ADR directory and parse each `NNNN-slug.md` into a directory model.
2. **Parse** Markdown once with pulldown-cmark, retaining byte offsets and front matter.
3. **Check** MADR structure and numbering from that model; run rumdl on the same source.
4. **Merge** findings, deduplicate, sort, and report as text, JSON or GitHub annotations.
5. **Lock** by hashing a deterministic serialization of the document's content.

`lint` never writes. Fixes belong to `format`, and only `lock`/`unlock` touch the manifest.

## Roadmap

- [x] Directory rules, `lint`, `init`, `new`
- [x] Markdown parser and document model
- [x] `MADR01`–`MADR08` structural rules
- [x] `MADR12`, `MADR21`, `MADR31` link, metadata and style rules
- [x] `adrlint lock` / `unlock` with semantic hashing
- [x] rumdl diagnostics and `adrlint format`
- [ ] External link checking (`MADR11`, opt-in)
- [ ] `[lint]` and `[rules]` configuration tables
- [ ] 1.0 release

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Rules ship with fixtures under `fixtures/adr/`, and a bug
report should include the smallest ADR that reproduces it.

## License

MIT — see [LICENSE](./LICENSE).
