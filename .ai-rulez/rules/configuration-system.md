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

Layered TOML defined in `src/config/` and resolved by `ConfigManager` (`src/config/manager.rs`).

### Precedence (highest to lowest)

1. **CLI flags.**
2. **Project config**, nearest to the ADR directory first, then each ancestor outward. A nested config
   overrides only the keys it contains.
3. **Global config** at `dirs::config_dir()/adrlint/config.toml`.
4. **Built-in defaults.**

`CONFIG_FILE_NAMES` is the authority on names: `.adrlint.toml` is preferred; `adrlint.toml` is a
deprecated fallback. A directory holding both uses the preferred name outright and the other is not
merged in.

### Shape

The 0.1.0 scaffold parses `[adr]` only. A key from the other tables is an unknown-key error until the
rule family that consumes it lands, rather than a silent no-op.

```toml
[adr]
dir = "docs/adr"
start = 1 # 0 or 1, fixed for the life of the directory
allow_gaps = false

[lint]
check_external_links = false
severity = "error" # default for findings without an override

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

- Every `[global]` key is `Option<T>` on the file struct and its presence is recorded in
  `SpecifiedFlags`. Layering needs "absent" to differ from "set to the default"; `#[serde(default)]`
  on a `bool` collapses the two and silently erases the outer setting.
- Every config struct carries `#[serde(deny_unknown_fields)]`. A typo is an error, not a no-op.
- Validate in `Config::validate`, not at use: reject unknown severity values, `start` outside `{0,1}`,
  a `dir` that escapes the project root, and a lock path outside it.
- `[rules]` keys are matched longest-prefix: setting `MADR03` configures `MADR03a` and `MADR03b`,
  while an explicit `MADR03b` wins. `off` removes the finding entirely.
- `adrlint init` writes the full template with comments, including every key. A key missing from the
  template does not exist as far as users are concerned.
- `adrlint lint` reports unknown or misspelled rule IDs in config once, at startup, rather than
  ignoring them.
