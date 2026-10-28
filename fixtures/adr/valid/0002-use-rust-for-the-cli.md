---
status: accepted
date: 2026-02-02
decision-makers: [Na'aman Hirschfeld]
consulted: []
informed: []
---

# Use Rust for the command-line interface

## Context and Problem Statement

The linter must parse Markdown, hash documents and ship as a single binary on five platforms. Which
implementation language should the command-line interface use?

## Decision Drivers

* A single self-contained binary for every supported platform
* Access to a mature Markdown parser and formatter
* Fast startup, because the tool runs in pre-commit hooks

## Considered Options

* Rust
* Go
* TypeScript on Node.js

## Decision Outcome

Chosen option: "Rust", because it produces a self-contained binary and the Markdown ecosystem already
provides the parser and formatter we need.

### Consequences

* Good, because distribution is a single binary per platform
* Good, because rumdl can be linked in-process
* Bad, because contributing requires the Rust toolchain

### Confirmation

Release archives contain one executable and no runtime dependencies.

## Pros and Cons of the Options

### Rust

* Good, because the binary is self-contained
* Good, because parsing performance is predictable
* Bad, because compile times are longer than a scripted tool

### Go

* Good, because the toolchain is simple
* Bad, because the Markdown linting ecosystem is thinner

### TypeScript on Node.js

* Good, because iteration is fast
* Bad, because it requires a Node.js runtime in every consumer

## More Information

* [rumdl](https://github.com/rvben/rumdl)
