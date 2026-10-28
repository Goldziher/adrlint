//! The structural MADR rules, `MADR01` through `MADR08`.
//!
//! A port of madrlint's `Rule01`–`Rule08`. Section titles, heading levels, canonical order and the
//! chosen-option and rationale keywords are copied from madrlint's `Section` and
//! `DecisionOutcomeElements` enums, so the two tools agree on what a valid ADR is. Every check
//! reads the parsed [`Document`]; nothing here re-reads or re-parses the source.

use std::path::Path;

use crate::adr::{Block, Document, Heading};
use crate::diagnostics::Diagnostic;

/// Keywords that introduce the chosen option inside `Decision Outcome`, lowercased.
const CHOSEN_OPTION: [&str; 3] = ["chosen option", "chosen alternative", "selected option"];

/// Keywords that introduce the rationale for the chosen option, lowercased.
const RATIONALE: [&str; 8] = [
    "rationale",
    "reasoning",
    "reason",
    "because",
    "justification",
    "motivation",
    "motivated",
    "as",
];

/// A section the MADR template defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Context,
    DecisionDrivers,
    ConsideredOptions,
    DecisionOutcome,
    ProsAndCons,
    MoreInformation,
    Consequences,
    Confirmation,
}

impl Section {
    /// Every template section, in canonical order.
    const ALL: [Self; 8] = [
        Self::Context,
        Self::DecisionDrivers,
        Self::ConsideredOptions,
        Self::DecisionOutcome,
        Self::ProsAndCons,
        Self::MoreInformation,
        Self::Consequences,
        Self::Confirmation,
    ];

    /// The sections that sit directly under the title, in template order.
    const TOP_LEVEL: [Self; 6] = [
        Self::Context,
        Self::DecisionDrivers,
        Self::ConsideredOptions,
        Self::DecisionOutcome,
        Self::ProsAndCons,
        Self::MoreInformation,
    ];

    /// The titles madrlint accepts, in its precedence order.
    fn titles(self) -> &'static [&'static str] {
        match self {
            Self::Context => &["Context and Problem Statement", "Context and Problem", "Context"],
            Self::DecisionDrivers => &["Decision Drivers"],
            Self::ConsideredOptions => &[
                "Considered Options",
                "Considered Alternatives",
                "Explored Options",
                "Explored Alternatives",
            ],
            Self::DecisionOutcome => &["Decision Outcome"],
            Self::ProsAndCons => &["Pros and Cons of the Options"],
            Self::MoreInformation => &["More Information", "Information"],
            Self::Consequences => &["Consequences", "Positive Consequences", "Negative Consequences"],
            Self::Confirmation => &["Confirmation"],
        }
    }

    fn title(self) -> &'static str {
        self.titles()[0]
    }

    /// The heading level the template gives the section.
    fn level(self) -> u8 {
        match self {
            Self::Consequences | Self::Confirmation => 3,
            _ => 2,
        }
    }
}

/// Checks every structural MADR rule against one parsed document.
pub fn check_structure(document: &Document, path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_mandatory_sections(document, path, &mut diagnostics);
    check_empty_sections(document, path, &mut diagnostics);
    check_decision_outcome(document, path, &mut diagnostics);
    check_single_title(document, path, &mut diagnostics);
    check_heading_levels(document, path, &mut diagnostics);
    check_nested_sections(document, path, &mut diagnostics);
    check_title_position(document, path, &mut diagnostics);
    check_section_order(document, path, &mut diagnostics);
    diagnostics
}

/// `MADR01a`–`MADR01d`: the title and the three mandatory sections are present.
fn check_mandatory_sections(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    if !document.headings.iter().any(|heading| heading.level == 1) {
        diagnostics.push(Diagnostic::error(
            "MADR01a",
            path,
            1,
            "missing title; add an H1 heading with the decision title",
        ));
    }

    for (code, section) in [
        ("MADR01b", Section::Context),
        ("MADR01c", Section::ConsideredOptions),
        ("MADR01d", Section::DecisionOutcome),
    ] {
        if section_heading(document, section).is_none() {
            diagnostics.push(Diagnostic::error(
                code,
                path,
                1,
                format!("missing or ill-formatted section: {}", section.title()),
            ));
        }
    }
}

/// `MADR02`: no section is empty.
///
/// A section is empty when it has neither block content nor a subsection. Counting subsections as
/// content is what keeps the title, whose body is normally only the sections beneath it, from being
/// reported as empty.
fn check_empty_sections(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    for (index, heading) in document.headings.iter().enumerate() {
        let has_content = document.blocks.iter().any(|block| within(block_start(block), heading));
        let has_subsection = document
            .headings
            .get(index + 1)
            .is_some_and(|next| next.start < heading.body_end);
        if !has_content && !has_subsection {
            diagnostics.push(Diagnostic::error(
                "MADR02",
                path,
                heading.line,
                format!("empty section '{}'; add content under it", heading.text),
            ));
        }
    }
}

