//! The metadata rules, `MADR21`.
//!
//! A port of madrlint's `Rule21`: the YAML front matter must parse. The document model already
//! carries the parser's verdict, so this rule only turns it into a finding.

use std::path::Path;

use crate::adr::Document;
use crate::diagnostics::Diagnostic;

/// Reports the front matter's YAML error, if it has one.
///
/// Unlike madrlint, the parser's message is reported verbatim rather than capitalised: a linter
/// does not rewrite the diagnostics it relays.
pub fn check_metadata(document: &Document, path: &Path) -> Vec<Diagnostic> {
    let Some(error) = document
        .front_matter
        .as_ref()
        .and_then(|front_matter| front_matter.error.as_ref())
    else {
        return Vec::new();
    };

    vec![Diagnostic::error(
        "MADR21",
        path,
        error.line,
        format!("YAML front matter: {}", error.message.trim()),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;

    #[test]
    fn should_report_a_yaml_error_on_its_file_line() {
        let source = "---\ntitle: ok\n  bad: indent\n---\n\n# Title\n";
        let diagnostics = check_metadata(&parse(source), Path::new("docs/adr/0001-test.md"));

        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.code, "MADR21");
        assert_eq!(diagnostic.line, 3);
        assert!(diagnostic.message.starts_with("YAML front matter: "));
    }

    #[test]
    fn should_accept_missing_and_valid_front_matter() {
        for source in ["# Title\n", "---\nstatus: accepted\n---\n# Title\n"] {
            assert!(check_metadata(&parse(source), Path::new("docs/adr/0001-test.md")).is_empty());
        }
    }
}
