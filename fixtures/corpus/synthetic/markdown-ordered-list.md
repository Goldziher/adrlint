# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

1. PostgreSQL
2. SQLite
3. DuckDB

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