/// `MADR03a`, `MADR03b` and `MADR04`: `Decision Outcome` opens with a chosen option and states a
/// rationale for it.
fn check_decision_outcome(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some((index, heading)) = section_heading(document, Section::DecisionOutcome) else {
        return;
    };
    let contents = direct_content(document, index);

    let Some(position) = contents
        .iter()
        .position(|content| keyword_index(&content.text, &CHOSEN_OPTION).is_some())
    else {
        diagnostics.push(Diagnostic::error(
            "MADR03a",
            path,
            heading.line,
            "no chosen option; expected 'Chosen option: \"<option>\", because <rationale>'",
        ));
        return;
    };

    let chosen = &contents[position];
    if position > 0 {
        diagnostics.push(Diagnostic::error(
            "MADR03b",
            path,
            chosen.line,
            "Decision Outcome must start with the chosen option",
        ));
    }

    let chosen_at = keyword_index(&chosen.text, &CHOSEN_OPTION).unwrap_or(0);
    let rationale = keyword_index(&chosen.text, &RATIONALE);
    if rationale.is_none_or(|rationale| rationale < chosen_at) {
        diagnostics.push(Diagnostic::error(
            "MADR04",
            path,
            chosen.line,
            "missing rationale after the chosen option; expected ', because <rationale>'",
        ));
    }
}

/// `MADR05`: only the title uses H1.
fn check_single_title(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    for heading in document.headings.iter().filter(|heading| heading.level == 1).skip(1) {
        diagnostics.push(Diagnostic::error(
            "MADR05",
            path,
            heading.line,
            format!("unexpected H1 heading '{}'; only the title may use H1", heading.text),
        ));
    }
}

/// `MADR06`: a known section keeps the heading level the template gives it.
fn check_heading_levels(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    for section in Section::ALL {
        let Some(heading) = section_heading(document, section).map(|(_, heading)| heading) else {
            continue;
        };
        if heading.level != section.level() {
            diagnostics.push(Diagnostic::error(
                "MADR06",
                path,
                heading.line,
                format!(
                    "expected level {} for '{}', found level {}",
                    section.level(),
                    heading.text,
                    heading.level
                ),
            ));
        }
    }
}

/// `MADR07a` and `MADR07b`: `Consequences` and `Confirmation` are nested under `Decision Outcome`.
fn check_nested_sections(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some((_, outcome)) = section_heading(document, Section::DecisionOutcome) else {
        return;
    };
    for (code, section) in [("MADR07a", Section::Consequences), ("MADR07b", Section::Confirmation)] {
        let Some((_, child)) = section_heading(document, section) else {
            continue;
        };
        if !(child.start >= outcome.body_start && child.start < outcome.body_end) {
            diagnostics.push(Diagnostic::error(
                code,
                path,
                child.line,
                format!(
                    "{} must be a level {} heading under Decision Outcome",
                    section.title(),
                    section.level()
                ),
            ));
        }
    }
}

/// `MADR08a`: the title is the first heading in the file.
fn check_title_position(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let Some(title) = document.headings.iter().find(|heading| heading.level == 1) else {
        return;
    };
    if document.headings.iter().any(|heading| heading.line < title.line) {
        diagnostics.push(Diagnostic::error(
            "MADR08a",
            path,
            title.line,
            "title must be the first heading in the file",
        ));
    }
}

/// `MADR08b`: the level-2 sections follow the template order.
///
/// The sections present are matched against their canonical order, and the longest run that is
/// already in document order is kept; every section outside it is reported. This is madrlint's
/// longest-increasing-subsequence pass, including its tie-breaking, so both tools name the same
/// out-of-order section when several are.
fn check_section_order(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    let present = Section::TOP_LEVEL
        .iter()
        .filter_map(|section| section_heading(document, *section).map(|(_, heading)| (*section, heading.line)))
        .collect::<Vec<_>>();
    if present.len() < 2 {
        return;
    }

    let mut lengths = vec![1; present.len()];
    let mut previous: Vec<Option<usize>> = vec![None; present.len()];
    let mut best_end = 0;
    let mut longest = 0;
    for index in 0..present.len() {
        for candidate in 0..index {
            if present[candidate].1 < present[index].1 && lengths[candidate] + 1 > lengths[index] {
                lengths[index] = lengths[candidate] + 1;
                previous[index] = Some(candidate);
            }
        }
        if lengths[index] > longest {
            longest = lengths[index];
            best_end = index;
        }
    }

    let mut ordered = vec![false; present.len()];
    let mut cursor = Some(best_end);
    while let Some(index) = cursor {
        ordered[index] = true;
        cursor = previous[index];
    }

    for (index, (section, line)) in present.iter().enumerate() {
        if !ordered[index] {
            diagnostics.push(Diagnostic::error(
                "MADR08b",
                path,
                *line,
                format!(
                    "'{}' is out of order; sections must follow the template order",
                    section.title()
                ),
            ));
        }
    }
}

