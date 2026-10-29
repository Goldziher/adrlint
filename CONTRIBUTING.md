# Contributing to adrlint

## Local development

### Prerequisites

- Rust 1.94+ with `cargo`
- `task` (go-task) for the Taskfile
- `poly` and `ai-rulez` 5.x for governance checks
- Node.js 24+ and pnpm 12+ only for the npm wrapper
- Python 3.10+ only for the PyPI wrapper

### Bootstrap

```bash
git clone https://github.com/Goldziher/adrlint.git
cd adrlint
cargo check
cargo test
```

### Checks

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
poly lint .
poly fmt --check .
```

Or `task check`, which runs lint and tests.

## Adding a rule

1. Choose a stable ID. Ported MADR rules keep the madrlint ID (`MADR01`–`MADR44`); adrlint-specific
   rules use `ADR50` and above. Never rename an existing ID — suppression in user configuration
   depends on it.
2. Add the implementation under the relevant module in `src/rules/`.
3. Add a valid fixture under `fixtures/adr/valid/` and one violating fixture per rule under
   `fixtures/adr/invalid/<rule-id>/`.
4. Add a test that asserts the exact code and the file it points at.
5. Document the rule in the README table and in `.ai-rulez/context/madr-model.md` when it changes the
   document model.

A rule that needs a network call is `#[ignore]`d by default and gated behind configuration.

## Fixtures

- `fixtures/adr/valid/` must pass every rule.
- `fixtures/adr/invalid/<rule-id>/` holds the smallest document that violates that rule.
- The [`adr/madrlint`](https://github.com/adr/madrlint) `madr-samples/` corpus is the conformance
  baseline for ported structural rules.
- Namespaced ADR logs (`adrs/<namespace>/`, `product/adrs/<namespace>/`) are covered by
  `tests/recursive.rs`; directory rules are per log, so the same number may appear in two namespaces.
- `fixtures/corpus/` vendors real ADRs from madrlint, adr-manager and log4brains at pinned commits,
  plus a generated matrix of MADR shapes. Do not edit the vendored files; refresh them with
  `scripts/fetch-corpus.sh` and regenerate the synthetic tree with `scripts/generate-corpus.py`.
- `tests/corpus.rs` runs parsing, formatting, hashing, truncation and CLI round trips over the whole
  corpus. A new fixture is picked up automatically.

## Pre-commit hooks

Install with `task setup` (or `poly hooks install`). The hooks run lint, format and file-safety
checks on every commit, and validate the commit message. Run them across the tree with:

```bash
poly hooks run pre-commit --all-files
```

## Governance files

`AGENTS.md`, `CLAUDE.md` and the assistant presets are generated from `.ai-rulez/`. Never edit them by
hand. After changing `.ai-rulez/`, run:

```bash
task ai:validate
ai-rulez generate --no-local
```

## Distribution and releases

adrlint ships through crates.io, npm, PyPI, Homebrew and GitHub Releases. Versions must match across
all surfaces.

### Version bump checklist

1. `Cargo.toml` — `version` under `[package]`
2. `npm-package/package.json` — `version`
3. `pip-package/pyproject.toml` — `version`
4. `pip-package/adrlint/__init__.py` — `__version__`

Then regenerate `Cargo.lock` if needed and commit with `chore(release): vX.Y.Z`.

### Tagging

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

Release candidates use `vX.Y.Z-rc.1` for Git and npm, and `X.Y.Zrc1` for PyPI.

### Publishing

Pushing the tag triggers `.github/workflows/publish.yaml`. It validates the tag, runs formatting,
clippy, tests and `cargo deny` on the tagged commit, builds the native archives, uploads them with
`adrlint_<version>_checksums.txt`, promotes the release, then publishes to crates.io, npm, PyPI and
the Homebrew tap. No artifact is published unless the verification job succeeds.
