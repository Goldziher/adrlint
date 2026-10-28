//! The style rules, `MADR31`: every unordered list item uses `*` as its marker.
//!
//! A port of madrlint's `Rule31`. Ordered lists are not checked: their marker carries the number a
//! renderer starts from.

use std::path::Path;

use crate::adr::Document;
use crate::diagnostics::Diagnostic;

/// The only marker an unordered list item may use.
const MARKER: &str = "*";

/// Reports every unordered list item written with `-` or `+`.
pub fn check_style(document: &Document, path: &Path) -> Vec<Diagnostic> {
    document
        .list_items
        .iter()
        .filter(|item| !item.ordered && item.marker != MARKER)
        .map(|item| {
            Diagnostic::error(
                "MADR31",
                path,
                item.line,
                format!("expected '{MARKER}' as the list marker, found '{}'", item.marker),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;

    #[test]
    fn should_only_check_unordered_list_items() {
        let source = "* one\n- two\n+ three\n1. four\n  * five\n";
        let diagnostics = check_style(&parse(source), Path::new("docs/adr/0001-test.md"));

        let findings = diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.line))
            .collect::<Vec<_>>();
        assert_eq!(findings, [("MADR31", 2), ("MADR31", 3)]);
    }

    #[test]
    fn should_accept_a_document_with_only_star_markers() {
        assert!(check_style(&parse("* one\n* two\n"), Path::new("docs/adr/0001-test.md")).is_empty());
    }
}
