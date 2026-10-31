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

/// Keywords that introduce the rationale for the chosen option, lowercased. Each matches as a whole
/// word, so the plurals are listed.
///
/// `as` is not listed: it counts only in the `, as ...` form that introduces the rationale.
const RATIONALE: [&str; 11] = [
    "rationale",
    "rationales",
    "reasoning",
    "reason",
    "reasons",
    "because",
    "justification",
    "justifications",
    "motivation",
    "motivations",
    "motivated",
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
    let contents = section_content(document, index);

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

    let text = chosen.text.to_lowercase();
    let rationale_from = keyword_span(&text, &CHOSEN_OPTION, 0).map_or(0, |(_, end)| option_end(&text, end));
    if keyword_span(&text, &RATIONALE, rationale_from).is_none() && !as_introduces_rationale(&text, rationale_from) {
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
///
/// Every heading whose text names a template section is checked, not only the first one: a document
/// may repeat an alias (`## Context and Problem Statement` plus a nested `### Context`), and each
/// wrong level is a separate defect. `section_for_title` deliberately matches by text alone, while
/// the identity-based rules keep using [`section_heading`]'s first-match semantics.
fn check_heading_levels(document: &Document, path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    for heading in &document.headings {
        let Some(section) = section_for_title(&heading.text) else {
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

/// The template section a heading's text names, if any, regardless of the heading's level.
fn section_for_title(title: &str) -> Option<Section> {
    Section::ALL
        .into_iter()
        .find(|section| section.titles().iter().any(|alias| title.eq_ignore_ascii_case(alias)))
}

/// The content belonging to the heading at `index`: everything in its section span, including the
/// content of nested subsections and excluding the following heading of the same or higher level.
///
/// The span is [`Heading::body_end`], the same boundary `MADR02` uses, so a `### Consequences`
/// subsection does not truncate the parent `Decision Outcome` before its chosen option.
fn section_content(document: &Document, index: usize) -> Vec<Content> {
    let heading = &document.headings[index];
    let end = heading.body_end;

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
/// The byte span of the first keyword in `text` (already lowercased) at or after `from`, matched as
/// a whole word, so `as` is not found inside `basemind`.
fn keyword_span(text: &str, keywords: &[&str], from: usize) -> Option<(usize, usize)> {
    let is_word = |character: char| character.is_alphanumeric();
    keywords
        .iter()
        .filter_map(|keyword| {
            text.match_indices(keyword)
                .map(|(start, _)| (start, start + keyword.len()))
                .find(|&(start, end)| {
                    start >= from
                        && !text[..start].chars().next_back().is_some_and(is_word)
                        && !text[end..].chars().next().is_some_and(is_word)
                })
        })
        .min()
}

fn keyword_index(text: &str, keywords: &[&str]) -> Option<usize> {
    keyword_span(&text.to_lowercase(), keywords, 0).map(|(start, _)| start)
}

/// Where the option named after the chosen-option keyword ends, so a keyword inside the option's
/// name is not read as its rationale: after the closing quote when the option is quoted
/// (`Chosen option: "…"`), else at the first comma, else at the end of the keyword
/// (`keyword_end`). A quote later in the sentence belongs to the rationale, not the option.
fn option_end(text: &str, keyword_end: usize) -> usize {
    let rest = &text[keyword_end..];
    let open = rest.len() - rest.trim_start_matches([':', ' ', '\t']).len();
    let quoted = rest[open..]
        .strip_prefix('"')
        .and_then(|name| name.find('"'))
        .map(|close| keyword_end + open + 1 + close + 1);
    quoted
        .or_else(|| rest.find(',').map(|comma| keyword_end + comma))
        .unwrap_or(keyword_end)
}

/// Whether `, as` right after the option introduces the rationale, as `, because` does.
///
/// `as` is not a rationale keyword on its own: `Chosen option: "a seam", with ort as the default`
/// and `Chosen option: "Redis as a cache"` must still be reported as missing one.
fn as_introduces_rationale(text: &str, from: usize) -> bool {
    let Some(rest) = text[from..].trim_start().strip_prefix(',') else {
        return false;
    };
    let rest = rest.trim_start();
    rest.strip_prefix("as").is_some_and(|after| {
        !after
            .chars()
            .next()
            .is_some_and(|character| character.is_alphanumeric())
    })
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
        ] {
            let source = VALID.replace(
                "Chosen option: \"Rust\", because it is fast.",
                &format!("Chosen option: \"Rust\". {rationale}"),
            );
            assert_eq!(findings(&source), [], "{rationale} should be accepted");
        }
    }

    #[test]
    fn should_not_take_a_keyword_inside_a_word_as_a_rationale() {
        for lead in [
            "Chosen option: \"basemind\", which indexes the code.",
            "Chosen option: \"Rust\", treasonably fast.",
            "Chosen option: \"Rust\", unjustifiably asserted.",
            "Chosen option: \"Rust\", becausewe like it.",
        ] {
            let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", lead);
            assert_eq!(findings(&source), [("MADR04", 14)], "{lead}");
        }
    }

    #[test]
    fn should_accept_a_plural_rationale_keyword() {
        for lead in [
            "Chosen option: \"Rust\", for two reasons: it is fast and safe.",
            "Chosen option: \"Rust\". Rationales: speed and safety.",
            "Chosen option: \"Rust\". Justifications: speed and safety.",
            "Chosen option: \"Rust\". Motivations: speed and safety.",
        ] {
            let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", lead);
            assert_eq!(findings(&source), [], "{lead}");
        }
    }

    #[test]
    fn should_not_take_a_keyword_in_the_option_name_as_a_rationale() {
        for lead in [
            "Chosen option: \"Use X as Y\".",
            "Chosen option: \"Reason about it\".",
            "Chosen option: Redis as a cache, deployed once.",
            "Chosen option: \"a seam\", with ort as the default backend.",
            "Chosen option: \"Redis as a cache\".",
            "Chosen option: \"Redis\".",
            "Chosen option: **Redis as a cache**.",
        ] {
            let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", lead);
            assert_eq!(findings(&source), [("MADR04", 14)], "{lead}");
        }
        for lead in [
            "Chosen option: \"Use X as Y\", because it is fast.",
            "Chosen option: Rust, because it is \"fast\".",
            "Chosen option: \"Rust\", as it is fast.",
            "Chosen option: Rust, as it is fast.",
            "Chosen option: **Rust**, as it is fast.",
            "Chosen option: `Rust`, because it is fast.",
        ] {
            let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", lead);
            assert_eq!(findings(&source), [], "{lead}");
        }
    }

    #[test]
    fn should_require_a_comma_before_as() {
        for lead in [
            "Chosen option: \"Rust\". as it is fast.",
            "Chosen option: \"Rust\", and it is as fast as the alternative.",
        ] {
            let source = VALID.replace("Chosen option: \"Rust\", because it is fast.", lead);
            assert_eq!(findings(&source), [("MADR04", 14)], "{lead}");
        }
    }

    #[test]
    fn should_find_the_rationale_after_the_chosen_option() {
        let source = VALID.replace(
            "Chosen option: \"Rust\", because it is fast.",
            "The rationale follows. Chosen option: \"Rust\", because it is fast.",
        );
        assert_eq!(findings(&source), []);
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
    fn should_check_every_heading_that_names_a_section() {
        let source = "\
# Title

## Context and Problem Statement

Context text.

### Context

More context.

## Considered Options

* One

## Decision Outcome

Chosen option: \"One\", because it works.
";
        assert_eq!(findings(source), [("MADR06", 7)]);
    }

    #[test]
    fn should_check_the_whole_section_for_the_chosen_option() {
        let source = "\
# Use Rust

## Context and Problem Statement

We need a decision.

## Considered Options

* Rust

## Decision Outcome

### Consequences

Chosen option: \"Rust\", because it is fast.
";
        assert_eq!(findings(source), []);
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
