---
type: Concept
title: Development Setup
x-ai-rulez:
  kind: context
  id: development-setup
  metadata:
    priority: high
---

## Development Setup

### Prerequisites

- Rust 1.94+ (`rustc --version`) — rumdl's current MSRV
- Cargo (included with Rust)
- Task runner (`task --version`) for task automation
- Node.js 24+ and pnpm 12+ only when verifying the npm wrapper
- Python 3.10+ only when verifying the PyPI wrapper

### Build

```bash
cargo build
cargo build --release
cargo run -- --help
```

### Test

```bash
cargo test
cargo test -- --nocapture
cargo test --test lint_command
```

Run the linter against the fixture corpus while developing rules:

```bash
cargo run -- lint fixtures/adr/valid
cargo run -- lint fixtures/adr/invalid/MADR41b
```

### Lint and format

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
poly lint .
poly fmt --check .
```

### Task runner

```bash
task          # list tasks
task setup    # install dependencies and hooks
task build    # release build
task test     # test suite
task lint     # poly lint
task check    # lint plus test
task format   # poly fmt --fix
task clean    # remove build artifacts
```
