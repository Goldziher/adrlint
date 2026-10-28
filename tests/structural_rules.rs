use std::path::{Path, PathBuf};

use adrlint::adr::parse;
use adrlint::rules::check_structure;

fn fixture_path(directory: &Path) -> PathBuf {
    std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "md"))
        .unwrap_or_else(|| panic!("no Markdown fixture in {}", directory.display()))
}

fn findings(source: &str, path: &Path) -> Vec<(&'static str, usize)> {
    check_structure(&parse(source), path)
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
fn should_report_the_expected_code_and_line_for_every_structural_rule() {
    for (code, line) in [
        ("MADR01a", 1),
        ("MADR01b", 1),
        ("MADR01c", 1),
        ("MADR01d", 1),
        ("MADR02", 7),
        ("MADR03a", 11),
        ("MADR03b", 15),
        ("MADR04", 13),
        ("MADR05", 15),
        ("MADR06", 7),
        ("MADR07a", 11),
        ("MADR07b", 11),
        ("MADR08a", 5),
        ("MADR08b", 7),
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
