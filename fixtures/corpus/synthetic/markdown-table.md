# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

| Option     | Managed | Cost |
| ---------- | ------- | ---- |
| PostgreSQL | yes     | low  |
| SQLite     | no      | none |

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
