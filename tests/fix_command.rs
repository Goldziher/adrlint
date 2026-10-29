//! `adrlint lint --fix`: safe fixes, then a fresh lint of what remains.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const ADR: &str = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md");

fn project(adr: &str) -> (TempDir, PathBuf) {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = []\n",
    )
    .expect("writing the configuration");
    let path = directory.path().join("docs/adr/0001-x.md");
    fs::create_dir_all(path.parent().expect("a parent")).expect("creating the ADR folder");
    fs::write(&path, adr).expect("writing the ADR");
    (directory, path)
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

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("reading the ADR")
}

fn misplaced() -> String {
    ADR.replace("### Consequences", "## Consequences")
        .replace("### Confirmation", "## Confirmation")
}

#[test]
fn should_fix_misplaced_heading_levels() {
    let (directory, path) = project(&misplaced());
    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));

    let output = run(&directory, &["lint", "--fix"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(stderr(&output).contains("fixed "), "{}", stderr(&output));
    let fixed = read(&path);
    assert!(fixed.contains("\n### Consequences\n"), "{fixed}");
    assert!(fixed.contains("\n### Confirmation\n"), "{fixed}");

    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
}

#[test]
fn should_not_apply_structural_fixes_when_formatting() {
    let (directory, path) = project(&misplaced());
    let output = run(&directory, &["format"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(read(&path).contains("\n## Consequences\n"));
    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
}

#[test]
fn should_apply_what_format_applies() {
    let broken = ADR
        .replace("* ", "- ")
        .replace("# Record architecture decisions", "# Record architecture decisions   ");
    let (directory, path) = project(&broken);
    let output = run(&directory, &["lint", "--fix"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
    let fixed = read(&path);
    assert!(
        fixed.contains("* Good, because the rationale survives staff changes"),
        "{fixed}"
    );
    assert!(!fixed.contains("decisions   \n"), "{fixed}");
}

#[test]
fn should_be_idempotent() {
    let (directory, path) = project(&misplaced());
    let first = run(&directory, &["lint", "--fix"]);
    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    let fixed = read(&path);

    let second = run(&directory, &["lint", "--fix"]);
    assert_eq!(second.status.code(), Some(0), "{}", stderr(&second));
    assert!(!stderr(&second).contains("fixed "), "{}", stderr(&second));
    assert_eq!(read(&path), fixed);
}

#[test]
fn should_keep_json_output_clean_while_fixing() {
    let (directory, _) = project(&misplaced());
    let output = run(&directory, &["lint", "--fix", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let found: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("JSON on stdout");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn should_report_what_remains_after_fixing() {
    let (directory, _) = project(&misplaced().replace("## Decision Outcome", "## Decision Result"));
    let output = run(&directory, &["lint", "--fix", "--format", "json"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let found: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("JSON on stdout");
    assert!(
        found.iter().any(|finding| finding["code"] == "MADR01d"),
        "the missing Decision Outcome is not fixable and stays: {found:?}"
    );
}

#[test]
fn should_refuse_to_change_locked_content_without_force() {
    let directory = TempDir::new().expect("temporary directory");
    let init = run(&directory, &["init"]);
    assert!(init.status.success(), "{}", stderr(&init));
    let path = directory.path().join("docs/adr/0001-record-architecture-decisions.md");
    let source = read(&path);
    fs::write(&path, format!("{source}\nSee https://example.com for details.\n")).expect("writing the ADR");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[markdown.rules]\nMD034 = \"warn\"\n\n[rules]\nMD034 = \"warn\"\n",
    )
    .expect("writing the configuration");

    let output = run(&directory, &["lock"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let locked = fs::read(&path).expect("reading the locked ADR");

    let output = run(&directory, &["lint", "--fix"]);
    assert!(
        stderr(&output).contains("refusing to change locked content"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read(&path).unwrap(), locked, "a refused fix must not write");
    assert_eq!(
        output.status.code(),
        Some(1),
        "a refused fix fails the run: {}",
        stderr(&output)
    );

    let output = run(&directory, &["lint", "--fix", "--force"]);
    assert!(stderr(&output).contains("fixed "), "{}", stderr(&output));
    assert!(read(&path).contains("<https://example.com>"));
}

#[test]
fn should_require_fix_for_force() {
    let (directory, _) = project(ADR);
    let output = run(&directory, &["lint", "--force"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
}

#[test]
fn should_fix_only_the_files_a_hook_names() {
    let (directory, first) = project(&misplaced());
    let second = directory.path().join("docs/adr/0002-y.md");
    fs::write(&second, misplaced()).expect("writing the second ADR");

    let output = run(&directory, &["lint", "--fix", "docs/adr/0002-y.md"]);
    assert_ne!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        read(&second).contains("\n### Consequences\n"),
        "the named file is fixed"
    );
    assert_eq!(
        read(&first),
        misplaced(),
        "a sibling the hook did not name keeps its unstaged edits"
    );
}

#[cfg(unix)]
#[test]
fn should_write_nothing_when_a_planned_file_is_a_symlink() {
    let (directory, first) = project(&misplaced());
    let target = directory.path().join("outside.md");
    fs::write(&target, misplaced()).expect("writing the symlink target");
    std::os::unix::fs::symlink(&target, directory.path().join("docs/adr/0002-y.md")).expect("creating a symlink");

    let output = run(&directory, &["lint", "--fix"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(read(&first), misplaced(), "no file is written when any plan fails");
    assert_eq!(read(&target), misplaced());
}

#[test]
fn should_not_mark_a_readme_finding_fixable() {
    let (directory, _) = project(ADR);
    fs::write(directory.path().join("docs/adr/README.md"), "# Index\n\n- one\n").expect("writing a README");

    let output = run(&directory, &["lint", "--format", "json"]);
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("JSON findings");
    let readme = findings
        .iter()
        .filter(|finding| finding["path"].as_str().is_some_and(|path| path.ends_with("README.md")))
        .collect::<Vec<_>>();
    assert!(!readme.is_empty(), "{}", stdout(&output));
    assert!(
        readme.iter().all(|finding| finding["fixable"] == false),
        "{}",
        stdout(&output)
    );
}
