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

[Install](#installation)&nbsp;·&nbsp;[Quick start](#quick-start)&nbsp;·&nbsp;[Integrations](#integrations)&nbsp;·&nbsp;[Rules](#rules)&nbsp;·&nbsp;[Configuration](#configuration)&nbsp;·&nbsp;[Locking](#immutability-and-locking)&nbsp;·&nbsp;[GitHub Action](#github-action)

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

adrlint checks all four in one run, from one parse, with one exit code.

## Status

adrlint is at **0.4.0**. The rule IDs and configuration keys are stable; nothing in this README is a
placeholder name. The implemented surface is:

- `adrlint lint` — `MADR01`–`MADR08` structural rules, `MADR11`/`MADR12`/`MADR21`/`MADR31` link,
  metadata and style rules, `MADR41`–`MADR44` directory rules, `ADR50`–`ADR52` immutability and
  readability checks, and rumdl's `MD###` Markdown rules
- `adrlint format` — rumdl's fixes, with `--check` for CI; `adrlint lint --fix` adds the safe MADR
  structural fixes and lints again
- `adrlint init` — writes `.adrlint.toml` and a starter ADR
- `adrlint new "<title>"` — creates the next sequential ADR
- `adrlint lock` / `adrlint unlock` — the content-hash immutability manifest

New in 0.4.0: a poly hook catalog (`poly-hooks.toml`, see [docs/poly-hooks.md](docs/poly-hooks.md)); `README.md`
and `index.md` in a log are no longer linted as ADRs; a `superseded by NNNN` status locks as superseded; each
path is checked under the `.adrlint.toml` of the project that owns it.

## Features

- **MADR rules** — the rules from [`adr/madrlint`](https://github.com/adr/madrlint), with stable IDs
- **Markdown quality** — [rumdl](https://github.com/rvben/rumdl) linked in-process, not shelled out to
- **Sequential numbering** — start at `0000` or `0001`, no gaps, no collisions, no renumbering
- **Immutability** — accepted ADRs locked by a content hash, not by file timestamps
- **Formatting** — one formatter for the Markdown, idempotent by construction
- **Gate-ready** — non-zero exit on errors, GitHub annotations, JSON output
- **Zero configuration** — finds ADR logs in a monorepo without being told where they are
- **Single binary** — no runtime, no downloaded grammar, no bundled script
- **Configurable** — layered `.adrlint.toml` with per-rule severities

## Installation

adrlint is one self-contained binary. Every channel installs the same release build.

| Channel                | Command                                                                                         |
| ---------------------- | ----------------------------------------------------------------------------------------------- |
| Homebrew (macOS/Linux) | `brew tap goldziher/tap && brew install adrlint`                                                |
| Scoop (Windows)        | `scoop bucket add goldziher https://github.com/Goldziher/scoop-bucket && scoop install adrlint` |
| Cargo (Rust)           | `cargo install adrlint`                                                                         |
| Cargo, prebuilt        | `cargo binstall adrlint`                                                                        |
| npm (Node.js)          | `pnpm add -g adrlint`                                                                           |
| pip (Python)           | `pip install adrlint`                                                                           |
| GitHub Release         | [archive per platform](#prebuilt-archives)                                                      |

### Cargo

```bash
cargo install adrlint          # compiles from crates.io; needs Rust 1.94+
cargo binstall adrlint         # downloads the release archive instead of compiling
```

[`cargo binstall`](https://github.com/cargo-bins/cargo-binstall) resolves
`adrlint-<target>.tar.gz` (`.zip` on Windows) from the matching GitHub Release.

### npm

```bash
npm install -g adrlint         # or: pnpm add -g adrlint, yarn global add adrlint, bun add -g adrlint
```

Run without installing:

```bash
npx adrlint lint               # or: pnpm dlx adrlint@latest lint, yarn dlx adrlint lint, bunx adrlint lint
```

The package downloads the release archive for your platform on `postinstall`, verifies its SHA-256
against the release checksums before extracting, and pins its own version. Requires Node.js 22+.

### Python

```bash
pip install adrlint            # or: pipx install adrlint, uv tool install adrlint
```

Run without installing:

```bash
uvx adrlint lint               # or: pipx run adrlint lint
```

The wheel downloads and caches the matching release binary with the same checksum verification.
Requires Python 3.10+.

### Prebuilt archives

Each release publishes five archives and a checksum manifest:

| Platform            | Archive                                    |
| ------------------- | ------------------------------------------ |
| Linux x86_64        | `adrlint-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux arm64         | `adrlint-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Intel         | `adrlint-x86_64-apple-darwin.tar.gz`       |
| macOS Apple Silicon | `adrlint-aarch64-apple-darwin.tar.gz`      |
| Windows x86_64      | `adrlint-x86_64-pc-windows-msvc.zip`       |

Checksums are in `adrlint_<version>_checksums.txt` (`sha256sum` format). Download, verify, install:

```bash
VERSION=0.4.0
TRIPLE=x86_64-unknown-linux-gnu
BASE=https://github.com/Goldziher/adrlint/releases/download/v${VERSION}

curl -fsSLO "${BASE}/adrlint-${TRIPLE}.tar.gz"
curl -fsSLO "${BASE}/adrlint_${VERSION}_checksums.txt"
grep " adrlint-${TRIPLE}.tar.gz\$" "adrlint_${VERSION}_checksums.txt" | sha256sum -c -   # macOS: shasum -a 256 -c -

tar xzf "adrlint-${TRIPLE}.tar.gz"
install -m 0755 adrlint /usr/local/bin/adrlint
```

On Windows, compare `(Get-FileHash adrlint-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash` with
the manifest line, then extract `adrlint.exe` onto your `PATH`. The Windows build targets MSVC only.

<details>
<summary><b>Build from source</b></summary>

```bash
git clone https://github.com/Goldziher/adrlint.git
cd adrlint
cargo install --path .
```

Requires Rust 1.94+ (the rumdl MSRV).

</details>

Verify the install with `adrlint --version`.

## Quick start

```bash
# Create .adrlint.toml, docs/adr, and the first ADR
adrlint init

# Lint the configured ADR directory
adrlint lint

# Lint every ADR under namespaced folders
adrlint lint --recursive adrs product/adrs

# Create the next ADR
adrlint new "Use PostgreSQL for event storage"

# Format the Markdown, then verify it in CI
adrlint format
adrlint format --check

# Lock accepted decisions, then verify the manifest in CI
adrlint lock
adrlint lock --check

# Lint with GitHub annotations
adrlint lint --format github
```

`adrlint` exits `0` when there are no errors, `1` when a rule reports an error, and `2` when the
command could not run (bad arguments, unreadable path, invalid configuration).

## Integrations

### GitHub Action

The root `action.yml` is a composite action: it downloads the release binary matching the runner,
verifies its checksum, caches it by version, OS and architecture, puts it on `PATH` and runs
`adrlint`. It bundles no JavaScript.

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

`@v0` is a floating tag that tracks the newest 0.x release; pin an exact tag (`@v0.4.0`) where
reproducibility matters. `--format github` emits workflow-command annotations with end positions, so
findings appear inline on the diff; with plain `lint`, the action registers a problem matcher that
turns the text output into annotations.

Gate formatting and the lock manifest in the same job, or install only and run adrlint yourself:

```yaml
- uses: Goldziher/adrlint@v0
  with:
    args: "format --check"
- uses: Goldziher/adrlint@v0
  with:
    args: "lock --check"

- uses: Goldziher/adrlint@v0
  id: adrlint
  with:
    install-only: "true"
- run: adrlint --version # ${{ steps.adrlint.outputs.version }} is also available
```

**Inputs** (all optional):

| Input               | Default                                      | Description                                                            |
| ------------------- | -------------------------------------------- | ---------------------------------------------------------------------- |
| `version`           | the tag the action is pinned to, else latest | adrlint version to install, with or without a leading `v`              |
| `args`              | `lint`                                       | Arguments passed to adrlint, as one space-separated string             |
| `working-directory` | `.`                                          | Directory to run adrlint in                                            |
| `install-only`      | `false`                                      | Install adrlint and put it on `PATH`, but do not run it                |
| `token`             | `${{ github.token }}`                        | Token used to query the GitHub API and download release assets         |
| `repository`        | `Goldziher/adrlint`                          | Advanced: override the `owner/repo` release assets are downloaded from |

`@v0` resolves to the newest 0.x release; a full tag resolves to that release. **Outputs:** `version`
(the version installed, without the leading `v`) and `path` (absolute path to the binary).

`args` is split on whitespace, so it cannot carry a quoted argument containing spaces. Globs are not
expanded by the shell first; adrlint expands them itself.

### pre-commit

```yaml
repos:
  - repo: https://github.com/Goldziher/adrlint
    rev: v0.4.0
    hooks:
      - id: adrlint-lint
      - id: adrlint
      - id: adrlint-lock
```

| Hook           | Runs                   | Files passed                                        |
| -------------- | ---------------------- | --------------------------------------------------- |
| `adrlint-lint` | `adrlint lint`         | changed Markdown under an ADR directory             |
| `adrlint`      | `adrlint format`       | changed Markdown under an ADR directory             |
| `adrlint-lock` | `adrlint lock --check` | none; runs when any `.md` or `adrlint.lock` changes |

The default `files` pattern covers `adr/`, `adrs/`, `decisions/`, `doc/adr/` and `docs/adr/`, at the
repository root or nested. Override `files` in your hook entry when the directory is elsewhere. The
hooks are `language: rust`, so pre-commit builds adrlint from source on first use and needs a Rust
toolchain (1.94+); for a toolchain-free setup, use a `repo: local` hook that calls an installed
binary, or the Lefthook configuration below.

A staged ADR is linted with the rest of its log, so a gap or a duplicate number next to it is still
reported; a staged Markdown file that is not an ADR and sits outside any log, such as an
`adrs/README.md` index, is skipped.

### poly

adrlint ships a [poly](https://github.com/Goldziher/poly) hook catalog, `poly-hooks.toml`. Declare it
as a source in your `poly.toml`:

```toml
[[hooks.sources]]
id = "adrlint"
git = "https://github.com/Goldziher/adrlint.git"
revision = "v0.4.0"
hooks = ["adrlint-lint", "adrlint-format-check", "adrlint-lock-check"]
```

Then run `poly hooks update` and `poly hooks install`. The hooks call an installed `adrlint`, write
nothing, and match the same directories as the pre-commit hooks; `adrlint-lock-check` runs on every
commit. See [docs/poly-hooks.md](docs/poly-hooks.md).

### Lefthook

```yaml
pre-commit:
  commands:
    adrlint-format:
      glob:
        - "{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md"
        - "**/{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md"
      run: adrlint format {staged_files}
      stage_fixed: true
    adrlint-lint:
      glob:
        - "{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md"
        - "**/{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md"
      run: adrlint lint {staged_files}
    adrlint-lock:
      glob: ["**/*.md", "adrlint.lock"]
      run: adrlint lock --check
```

This uses an installed binary, so any channel above works. `format` and `lint --fix` given only
files rewrite only those files, never a sibling ADR with unstaged edits.

### Other CI

Install a binary with any channel above, then run the gates. Each exits non-zero on failure and
writes nothing:

```bash
adrlint lint --recursive           # structure, numbering, Markdown, locked content
adrlint format --check             # formatting drift
adrlint lock --check               # manifest out of date
```

Add `--format json` for machine-readable output, or `--format github` on GitHub-hosted runners.
Exit `2` means the run could not complete, so a broken gate never reads as a pass.

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
adrlint lint [PATH]... [-r|--recursive] [--format text|json|github] [--[no-]check-external-links] [--fix [--force]]
```

Every command accepts files, directories and glob patterns. A file is checked together with its
parent directory, so a pre-commit hook that passes only the changed files still sees the numbering
next to them; a directory is one ADR log; a glob (`adrs/*`, `product/adrs/*`) expands to whatever it
matches. With no paths, the configured `adr.paths` are used, falling back to `adr.dir`. With neither
configured and no `docs/adr`, the project is searched for ADR logs, so a repository with
`adrs/<area>/` folders or a service's own `adrs/` needs no configuration at all.

A Markdown file passed explicitly that is not an ADR and sits outside any ADR log (`adrs/README.md`,
an ADR template) is skipped, so a hook can pass every changed file under `adrs/`.

`--recursive` (or `[adr].recursive = true`) adds every descendant directory that directly contains a
numbered ADR, skipping `.git`, `target`, `node_modules`, hidden directories and whatever the
project's `.gitignore` files exclude (`.ignore` files and `.gitignore` files above the project are
not read). Symlinked directories are not followed. A date-prefixed document (`2026-07-01-design.md`)
is not an ADR and does not make its directory a log. Numbering is still checked per ADR directory,
so `adrs/team-a/0001.md` and `adrs/team-b/0001.md` are independent logs.

Each root is walked once per invocation and the directories found are merged, so overlapping roots
(`adrs` and `adrs/team-a`) lint each log once; a root under a directory another root ignores is still
walked. A directory or file the walk cannot read is reported on stderr and skipped rather than
aborting the run. Directories are listed and files are checked in parallel on every core, each file
is read once, and results are merged in a fixed order, so the output is byte-identical from run to
run.

```bash
adrlint lint adrs/team-a/0002-use-postgresql.md   # that file, with its directory
adrlint lint 'adrs/*'                             # both namespaces
adrlint lint --recursive adrs product/adrs        # every nested namespace
adrlint lint --check-external-links               # also resolve http(s) links
```

#### Diagnostics

Every finding has a stable code, a severity, a path (relative to the working directory when the
file is under it), and a 1-based line and column (columns count characters). Where the rule knows the extent, it also has an end line and an exclusive end column,
and a short `help` saying what to change. Text output quotes the offending line and underlines it:

```text
docs/adr/0001-x.md:19:1: error [MADR07a] Consequences must be a level 3 heading under Decision Outcome
   |
19 | ## Consequences
   | ^^^^^^^^^^^^^^^
   = help: make it a `###` heading inside `## Decision Outcome`
```

A closing line on stderr counts the run:
``adrlint: 2 errors, 0 warnings in 14 files checked (2 fixable with `adrlint lint --fix`)``. `--format json` keeps `code`, `severity`, `message`, `path`, `line` and
`column`, and adds `fixable` plus `end_line`, `end_column` and `help` when known. `--format github`
adds `endLine` and `endColumn` to each annotation. JSON and GitHub output carry no summary, so they
stay machine-readable.

#### `--fix`

`adrlint lint --fix` applies the safe, deterministic fixes and then lints again, so the report and
the exit code describe what is left. The fixes are everything `format` applies (rumdl's fixes,
including the `*` list marker of `MADR31`, trailing whitespace and blank lines) plus the heading
level of `Consequences` and `Confirmation` when a `##` heading directly follows `## Decision Outcome`
and is one level too shallow (`MADR06`/`MADR07`). It never renames or renumbers a file and never
touches decision text. Fixable findings are marked `fixable` in JSON and counted in the summary.
Like `format`, it refuses a fix that changes the canonical content of a locked ADR unless `--force`
is passed, and exits `1` when it refused. Files it rewrote are listed on stderr as `fixed <path>`.

It plans every file before writing any: if a file cannot be read, or an ADR is a symlink, nothing is
written. A fix refused on a locked ADR is reported and the other files are still written. A file
edited after planning is not overwritten, and rewritten files keep their permissions. It rewrites
only numbered ADRs (never a template or other Markdown inside a log), and given only files rewrites only
those files.

### `adrlint format`

```text
adrlint format [PATH]... [-r|--recursive] [--check] [--force]
```

Resolves paths exactly like `lint`, then applies rumdl's fixes to each numbered ADR. Only files that
change are written, and each is printed. `--check` writes nothing and exits `1` when a file would
change, which is the form to run in CI. Formatting is idempotent: a second run changes nothing.

A rumdl fix that rewrites prose (MD034 wrapping a bare URL, for instance) changes the canonical
content of a locked ADR. `format` refuses that fix, names the file and exits `1`; pass `--force` to
write it and then `adrlint unlock` before re-locking. The check compares the file's canonical content
before and after the fix, so formatting the serializer ignores — markers, reflow, trailing spaces —
is always written, even to an ADR that has already drifted from the manifest or under a manifest
from an older adrlint.

### `adrlint init`

```text
adrlint init [--dir DIR] [--start 0|1] [--force]
```

Writes `.adrlint.toml`, creates the ADR directory, and adds
`NNNN-record-architecture-decisions.md` when the directory is empty. `--start` defaults to `1`.
`--force` overwrites an existing configuration; it never overwrites an ADR.

### `adrlint new`

```text
adrlint new "<title>"
```

Creates the next number with `O_EXCL`, so two concurrent invocations cannot claim the same file. The
new ADR starts as `proposed` and is not locked.

### `adrlint lock`

```text
adrlint lock [PATH]... [-r|--recursive] [--check] [--all]
```

Lints first and refuses to lock a directory with errors. By default it locks `accepted` and
`rejected` ADRs; `--all` includes proposed and withdrawn ones. `--check` verifies the manifest
without writing and exits `1` when it is out of date.

### `adrlint unlock`

```text
adrlint unlock <PATH> --reason "<why>"
```

Removes one entry. The reason is printed for the commit message and the lock-file diff is the audit
trail. The ADR itself is not modified.

## Rules

Every MADR rule is implemented, along with `ADR50`–`ADR52` and the rumdl `MD###` rules. `MADR11` is
opt-in.

Rule IDs are public API. Suppressing or re-severitying one in `.adrlint.toml` must keep working
across releases; a new check gets a new ID.

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

`MADR04` looks for `because`, `as`, `rationale(s)`, `reason(s)`, `reasoning`, `justification(s)`,
`motivation(s)` or `motivated` as a whole word after the chosen option, so `as` inside `basemind` is
not a rationale. The option itself is skipped: the search starts after its closing quote, or after
the first comma when it is not quoted.

### Links (`MADR11`–`MADR12`)

| ID        | Check                                                   |
| --------- | ------------------------------------------------------- |
| `MADR11`  | External links resolve (opt-in; needs network; warning) |
| `MADR12a` | Local relative paths exist                              |
| `MADR12b` | Anchor links match a heading in the document            |
| `MADR12c` | Root-relative paths exist                               |
| `MADR12d` | Absolute filesystem paths are not used                  |

Local paths, root-relative paths and anchors are always checked offline; anchors are matched against
slugs generated from the parsed heading tree. A protocol-relative destination (`//host`) is external.

`MADR11` is off by default because it needs network access and makes a run non-deterministic. Enable
it per run with `adrlint lint --check-external-links`, or for the project with
`[lint].check_external_links = true`; `--no-check-external-links` overrides the configuration for one
run. Its default severity is warning, so it does not fail a build; promote it with
`[rules] MADR11 = "error"`, or turn it off with `MADR11 = "off"`, which also skips the requests.

Each unique URL (fragment removed) is requested once per run: `HEAD`, then `GET` when the server
refuses `HEAD`, with a 10 s timeout, at most five redirects and eight requests at a time. `2xx` and
`3xx` pass, `429` is not reported, and any other status or a network error is a finding at every
place the URL is cited. Enabling it lets `lint` contact every host your ADRs link to.

### Metadata (`MADR21`)

| ID       | Check                            |
| -------- | -------------------------------- |
| `MADR21` | YAML front matter parses as YAML |

### Style (`MADR31`)

| ID       | Check                          |
| -------- | ------------------------------ |
| `MADR31` | `*` is used as the list marker |

### Directory (`MADR41`–`MADR44`)

| ID        | Check                                          |
| --------- | ---------------------------------------------- |
| `MADR41a` | The first number is `[adr].start` (`0` or `1`) |
| `MADR41b` | No number is missing between two ADRs          |
| `MADR42`  | No non-ADR file sits in the ADR directory¹     |
| `MADR43`  | No two ADRs share a number                     |
| `MADR44`  | File names match `NNNN-brief-description.md`   |

¹ Images (`png`, `jpg`, `jpeg`, `gif`, `svg`, `webp`) that an ADR in the same directory links to are
allowed.

An index, `README.md` or `index.md` in any letter case (`README.MD` too), is not part of the log: it
is neither linted as an ADR nor reported as a foreign file, so a log can carry its own table of
contents. An image the index links to is an allowed asset, like one an ADR links to.

### adrlint (`ADR50`–`ADR52`)

| ID      | Check                                                   |
| ------- | ------------------------------------------------------- |
| `ADR50` | A locked ADR's content hash differs from `adrlint.lock` |
| `ADR51` | A locked ADR is missing from disk                       |
| `ADR52` | An ADR cannot be read, or is not valid UTF-8            |

### Markdown (`MD###`)

rumdl's rules are reported verbatim, so a suppression you already know keeps working. Every enabled
Markdown rule is an error by default, so `lint` gates on Markdown quality without extra
configuration; `[markdown.rules] MD009 = "warn"` downgrades one rule. Opinionated defaults: line
length (`MD013`) and inline HTML (`MD033`) are off, because ADR prose is reflowed and decision
records legitimately embed HTML tables. `MD004` is pinned to `*`, matching `MADR31`, so `format` and
`lint` agree.

## Configuration

`.adrlint.toml` is discovered from the current directory upward and merged lowest-precedence first:
the global file at `dirs::config_dir()/adrlint/config.toml`, then each project file from the
outermost ancestor inward; `--config PATH` selects a file explicitly. Each layer sets only the keys
it contains, and `[rules]` and `[markdown.rules]` merge key by key, so a nested file overrides one
rule without discarding the rest. `adrlint init` writes the full commented template.

When `lint`, `format` or `lock` is given paths, the walk starts at the nearest project file above
them instead of the current directory, so `adrlint lint repo-a/adrs` run from a parent directory
uses `repo-a/.adrlint.toml` and `repo-a/adrlint.lock`. `lint` and `format` given paths from several
projects check each project's paths under its own configuration and report the findings together,
so a hook in a monorepo with nested `.adrlint.toml` files works unchanged. `lock` writes one
manifest per project, so it refuses paths from more than one project (exit `2`); run it once per
project. `--config` applies one file to every path.

```toml
[adr]
dir = "docs/adr"
# Extra ADR logs used when no paths are passed. Directories or globs, relative to the project root.
paths = ["adrs/*", "product/adrs/*"]
start = 1 # 0 or 1; used by init and new
allow_gaps = false # only for imported histories
recursive = false # search the roots for nested ADR logs, as --recursive does

[lint]
check_external_links = false # true enables MADR11, as --check-external-links does

[lock]
enabled = true
path = "adrlint.lock"
statuses = ["accepted", "rejected"]

[markdown]
enabled = true

[markdown.rules]
MD013 = "off"
MD033 = "off"

[rules]
MADR03 = "warn"
MADR11 = "error" # upgrade external link failures from the default warning
MADR12c = "error"
```

`[adr].paths` is the default input set when no CLI paths are given; `dir` remains the single ADR
directory used by `init` and `new`. Each layer replaces the path list as a whole.

`[markdown].enabled = false` removes every rumdl finding and every rumdl fix. `[markdown.rules]`
takes `off`, `warn` or `error`, keyed by rumdl's own code: `off` removes both the finding and the
fix, while `warn` and `error` set the finding's severity. A rule you do not name keeps its default —
the shipped ones turn `MD013` and `MD033` off.

`[rules]` sets the severity of any finding, including rumdl's `MD###`. A key matches every code it
prefixes, ignoring case, and the longest key wins: `MADR03 = "warn"` covers `MADR03a` and `MADR03b`,
while an explicit `MADR03b = "off"` removes only that variant.

`[lint]` holds opt-in checks that need more than the ADR files. `check_external_links = true`
enables `MADR11`; the `--check-external-links` flag does the same for one run.

Unknown configuration keys are errors, not silently ignored. A rule ID in `[rules]` or
`[markdown.rules]` that matches no adrlint or rumdl code is reported once when `lint` starts.

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
- The hash is over a **semantic serialization**, not the bytes on disk. Formatting, line endings,
  list markers, fence styles and paragraph reflow do not change it; a changed word, heading, link
  target or code body does. A formatter fix that rewrites prose still needs an unlock.
- `adrlint lock --check` is the strict CI form and exits `1` when the manifest is out of date,
  including when an ADR in a locked status exists but `adrlint.lock` does not. With no manifest and
  nothing to lock it exits `0` ("nothing to lock"), so the gate can be enabled before the first lock.
- The `canonical` field versions that serializer, so a tool upgrade warns instead of failing every
  consumer's build. `lint` prints that warning once and verifies nothing until the manifest is
  refreshed with `adrlint lock --all`.

### Superseding a decision

A locked ADR is never rewritten. To replace a decision:

1. Write and accept the new ADR (`adrlint new`), citing the one it replaces.
2. `adrlint unlock <old> --reason "superseded by NNNN"`.
3. Mark the old ADR with `status: superseded` and a `superseded-by` key naming its successor:

   ```yaml
   ---
   status: superseded
   superseded-by: "0031"
   date: 2026-01-15
   ---
   ```

4. `adrlint lock`.

Add `superseded` (and `deprecated`, if you use it) to `[lock].statuses` so the replaced ADR is locked
again; the default list holds only `accepted` and `rejected`. MADR's template form,
`status: superseded by ADR-0031`, also counts as `superseded`, so either spelling locks.

The manifest is committed. It is reviewed like source; `ADR50` and `ADR51` are the only findings a
change to it can produce. `ADR52` is separate: it reports a file that cannot be read at all, and the
run continues so one corrupt file cannot hide every other finding.

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

## How it works

1. **Discover** the ADR directory and parse each `NNNN-slug.md` into a directory model.
2. **Parse** Markdown once with pulldown-cmark, retaining byte offsets and front matter.
3. **Check** MADR structure and numbering from that model; run rumdl on the same source.
4. **Merge** findings, sort, and report as text, JSON or GitHub annotations. Overlapping inputs are
   merged before linting, so each file is checked once.
5. **Lock** by hashing a deterministic serialization of the document's content.

Steps 1 to 3 run in parallel across directories and files. `lint` writes nothing unless you pass
`--fix`; `format` writes fixes, and only `lock`/`unlock` touch the manifest.

### Large repositories

Hardening for monorepos: logs are found at any depth (`services/x/adrs/area/sub/`), including logs
of three- or five-digit names (`001-x.md`), which are then reported as `MADR44`; `adrs/README.md`
indexes, ADR templates and dated design documents (`docs/specs/2026-07-01-x.md`) are not logs;
`.gitignore`d build output, hidden directories and `node_modules` are never entered; symlinks are not
followed, so a loop cannot hang a walk; an unreadable file is an `ADR52` finding and an unreadable
directory a warning; a non-UTF-8 file is an `ADR52` finding; an ADR over 16 MiB is reported instead
of parsed; CRLF files lint and lock like LF files; control characters in an ADR are never echoed raw
to the terminal; every file is checked on a worker thread with a 16 MiB stack. `adrlint.lock`
keys are project-root-relative paths with forward slashes, `lock` finds the same logs as `lint`, and
`lock --check` verifies every nested log.

## Roadmap

- [x] Directory rules, `lint`, `init`, `new`
- [x] Markdown parser and document model
- [x] `MADR01`–`MADR08` structural rules
- [x] `MADR12`, `MADR21`, `MADR31` link, metadata and style rules
- [x] `adrlint lock` / `unlock` with semantic hashing
- [x] rumdl diagnostics and `adrlint format`
- [x] Parallel linting, `--fix`, spans and hints in diagnostics
- [x] External link checking (`MADR11`, opt-in)
- [ ] 1.0 release

## Library

The crate is also a library: `adr::parse` (re-exported as `parse_adr`) gives you the single
Markdown parse and its `Document`,
`rules::check_document` runs the MADR rules, `markdown::check` and `markdown::format` wrap rumdl,
`lock::canonical_hash` produces the immutability hash, and `config::load` resolves the layered
configuration.

```rust
use adrlint::{adr, rules};

let document = adr::parse(source);
let findings = rules::check_document(&document, path, root);
```

The CLI is a thin layer over those modules; nothing in the library shells out.

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md). Rules ship with fixtures under `fixtures/adr/`, and a bug
report should include the smallest ADR that reproduces it.

## License

MIT — see [LICENSE](./LICENSE).
