#!/usr/bin/env python3
"""Generate the synthetic ADR corpus under fixtures/corpus/synthetic/.

Each file is small and focused on one shape the parser, the MADR rules, the Markdown layer or the
canonical serializer must handle. The generated files are committed; run this script after editing
a case so the corpus stays reproducible.

`fixtures/corpus/synthetic/` must lint clean: every document there is valid MADR and valid Markdown
under adrlint's defaults. Shapes that are deliberately invalid — a missing title, an empty section, a
document of headings only, duplicate headings, a missing final newline — live in
`fixtures/corpus/synthetic-invalid/` instead, so a clean run over the first tree stays meaningful.

Stdlib only.
"""

from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "fixtures" / "corpus"

TITLE = "Use PostgreSQL as the primary datastore"


def block(text: str) -> str:
    """Trim surrounding blank lines so every block is separated by exactly one blank line."""
    return text.strip("\n")


def minimal(
    title: str = TITLE,
    context: str = "The service needs a transactional store that survives a restart.",
    options: str = "* PostgreSQL\n* SQLite\n",
    outcome: str = 'Chosen option: "PostgreSQL", because the team already operates it.',
) -> str:
    """A valid MADR document with only the three mandatory sections."""
    return (
        f"# {title}\n"
        "\n"
        "## Context and Problem Statement\n"
        "\n"
        f"{block(context)}\n"
        "\n"
        "## Considered Options\n"
        "\n"
        f"{block(options)}\n"
        "\n"
        "## Decision Outcome\n"
        "\n"
        f"{block(outcome)}\n"
    )


def with_section(heading: str, body: str) -> str:
    """A minimal document with one extra H2 section appended after `Decision Outcome`."""
    return minimal() + f"\n## {heading}\n\n{block(body)}\n"


def with_front_matter(front_matter: str, body: str | None = None) -> str:
    return f"---\n{front_matter}---\n\n" + (body if body is not None else minimal())


FULL = """\
# Adopt an event-sourced write model

## Context and Problem Statement

Auditors need every state transition, not only the current row.
The existing CRUD tables overwrite history and cannot answer when a value changed.

## Decision Drivers

* Regulatory auditability
* Replay of historical state for debugging
* Operability by the current team

## Considered Options

* Event sourcing
* Append-only audit table
* Temporal tables

## Decision Outcome

Chosen option: "Append-only audit table", because it records every transition without rewriting the
write model.

### Consequences

* Good, because the change is additive and can ship behind a flag.
* Bad, because reads that need history must join a second table.

### Confirmation

The audit table is confirmed by replaying one month of production writes in staging.

## Pros and Cons of the Options

### Event sourcing

* Good, because the log is the source of truth.
* Bad, because the team has no operational experience with projections.

### Append-only audit table

* Good, because it is a small change to the existing model.
* Bad, because the history is not the source of truth.

### Temporal tables

* Good, because the database manages the versioning.
* Bad, because the chosen engine does not support them.

## More Information

The discussion is recorded in the architecture meeting notes.
"""

SETEXT = """\
Record architecture decisions
=============================

Context and Problem Statement
-----------------------------

We need a durable record of why the architecture changed.

Considered Options
------------------

* A decision log in the repository
* A wiki page

Decision Outcome
----------------

Chosen option: "A decision log in the repository", because it is reviewed with the code.
"""

LONG_TITLE = "Standardise every service on the shared deployment pipeline rather than per-team scripts that drift"

