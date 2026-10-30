use std::path::{Path, PathBuf};

use adrlint::adr::parse;
use adrlint::config::AdrConfig;
use adrlint::discovery::{AdrFile, Directory};
use adrlint::rules::{check_directory, check_structure};

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

#[test]
fn should_read_the_chosen_option_after_a_subsection() {
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
    assert_eq!(findings(source, Path::new("docs/adr/0001-test.md")), []);
}

fn adr_file(name: &str) -> AdrFile {
    AdrFile {
        path: Path::new("docs/adr").join(name),
        name: name.to_owned(),
        number: adrlint::discovery::parse_number(name),
        valid_name: adrlint::discovery::is_valid_name(name),
    }
}

fn directory(name: &str) -> Directory {
    Directory {
        path: PathBuf::from("docs/adr"),
        adrs: vec![adr_file(name)],
        foreign: Vec::new(),
        indexes: Vec::new(),
    }
}

#[test]
fn should_compare_the_first_number_with_the_configured_start() {
    for (start, name, expected) in [
        (0, "0000-zero.md", false),
        (0, "0001-one.md", true),
        (1, "0000-zero.md", true),
        (1, "0001-one.md", false),
    ] {
        let config = AdrConfig {
            start,
            ..AdrConfig::default()
        };
        let found = check_directory(&directory(name), &config)
            .into_iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert_eq!(
            found.contains(&"MADR41a"),
            expected,
            "start {start} with {name} found {found:?}"
        );
    }
}

/// The numbering rules over a real directory listing, for every name shape AGENTS.md requires.
#[test]
fn should_judge_every_file_name_shape_in_one_directory() {
    let temp = tempfile::TempDir::new().expect("temporary directory");
    for name in [
        "0000-zero.md",
        "0001-use-CC0-or-MIT-as-license.md",
        "0002-two.md",
        "0002-also-two.md",
        "0004-after-a-gap.md",
        "005-three-digits.md",
        "00006-five-digits.md",
        "notes.txt",
    ] {
        std::fs::write(temp.path().join(name), "# x\n").expect("writing a file");
    }
    let directory = adrlint::discovery::scan(temp.path()).expect("scanning the directory");
    let config = AdrConfig {
        start: 0,
        ..AdrConfig::default()
    };
    let mut found = check_directory(&directory, &config)
        .into_iter()
        .map(|diagnostic| {
            let name = diagnostic
                .path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            (diagnostic.code, name)
        })
        .collect::<Vec<_>>();
    found.sort();
    assert_eq!(
        found,
        [
            ("MADR41b", "0004-after-a-gap.md".to_owned()),
            ("MADR42", "notes.txt".to_owned()),
            ("MADR43", "0002-also-two.md".to_owned()),
            ("MADR43", "0002-two.md".to_owned()),
            ("MADR44", "00006-five-digits.md".to_owned()),
            ("MADR44", "005-three-digits.md".to_owned()),
        ]
    );
}
