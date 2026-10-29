# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.

## More Information

The health check is:

    curl --fail http://localhost:8080/health

It runs every ten seconds.