VALID = {
    # Minimal and full MADR shapes.
    "valid-minimal-madr.md": minimal(),
    "valid-full-madr.md": FULL,
    # Front matter.
    "front-matter-accepted.md": with_front_matter("status: accepted\n"),
    "front-matter-proposed.md": with_front_matter("status: proposed\n"),
    "front-matter-rejected.md": with_front_matter("status: rejected\n"),
    "front-matter-deprecated.md": with_front_matter("status: deprecated\n"),
    "front-matter-superseded.md": with_front_matter("status: superseded\nsuperseded-by: 0012-use-sqlite.md\n"),
    "front-matter-list-value.md": with_front_matter("status: accepted\ndeciders:\n  - alice\n  - bob\n"),
    "front-matter-date.md": with_front_matter("status: accepted\ndate: 2025-01-15\n"),
    "front-matter-quoted-strings.md": with_front_matter(
        'status: "accepted"\ndate: "2025-01-15"\ndeciders: ["Alice", "Bob"]\n'
    ),
    "front-matter-none.md": minimal(
        title="Keep status in the body when there is no front matter",
    ),
    # Heading aliases accepted by the MADR rules.
    "heading-alias-context-and-problem.md": minimal().replace("Context and Problem Statement", "Context and Problem"),
    "heading-alias-considered-alternatives.md": minimal().replace("Considered Options", "Considered Alternatives"),
    "heading-alias-explored-options.md": minimal().replace("Considered Options", "Explored Options"),
    "heading-alias-information.md": with_section("Information", "The rollout is tracked in the platform backlog."),
    "headings-setext.md": SETEXT,
    # Line endings and Unicode.
    "line-endings-crlf.md": minimal().replace("\n", "\r\n"),
    "unicode-cjk.md": minimal(
        title="主数据库采用 PostgreSQL",
        context="服务需要一个在重启后仍然可靠的事务型数据库。",
        outcome='Chosen option: "PostgreSQL", because 团队已有运维经验。',
    ),
    "unicode-accented.md": minimal(
        title="Décider de la base de données principale",
        context="Le service a besoin d'un magasin transactionnel — déjà exploité par l'équipe.",
        outcome='Chosen option: "PostgreSQL", because la équipe le maîtrise déjà.',
    ),
    "unicode-emoji.md": minimal(
        title="Store the roadmap beside the code",
        context="Planning lives in a separate tool, so it drifts. 🚀",
        outcome='Chosen option: "PostgreSQL", because the team already runs it. 🎉',
    ),
    # Markdown shapes.
    "markdown-nested-lists.md": minimal(
        options=("* PostgreSQL\n  * managed service\n    * point-in-time recovery\n  * self-hosted\n* SQLite\n")
    ),
    "markdown-ordered-list.md": minimal(options="1. PostgreSQL\n2. SQLite\n3. DuckDB\n"),
    "markdown-task-list.md": with_section(
        "More Information",
        "* [x] The runbook names the failover owner.\n* [ ] The alert threshold is reviewed.\n",
    ),
    "markdown-blockquote.md": minimal(
        context=(
            "The platform team raised the requirement.\n"
            "\n"
            "> Every write must survive a restart.\n"
            "> Reads may be served from a replica.\n"
        ),
        options="* PostgreSQL\n* SQLite\n",
    ),
    "markdown-fenced-code.md": with_section(
        "More Information",
        "The probe is configured like this:\n\n```yaml\nliveness:\n  path: /health\n\n  period: 10s\n```\n",
    ),
    "markdown-indented-code.md": with_section(
        "More Information",
        "The health check is:\n\n    curl --fail http://localhost:8080/health\n\nIt runs every ten seconds.\n",
    ),
    "markdown-table.md": minimal(
        options=(
            "| Option     | Managed | Cost |\n"
            "| ---------- | ------- | ---- |\n"
            "| PostgreSQL | yes     | low  |\n"
            "| SQLite     | no      | none |\n"
        )
    ),
    "markdown-html-table.md": minimal(
        options=(
            "<table>\n"
            "  <tr><th>Option</th><th>Managed</th></tr>\n"
            "  <tr><td>PostgreSQL</td><td>yes</td></tr>\n"
            "</table>\n"
        )
    ),
    "markdown-image.md": minimal(
        context=(
            "The deployment topology is unchanged.\n"
            "\n"
            '![Deployment diagram](https://example.com/diagram.png "Deployment diagram")\n'
        )
    ),
    "markdown-inline-link.md": minimal(
        context="The [MADR template](https://adr.github.io/madr/) describes the sections."
    ),
    "markdown-reference-link.md": minimal(
        context="The [MADR template][madr] describes the sections.",
    )
    + "\n[madr]: https://adr.github.io/madr/\n",
    "markdown-anchor-link.md": minimal(
        context="The trade-offs are listed under [Considered Options](#considered-options)."
    ),
    "markdown-hard-break.md": minimal(
        context=("The first line ends with a hard break.  \nThe second line follows it.")
    ),
    "markdown-soft-wrap.md": minimal(
        context=(
            "The platform team has to keep the service running through a database restart, a\n"
            "rolling deployment and a regional failover, and wants a store that its on-call\n"
            "engineers can operate without learning a new query language or a new backup tool.\n"
        )
    ),
    # Edge shapes that still lint clean.
    "edge-long-title.md": minimal(title=LONG_TITLE),
    # A UTF-8 BOM must not hide the title or the sections.
    "line-endings-utf8-bom.md": "\ufeff" + minimal(),
}

