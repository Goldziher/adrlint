//! Front matter must not defeat the parse or exhaust the process.
//!
//! A BOM at byte 0 used to hide the opening fence, and a hostile YAML block used to recurse until
//! the stack overflowed. Both are reported as ordinary findings so a lint run never aborts.

use std::path::Path;

use adrlint::adr::parse;
use adrlint::rules::{check_document, check_metadata};

const FIXTURE: &str = "fixtures/adr/valid/0001-record-architecture-decisions.md";
const TITLE: &str = "# Record architecture decisions";

fn valid_adr() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE)).expect("reading the valid fixture")
}

/// The fixture body, without its front matter.
fn body() -> String {
    let source = valid_adr();
    let start = source.find(TITLE).expect("the fixture title");
    source[start..].to_owned()
}

fn with_front_matter(block: &str) -> String {
    format!("---\n{block}---\n{}", body())
}

fn metadata(source: &str) -> Vec<adrlint::diagnostics::Diagnostic> {
    check_metadata(&parse(source), Path::new("docs/adr/0001-test.md"))
}

#[test]
fn should_lint_a_bom_prefixed_valid_adr_clean() {
    let source = format!("\u{feff}{}", valid_adr());
    let path = Path::new("docs/adr/0001-record-architecture-decisions.md");
    let findings = check_document(&parse(&source), path, Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(
        findings.is_empty(),
        "a BOM must not change the verdict, found {findings:?}"
    );
}

#[test]
fn should_report_madr21_for_an_oversized_front_matter_block() {
    let block = (0..7000)
        .map(|index| format!("key{index}: value\n"))
        .collect::<String>();
    assert!(block.len() > 64 * 1024, "the fixture must exceed the limit");

    let findings = metadata(&with_front_matter(&block));
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "MADR21");
    assert_eq!(findings[0].line, 2);
    assert!(findings[0].message.contains("limit"), "{}", findings[0].message);
}

#[test]
fn should_report_madr21_for_deeply_nested_front_matter() {
    let mut block = String::new();
    for level in 0..200 {
        block.push_str(&"  ".repeat(level));
        block.push_str(&format!("key{level}:\n"));
    }
    assert!(block.len() < 64 * 1024, "the fixture must stay under the size limit");

    let findings = metadata(&with_front_matter(&block));
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert_eq!(findings[0].code, "MADR21");
    assert_eq!(
        findings[0].line, 67,
        "the error should point at the line that first exceeds the depth limit"
    );
    assert!(findings[0].message.contains("levels"), "{}", findings[0].message);
}
