//! Locations, spans, hints and the closing summary of `lint` output.

use std::fs;
use std::process::{Command, Output};

use tempfile::TempDir;

const ADR: &str = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md");

/// The valid ADR with `Consequences` and `Confirmation` one level too shallow.
fn misplaced() -> String {
    ADR.replace("### Consequences", "## Consequences")
        .replace("### Confirmation", "## Confirmation")
}

fn project(adrs: &[(&str, String)]) -> TempDir {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = []\n",
    )
    .expect("writing the configuration");
    fs::create_dir_all(directory.path().join("docs/adr")).expect("creating the ADR folder");
    for (name, contents) in adrs {
        fs::write(directory.path().join("docs/adr").join(name), contents).expect("writing the ADR");
    }
    directory
}

fn run(directory: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
        .current_dir(directory.path())
        .args(args)
        .output()
        .expect("running adrlint")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn findings(output: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!("parsing JSON: {error}\nstdout: {}\nstderr: {}", stdout(output), stderr(output))
    })
}

#[test]
fn should_add_span_help_and_fixable_to_json_without_dropping_fields() {
    let directory = project(&[("0001-x.md", misplaced())]);
    let output = run(&directory, &["lint", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));

    let found = findings(&output);
    let level = found
        .iter()
        .find(|finding| finding["code"] == "MADR06")
        .unwrap_or_else(|| panic!("MADR06 expected: {}", stdout(&output)));
    for field in ["code", "severity", "message", "path", "line", "column"] {
        assert!(!level[field].is_null(), "{field} is part of the stable contract: {level}");
    }
    assert_eq!(level["end_line"], level["line"], "{level}");
    let column = level["column"].as_u64().expect("a column");
    let end_column = level["end_column"].as_u64().expect("an end column");
    assert!(end_column > column, "{level}");
    assert!(level["help"].as_str().is_some_and(|help| !help.is_empty()), "{level}");
    assert_eq!(level["fixable"], true, "{level}");
}

#[test]
fn should_mark_numbering_findings_as_not_fixable() {
    let directory = project(&[("0001-x.md", ADR.to_owned()), ("0003-y.md", ADR.to_owned())]);
    let output = run(&directory, &["lint", "--format", "json"]);
    let found = findings(&output);
    let gap = found
        .iter()
        .find(|finding| finding["code"] == "MADR41b")
        .unwrap_or_else(|| panic!("MADR41b expected: {}", stdout(&output)));
    assert_eq!(gap["fixable"], false, "{gap}");
    assert!(gap["help"].is_string(), "{gap}");
}

#[test]
fn should_quote_the_offending_line_in_text_output() {
    let directory = project(&[("0001-x.md", misplaced())]);
    let output = run(&directory, &["lint"]);
    let text = stdout(&output);
    assert!(text.contains("error [MADR06]"), "{text}");
    assert!(text.contains("| ## Consequences"), "{text}");
    assert!(
        text.lines().any(|line| line.trim_start().starts_with("| ^^^")),
        "an underline is drawn: {text}"
    );
    assert!(text.contains("= help: "), "{text}");
}

#[test]
fn should_print_a_summary_to_stderr_in_text_mode_only() {
    let directory = project(&[("0001-x.md", misplaced())]);
    let output = run(&directory, &["lint"]);
    let summary = stderr(&output);
    assert!(summary.contains("errors"), "{summary}");
    assert!(summary.contains("1 file checked"), "{summary}");
    assert!(summary.contains("fixable with `adrlint lint --fix`"), "{summary}");

    let output = run(&directory, &["lint", "--format", "json"]);
    assert!(!stderr(&output).contains("files checked") && !stderr(&output).contains("file checked"));
}

#[test]
fn should_report_a_clean_run_in_the_summary() {
    let directory = project(&[("0001-x.md", ADR.to_owned())]);
    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stdout(&output).is_empty(), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("no problems found in 1 file checked"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn should_put_the_end_position_in_github_annotations() {
    let directory = project(&[("0001-x.md", misplaced())]);
    let output = run(&directory, &["lint", "--format", "github"]);
    let text = stdout(&output);
    assert!(text.contains(",endLine="), "{text}");
    assert!(text.contains(",endColumn="), "{text}");
    assert!(text.contains("::[MADR06] "), "{text}");
}