INVALID = {
    # No H1: MADR01a.
    "missing-title.md": minimal().replace(f"# {TITLE}\n\n", ""),
    # An empty mandatory section: MADR02.
    "empty-required-section.md": (
        f"# {TITLE}\n"
        "\n"
        "## Context and Problem Statement\n"
        "\n"
        "The service needs a transactional store.\n"
        "\n"
        "## Considered Options\n"
        "\n"
        "## Decision Outcome\n"
        "\n"
        'Chosen option: "PostgreSQL", because the team already operates it.\n'
    ),
    # An empty optional section is still an empty section: MADR02.
    "empty-optional-section.md": (
        f"# {TITLE}\n"
        "\n"
        "## Context and Problem Statement\n"
        "\n"
        "The service needs a transactional store.\n"
        "\n"
        "## Decision Drivers\n"
        "\n"
        "## Considered Options\n"
        "\n"
        "* PostgreSQL\n"
        "* SQLite\n"
        "\n"
        "## Decision Outcome\n"
        "\n"
        'Chosen option: "PostgreSQL", because the team already operates it.\n'
    ),
    # Only headings, so every section is empty: MADR02.
    "headings-only.md": (
        "# Record architecture decisions\n"
        "\n"
        "## Context and Problem Statement\n"
        "\n"
        "## Considered Options\n"
        "\n"
        "## Decision Outcome\n"
    ),
    # Repeated heading text: MD024. The anchor link proves the slugs are de-duplicated.
    "duplicate-headings.md": minimal()
    + (
        "\n## More Information\n"
        "\n"
        "See [the second notes section](#notes-1).\n"
        "\n"
        "### Notes\n"
        "\n"
        "The first set of notes.\n"
        "\n"
        "### Notes\n"
        "\n"
        "The second set of notes.\n"
    ),
    # No trailing newline: MD047.
    "no-trailing-newline.md": minimal().rstrip("\n"),
}


def write(directory: Path, files: dict[str, str]) -> None:
    """Write every case, removing a stale Markdown file the manifest no longer lists."""
    directory.mkdir(parents=True, exist_ok=True)
    for existing in directory.glob("*.md"):
        if existing.name not in files:
            existing.unlink()
    for name, content in files.items():
        (directory / name).write_bytes(content.encode("utf-8"))


def main() -> None:
    write(ROOT / "synthetic", VALID)
    write(ROOT / "synthetic-invalid", INVALID)
    print(  # noqa: T201 - a corpus generator reports what it wrote
        f"wrote {len(VALID)} clean and {len(INVALID)} invalid synthetic ADRs under {ROOT}"
    )


if __name__ == "__main__":
    main()
