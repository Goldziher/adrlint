---
status: accepted
date: 2026-01-15
decision-makers: [Na'aman Hirschfeld]
consulted: []
informed: []
---

# Record architecture decisions

## Context and Problem Statement

We need to record the architectural decisions made on this project, including the context that made
each decision necessary and the alternatives that were rejected.

## Decision Drivers

* Decisions must remain discoverable after the people who made them move on
* The record must live next to the code it describes
* Writing a record must not require a special tool to read

## Considered Options

* Architecture Decision Records in Markdown
* A wiki page per decision
* No formal record

## Decision Outcome

Chosen option: "Architecture Decision Records in Markdown", because they are reviewable in the same
pull request as the change they justify and remain readable without tooling.

### Consequences

* Good, because the rationale survives staff changes
* Good, because decisions are reviewed like code
* Bad, because every decision costs a small amount of writing time

### Confirmation

The presence of this directory and the entries in it confirm adoption.

## Pros and Cons of the Options

### Architecture Decision Records in Markdown

* Good, because they are versioned with the code
* Good, because they need no special tooling to read
* Bad, because they are easy to skip under delivery pressure

### A wiki page per decision

* Good, because editing is frictionless
* Bad, because the page drifts away from the code
* Bad, because review is separate from the change

### No formal record

* Good, because nothing has to be written
* Bad, because context is lost

## More Information

* [MADR](https://adr.github.io/madr/)
* [Architecture Decision Records](https://adr.github.io/)
