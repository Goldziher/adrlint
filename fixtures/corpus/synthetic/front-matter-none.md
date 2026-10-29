# Keep status in the body when there is no front matter

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