/// One piece of block content directly inside a section, with the line it starts on.
struct Content {
    line: usize,
    text: String,
}

/// The heading madrlint would resolve for a section: the first alias present, and the first heading
/// matching it, with case folded.
fn section_heading(document: &Document, section: Section) -> Option<(usize, &Heading)> {
    section.titles().iter().find_map(|title| {
        document
            .headings
            .iter()
            .enumerate()
            .find(|(_, heading)| heading.text.eq_ignore_ascii_case(title))
    })
}

/// Block content directly inside the heading at `index`, excluding subsections, in document order.
fn direct_content(document: &Document, index: usize) -> Vec<Content> {
    let heading = &document.headings[index];
    let end = document.headings.get(index + 1).map_or(usize::MAX, |next| next.start);

    let mut contents = Vec::new();
    for block in document
        .blocks
        .iter()
        .filter(|block| block_start(block) >= heading.body_start && block_start(block) < end)
    {
        match block {
            Block::List { start, end, depth, .. } => contents.extend(
                document
                    .list_items
                    .iter()
                    .filter(|item| item.start >= *start && item.start < *end && item.depth == *depth)
                    .map(|item| Content {
                        line: item.line,
                        text: item.text.clone(),
                    }),
            ),
            Block::Paragraph { text, line, .. }
            | Block::CodeBlock { text, line, .. }
            | Block::Html { text, line, .. } => contents.push(Content {
                line: *line,
                text: text.clone(),
            }),
            Block::Table { header, rows, line, .. } => {
                let mut text = header.join(" ");
                for row in rows {
                    text.push('\n');
                    text.push_str(&row.join(" "));
                }
                contents.push(Content { line: *line, text });
            }
            Block::Rule { line, .. } => contents.push(Content {
                line: *line,
                text: String::new(),
            }),
        }
    }
    contents
}

/// The index of the first keyword found in `text`, in keyword order, as madrlint does it.
fn keyword_index(text: &str, keywords: &[&str]) -> Option<usize> {
    let text = text.to_lowercase();
    keywords.iter().find_map(|keyword| text.find(keyword))
}

fn block_start(block: &Block) -> usize {
    match block {
        Block::Paragraph { start, .. }
        | Block::List { start, .. }
        | Block::CodeBlock { start, .. }
        | Block::Table { start, .. }
        | Block::Html { start, .. }
        | Block::Rule { start, .. } => *start,
    }
}

