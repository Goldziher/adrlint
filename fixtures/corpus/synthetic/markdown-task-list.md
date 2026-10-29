# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.

## More Information

* [x] The runbook names the failover owner.
* [ ] The alert threshold is reviewed.
