# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

* PostgreSQL
  * managed service
    * point-in-time recovery
  * self-hosted
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
