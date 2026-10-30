# Poly hooks

adrlint publishes a [poly](https://github.com/Goldziher/poly) hook catalog in `poly-hooks.toml`. A
consumer repository declares adrlint as a hook source in its own `poly.toml` and selects the hooks it
wants; it never creates a `poly-hooks.toml` of its own.

The catalog exists since adrlint 0.4.0. Every hook runs the installed `adrlint` executable, so the
catalog revision pins the hook definitions, not the binary: install the reviewed adrlint version
separately (any channel in the README) in local setup and CI.

The catalog needs poly 0.14.0 or later, the first release whose catalog hooks accept
`pass_filenames` and `always_run`. It is tested with poly 0.31.2.

## Add adrlint to a repository

Add a Git source to `poly.toml`:

```toml
[[hooks.sources]]
id = "adrlint"
git = "https://github.com/Goldziher/adrlint.git"
revision = "v0.4.0"
hooks = ["adrlint-lint", "adrlint-format-check", "adrlint-lock-check"]
```

Every Git source needs an explicit revision and a nonempty hook selection; poly rejects unknown hook
IDs, and a hook added to a later catalog never runs until you select it.

Select the system channel in the gitignored `poly.local.toml`:

```toml
[hook_preferences]
channels = ["system"]
```

Then resolve the revision, commit the lock and install the Git shims:

```bash
poly hooks update
git add poly.toml poly-hooks.lock
poly hooks install
```

Hook runs use the locked commit. Run `poly hooks update` again after changing `revision`.

## Available hooks

| Hook                   | Runs                     | Files passed                            |
| ---------------------- | ------------------------ | --------------------------------------- |
| `adrlint-lint`         | `adrlint lint`           | changed Markdown under an ADR directory |
| `adrlint-format-check` | `adrlint format --check` | changed Markdown under an ADR directory |
| `adrlint-lock-check`   | `adrlint lock --check`   | none; runs on every commit              |

None of the hooks writes a file. Run `adrlint format` yourself when `adrlint-format-check` fails.

The file hooks match Markdown under `adr/`, `adrs/`, `decisions/`, `doc/adr/` and `docs/adr/`, at the
repository root or nested, the same directories as the pre-commit hooks. A staged ADR is linted with
the rest of its log, so a gap or duplicate number next to it is still reported. A commit that only
deletes an ADR stages no file for these hooks, so it is not linted, and the gap it leaves is
reported on the next commit that touches the log or by `adrlint lint` in CI; `adrlint-lock-check`
still fails when the deleted ADR was locked (`ADR51`).

`adrlint-lock-check` always runs and receives no filenames, because poly leaves deleted files out of
the staged list: a commit that only deletes a locked ADR or edits `adrlint.lock` must still be
checked. Before the log is locked, with no `adrlint.lock` and no ADR in a locked status, it passes
with "nothing to lock", so it is safe to select from the start.

## Use a local source

For hook development, replace `git` and `revision` with a path to an adrlint checkout:

```toml
[[hooks.sources]]
id = "adrlint"
path = "../adrlint"
hooks = ["adrlint-lint"]
```

poly reads a local catalog on every run and writes no lock entry for it.
