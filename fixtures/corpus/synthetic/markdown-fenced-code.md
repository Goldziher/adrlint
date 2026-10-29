# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

* PostgreSQL
* SQLite

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.

## More Information

The probe is configured like this:

```yaml
liveness:
  path: /health

  period: 10s
```
