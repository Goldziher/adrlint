# Use PostgreSQL as the primary datastore

## Context and Problem Statement

The service needs a transactional store that survives a restart.

## Considered Options

<table>
  <tr><th>Option</th><th>Managed</th></tr>
  <tr><td>PostgreSQL</td><td>yes</td></tr>
</table>

## Decision Outcome

Chosen option: "PostgreSQL", because the team already operates it.
