# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The platform team has to keep the service running through a database restart, a
rolling deployment and a regional failover, and wants a store that its on-call
engineers can operate without learning a new query language or a new backup tool.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
