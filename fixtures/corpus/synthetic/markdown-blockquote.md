# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The platform team raised the requirement.

> Every write must survive a restart.
> Reads may be served from a replica.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
