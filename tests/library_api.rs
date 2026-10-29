//! The public library surface, exercised from outside the crate.
//!
//! These tests use only the re-exports at the crate root, so a rename or a missing export breaks the
//! build rather than silently narrowing the API downstream consumers such as poly rely on.

use std::path::Path;

use adrlint::config::MarkdownConfig;
use adrlint::diagnostics::Severity;

const VALID: &str = "\
# Use Rust

## Context and Problem Statement

We need a fast CLI.

## Considered Options

* Rust
* Go

## Decision Outcome

Chosen option: \"Rust\", because it is fast.
";

#[test]
fn should_parse_and_lint_a_document_through_the_root_exports() {
    let document = adrlint::parse_adr(VALID);
    assert_eq!(document.headings.len(), 4);

    let findings = adrlint::check_document(&document, Path::new("0001-use-rust.md"), Path::new("."));
    assert!(
        findings.iter().all(|finding| !finding.severity.is_error()),
        "{findings:?}"
    );
}

#[test]
fn should_report_a_structural_error_through_the_root_exports() {
    let source = VALID.replace("## Context and Problem Statement", "## Background");
    let document = adrlint::parse_adr(&source);
    let findings = adrlint::check_document(&document, Path::new("0001-use-rust.md"), Path::new("."));

    assert!(
        findings
            .iter()
            .any(|finding| finding.code == "MADR01b" && finding.severity == Severity::Error),
        "{findings:?}"
    );
}

#[test]
fn should_hash_formatting_variants_identically_through_the_root_export() {
    let bullets = VALID.replace("* Rust", "- Rust").replace("* Go", "- Go");
    assert_eq!(
        adrlint::canonical_hash(&adrlint::parse_adr(VALID)),
        adrlint::canonical_hash(&adrlint::parse_adr(&bullets))
    );
}

#[test]
fn should_format_markdown_through_the_root_export() {
    let source = "# Use Rust   \n\nBody without a final newline";
    let formatted = adrlint::format_markdown(source, Path::new("0001-use-rust.md"), &MarkdownConfig::default())
        .expect("formatting succeeds");

    assert!(formatted.ends_with('\n'), "{formatted:?}");
    assert!(!formatted.contains("Rust   "), "{formatted:?}");
    assert_eq!(
        formatted,
        adrlint::format_markdown(&formatted, Path::new("0001-use-rust.md"), &MarkdownConfig::default()).unwrap()
    );
}
