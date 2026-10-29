use std::path::{Path, PathBuf};

use adrlint::adr::parse;
use adrlint::rules::check_document;
use tempfile::TempDir;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixture_path(directory: &Path) -> PathBuf {
    std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "md"))
        .unwrap_or_else(|| panic!("no Markdown fixture in {}", directory.display()))
}

fn findings(source: &str, path: &Path) -> Vec<(&'static str, usize)> {
    check_document(&parse(source), path, &root())
        .into_iter()
        .map(|diagnostic| (diagnostic.code, diagnostic.line))
        .collect()
}

fn invalid(code: &str) -> (String, PathBuf) {
    let path = fixture_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/adr/invalid")
            .join(code),
    );
    let source = std::fs::read_to_string(&path).expect("reading fixture");
    (source, path)
}

#[test]
fn should_report_the_expected_code_and_line_for_every_link_metadata_and_style_rule() {
    for (code, line) in [
        ("MADR12a", 5),
        ("MADR12b", 5),
        ("MADR12c", 5),
        ("MADR12d", 5),
        ("MADR21", 3),
        ("MADR31", 9),
    ] {
        let (source, path) = invalid(code);
        assert_eq!(
            findings(&source, &path),
            [(code, line)],
            "fixture {code} should report only {code} on line {line}"
        );
    }
}

#[test]
fn should_accept_the_valid_fixtures() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr/valid");
    for name in ["0001-record-architecture-decisions.md", "0002-use-rust-for-the-cli.md"] {
        let path = directory.join(name);
        let source = std::fs::read_to_string(&path).expect("reading fixture");
        let found = findings(&source, &path);
        assert!(found.is_empty(), "{name} should be clean, found {found:?}");
    }
}

/// A valid ADR whose `More Information` section holds the links under test.
fn adr_with_links(links: &str) -> String {
    format!(
        "\
# Links

## Context and Problem Statement

We need to decide how links are checked.

## Considered Options

* Check them

## Decision Outcome

Chosen option: \"Check them\", because broken links are worse than none.

## More Information

{links}
"
    )
}

fn link_codes(root: &Path, links: &str) -> Vec<&'static str> {
    let path = root.join("docs/adr/0001-links.md");
    check_document(&parse(&adr_with_links(links)), &path, root)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn should_skip_protocol_relative_links() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let codes = link_codes(root, "* [passwd](//etc/passwd)\n* [web](//example.com/x)\n");
    assert!(
        codes.is_empty(),
        "protocol-relative links should be skipped, found {codes:?}"
    );
}

#[test]
fn should_accept_a_root_relative_path_under_the_root() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/note.md"), "# Note\n").unwrap();

    let codes = link_codes(root, "* [note](/docs/note.md)\n");
    assert!(
        codes.is_empty(),
        "an existing root-relative path should pass, found {codes:?}"
    );
}

#[test]
fn should_report_a_missing_root_relative_path() {
    let directory = TempDir::new().unwrap();
    let root = directory.path();
    let codes = link_codes(root, "* [missing](/docs/missing.md)\n");
    assert_eq!(codes, ["MADR12c"]);
}
