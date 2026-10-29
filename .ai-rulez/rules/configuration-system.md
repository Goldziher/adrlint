---
type: Decision
title: Configuration System
x-ai-rulez:
  kind: rule
  id: configuration-system
  metadata:
    priority: medium
---

## Configuration System

Layered TOML defined in `src/config.rs` and resolved by `config::load`.

### Precedence (highest to lowest)

1. **CLI flags.**
2. **Project config**, found by walking up from the current directory: the nearest first, then each
   ancestor outward. A nested config overrides only the keys it contains.
3. **Global config** at `dirs::config_dir()/adrlint/config.toml`.
4. **Built-in defaults.**

`CONFIG_FILE_NAMES` is the authority on names: `.adrlint.toml` is preferred; `adrlint.toml` is a
deprecated fallback. A directory holding both uses the preferred name outright and the other is not
merged in.

### Shape

The 0.3.0 release parses `[adr]`, `[lint]`, `[lock]`, `[markdown]` and `[rules]`.

```toml
[adr]
dir = "docs/adr"
# Extra logs used when no CLI paths are given; directories or globs, replaced as a whole per layer.
paths = ["adrs/*", "product/adrs/*"]
start = 1 # 0 or 1, fixed for the life of the directory
allow_gaps = false
recursive = false # search the roots for nested ADR logs, as --recursive does

[lint]
check_external_links = false

[markdown]
enabled = true

[markdown.rules]
MD013 = "off"
MD033 = "off"

[rules]
MADR03 = "error"
MADR12c = "warn"

[lock]
enabled = true
path = "adrlint.lock"
statuses = ["accepted", "rejected"]
```

### Rules of the wire format

- Each file deserializes into a private `Layer` whose scalar fields are `Option<T>`, so "absent"
  differs from "set to the default". `Layer::apply_to` replaces only the keys a layer contains.
  `[rules]` and `[markdown.rules]` fold key by key; `lock.statuses` replaces as a whole list.
- Every config struct carries `#[serde(deny_unknown_fields)]`. A typo is an error, not a no-op.
- Validate in `config::validate` after the layers are folded, not at use: reject unknown severity
  values, `start` outside `{0,1}`, a `dir` that escapes the project root, and a lock path outside it
  (including a Windows drive or UNC prefix).
- `[rules]` keys are matched longest-prefix: setting `MADR03` configures `MADR03a` and `MADR03b`,
  while an explicit `MADR03b` wins. `off` removes the finding entirely.
- Inputs resolve in this order: CLI paths, `adr.paths`, `adr.dir`. A file is always checked with its
  parent directory; globs expand; `--recursive` adds descendant directories that contain Markdown.
- `adrlint init` writes the full template with comments, including every key. A key missing from the
  template does not exist as far as users are concerned.
- `adrlint lint` reports unknown or misspelled rule IDs in config once, at startup, rather than
  ignoring them.
