# Adopt an event-sourced write model

## Context and Problem Statement

Auditors need every state transition, not only the current row.
The existing CRUD tables overwrite history and cannot answer when a value changed.

## Decision Drivers

* Regulatory auditability
* Replay of historical state for debugging
* Operability by the current team

## Considered Options

* Event sourcing
* Append-only audit table
* Temporal tables

## Decision Outcome

Chosen option: "Append-only audit table", because it records every transition without rewriting the
write model.

### Consequences

* Good, because the change is additive and can ship behind a flag.
* Bad, because reads that need history must join a second table.

### Confirmation

The audit table is confirmed by replaying one month of production writes in staging.

## Pros and Cons of the Options

### Event sourcing

* Good, because the log is the source of truth.
* Bad, because the team has no operational experience with projections.

### Append-only audit table

* Good, because it is a small change to the existing model.
* Bad, because the history is not the source of truth.

### Temporal tables

* Good, because the database manages the versioning.
* Bad, because the chosen engine does not support them.

## More Information

The discussion is recorded in the architecture meeting notes.
