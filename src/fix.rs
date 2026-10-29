//! Safe, deterministic autofixes for structural MADR findings.
//!
//! The only structural fix is the heading level of `Consequences` and `Confirmation` when it is
//! unambiguously one level too shallow: a `##` heading that follows `## Decision Outcome` with no
//! other section in between. Everything else (numbering, file names, decision text) is left to the
//! author. Formatting fixes come from rumdl through [`crate::markdown::format`].

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;

use crate::adr::{self, Document};
use crate::config::MarkdownConfig;
use crate::diagnostics::Diagnostic;
use crate::markdown;

/// Titles of the sections the template nests one level below `Decision Outcome`.
const NESTED_TITLES: [&str; 4] = [
    "Consequences",
    "Positive Consequences",
    "Negative Consequences",
    "Confirmation",
];

/// Lines of ATX `##` headings that should be `###`.
pub fn misplaced_heading_lines(document: &Document, source: &str) -> BTreeSet<usize> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut fixes = BTreeSet::new();
    let mut under_outcome = false;
    for heading in &document.headings {
        match heading.level {
            1 => under_outcome = false,
            2 => {
                let nested = NESTED_TITLES
                    .iter()
                    .any(|title| heading.text.eq_ignore_ascii_case(title));
                if under_outcome && nested && is_atx_level_two(&lines, heading.line) {
                    fixes.insert(heading.line);
                } else {
                    under_outcome = heading.text.eq_ignore_ascii_case("Decision Outcome");
                }
            }
            _ => {}
        }
    }
    fixes
}

fn is_atx_level_two(lines: &[&str], line: usize) -> bool {
    let Some(text) = line.checked_sub(1).and_then(|index| lines.get(index)) else {
        return false;
    };
    text.trim_start()
        .strip_prefix("##")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// Deepens every heading in [`misplaced_heading_lines`] by one level, leaving all other bytes alone.
pub fn apply_structural(source: &str, document: &Document) -> String {
    let lines = misplaced_heading_lines(document, source);
    if lines.is_empty() {
        return source.to_owned();
    }
    let mut fixed = String::with_capacity(source.len() + lines.len());
    for (index, line) in source.split_inclusive('\n').enumerate() {
        if lines.contains(&(index + 1)) {
            let indent = line.len() - line.trim_start().len();
            fixed.push_str(&line[..indent]);
            fixed.push('#');
            fixed.push_str(&line[indent..]);
        } else {
            fixed.push_str(line);
        }
    }
    fixed
}

/// Marks the findings `lint --fix` resolves for this document as fixable.
///
/// `MADR06` and `MADR07` findings are fixable on the lines [`misplaced_heading_lines`] returns.
pub fn mark_fixable(findings: &mut [Diagnostic], document: &Document, source: &str) {
    let lines = misplaced_heading_lines(document, source);
    if lines.is_empty() {
        return;
    }
    for finding in findings.iter_mut() {
        if (finding.code == "MADR06" || finding.code.starts_with("MADR07")) && lines.contains(&finding.line) {
            finding.fixable = true;
        }
    }
}

/// The fixed form of one ADR.
///
/// With `structural` the heading fixes run first, then rumdl's fixes. Without it only rumdl runs,
/// which is what `format` does. A document nothing applies to comes back byte-identical.
pub fn fix_source(source: &str, path: &Path, markdown: &MarkdownConfig, structural: bool) -> Result<String> {
    if structural {
        let staged = apply_structural(source, &adr::parse(source));
        return markdown::format(&staged, path, markdown);
    }
    markdown::format(source, path, markdown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;

    const BASE: &str = "# T\n\n## Decision Outcome\n\nChosen option: \"x\", because y.\n\n";

    fn lines(source: &str) -> Vec<usize> {
        misplaced_heading_lines(&parse(source), source).into_iter().collect()
    }

    #[test]
    fn should_deepen_consequences_that_follow_the_outcome() {
        let source = format!("{BASE}## Consequences\n\n* Good, because z\n");
        assert_eq!(lines(&source), [7]);
        let fixed = apply_structural(&source, &parse(&source));
        assert!(fixed.contains("\n### Consequences\n"), "{fixed}");
        assert!(lines(&fixed).is_empty());
    }

    #[test]
    fn should_deepen_confirmation_after_nested_consequences() {
        let source = format!("{BASE}### Consequences\n\n* Good\n\n## Confirmation\n\nText.\n");
        assert_eq!(lines(&source), [11]);
    }

    #[test]
    fn should_not_guess_when_another_section_intervenes() {
        let source = format!("{BASE}## Pros and Cons of the Options\n\nText.\n\n## Consequences\n\n* Good\n");
        assert!(lines(&source).is_empty());
    }

    #[test]
    fn should_leave_setext_headings_alone() {
        let source = format!("{BASE}Consequences\n------------\n\n* Good\n");
        assert!(lines(&source).is_empty());
    }

    #[test]
    fn should_preserve_crlf_when_deepening() {
        let source = format!("{BASE}## Consequences\n\n* Good\n").replace('\n', "\r\n");
        let fixed = apply_structural(&source, &parse(&source));
        assert!(fixed.contains("\r\n### Consequences\r\n"), "{fixed:?}");
    }

    #[test]
    fn should_mark_matching_findings_fixable() {
        let source = format!("{BASE}## Consequences\n\n* Good\n");
        let mut findings = vec![
            Diagnostic::error("MADR06", "0001-x.md", 7, "level"),
            Diagnostic::error("MADR07a", "0001-x.md", 7, "nested"),
            Diagnostic::error("MADR06", "0001-x.md", 3, "other"),
        ];
        mark_fixable(&mut findings, &parse(&source), &source);
        assert_eq!(
            findings.iter().map(|finding| finding.fixable).collect::<Vec<_>>(),
            [true, true, false]
        );
    }
}