fn within(offset: usize, heading: &Heading) -> bool {
    offset >= heading.body_start && offset < heading.body_end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;

    /// A minimal ADR that satisfies every structural rule.
    const VALID: &str = "\
# Use Rust

## Context and Problem Statement

We need a decision.

## Considered Options

* Rust
* Go

## Decision Outcome

Chosen option: \"Rust\", because it is fast.
";

    fn findings(source: &str) -> Vec<(&'static str, usize)> {
        check_structure(&parse(source), Path::new("docs/adr/0001-test.md"))
            .into_iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.line))
            .collect()
    }

    #[test]
    fn should_accept_a_valid_document() {
        assert_eq!(findings(VALID), []);
    }

    #[test]
    fn should_require_the_title_and_the_mandatory_sections() {
        assert_eq!(
            findings("Just a paragraph.\n"),
            [("MADR01a", 1), ("MADR01b", 1), ("MADR01c", 1), ("MADR01d", 1),]
        );
    }

    #[test]
    fn should_accept_every_context_alias() {
        for title in ["Context and Problem Statement", "Context and Problem", "Context"] {
            let source = VALID.replace("Context and Problem Statement", title);
            assert_eq!(findings(&source), [], "{title} should be accepted");
        }
    }

    #[test]
    fn should_accept_every_considered_options_alias() {
        for title in [
            "Considered Options",
            "Considered Alternatives",
            "Explored Options",
            "Explored Alternatives",
        ] {
            let source = VALID.replace("Considered Options", title);
            assert_eq!(findings(&source), [], "{title} should be accepted");
        }
    }

    #[test]
    fn should_match_section_titles_case_insensitively() {
        let source = VALID
            .replace("Considered Options", "considered options")
            .replace("Decision Outcome", "DECISION OUTCOME");
        assert_eq!(findings(&source), []);
    }

    #[test]
    fn should_accept_every_chosen_option_keyword() {
        for keyword in ["Chosen Option", "Chosen Alternative", "Selected Option"] {
            let source = VALID.replace("Chosen option", keyword);
            assert_eq!(findings(&source), [], "{keyword} should be accepted");
        }
    }

    #[test]
    fn should_report_a_missing_chosen_option() {
        let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", "We will use Rust.");
        assert_eq!(findings(&source), [("MADR03a", 12)]);
    }

    #[test]
    fn should_report_a_chosen_option_that_is_not_first() {
        let source = VALID.replace(
            "Chosen option: \"Rust\", because it is fast.",
            "We evaluated two options.\n\nChosen option: \"Rust\", because it is fast.",
        );
        assert_eq!(findings(&source), [("MADR03b", 16)]);
    }

    #[test]
    fn should_report_a_missing_rationale() {
        let source = VALID.replace(
            "Chosen option: \"Rust\", because it is fast.",
            "Chosen option: \"Rust\".",
        );
        assert_eq!(findings(&source), [("MADR04", 14)]);
    }

    #[test]
    fn should_accept_every_rationale_keyword() {
        for rationale in [
            "Rationale: it is fast.",
            "Reasoning: it is fast.",
            "Reason: it is fast.",
            "because it is fast.",
            "Justification: it is fast.",
            "Motivation: it is fast.",
            "Motivated by speed.",
            "as it is fast.",
        ] {
            let source = VALID.replace(
                "Chosen option: \"Rust\", because it is fast.",
                &format!("Chosen option: \"Rust\". {rationale}"),
            );
            assert_eq!(findings(&source), [], "{rationale} should be accepted");
        }
    }

    #[test]
    fn should_read_a_chosen_option_from_a_list() {
        let source = VALID.replace(
            "Chosen option: \"Rust\", because it is fast.",
            "* Chosen option: \"Rust\", because it is fast.",
        );
        assert_eq!(findings(&source), []);
    }

    #[test]
    fn should_report_a_list_item_before_the_chosen_option() {
        let source = VALID.replace(
            "Chosen option: \"Rust\", because it is fast.",
            "* We evaluated one option.\n* Chosen option: \"Rust\", because it is fast.",
        );
        assert_eq!(findings(&source), [("MADR03b", 15)]);
    }

    #[test]
    fn should_report_an_empty_section() {
        let source = VALID.replace("We need a decision.\n\n", "");
        assert_eq!(findings(&source), [("MADR02", 3)]);
    }

    #[test]
    fn should_not_treat_the_title_as_empty() {
        let source = "# Title\n\n## Context\n\nContext text.\n\n## Considered Options\n\n* One\n\n## Decision Outcome\n\nChosen option: \"One\", because it works.\n";
        assert_eq!(findings(source), []);
    }

    #[test]
    fn should_report_a_second_h1() {
        let source = format!("{VALID}\n# Another title\n\nExtra text.\n");
        assert_eq!(findings(&source), [("MADR05", 16)]);
    }

    #[test]
    fn should_report_a_wrong_heading_level() {
        let source = VALID.replace("## Considered Options", "### Considered Options");
        assert_eq!(findings(&source), [("MADR06", 7)]);
    }

    #[test]
    fn should_report_consequences_outside_the_decision_outcome() {
        let source = VALID.replace("* Go\n", "* Go\n\n### Consequences\n\nContent.\n");
        assert_eq!(findings(&source), [("MADR07a", 12)]);
    }

    #[test]
    fn should_report_confirmation_outside_the_decision_outcome() {
        let source = VALID.replace("* Go\n", "* Go\n\n### Confirmation\n\nContent.\n");
        assert_eq!(findings(&source), [("MADR07b", 12)]);
    }

    #[test]
    fn should_accept_nested_consequences_and_confirmation() {
        let source = format!("{VALID}\n### Consequences\n\nContent.\n\n### Confirmation\n\nMore content.\n");
        assert_eq!(findings(&source), []);
    }

    #[test]
    fn should_report_a_title_that_is_not_first() {
        let source = "## Context\n\nContext text.\n\n# Title\n\n## Considered Options\n\n* One\n\n## Decision Outcome\n\nChosen option: \"One\", because it works.\n";
        assert_eq!(findings(source), [("MADR08a", 5)]);
    }

    #[test]
    fn should_report_sections_out_of_template_order() {
        let source = "\
# Title

## Context

Context text.

## Decision Outcome

Chosen option: \"One\", because it works.

## Considered Options

* One
";
        assert_eq!(findings(source), [("MADR08b", 7)]);
    }
}
