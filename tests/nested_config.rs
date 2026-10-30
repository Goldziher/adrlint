//! A path inside another project's log is checked with that project's `.adrlint.toml`, so a parent
//! directory (a polyrepo or monorepo root) can lint every nested repository's log.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const ADR: &str = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md");

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("creating the directory");
    fs::write(path, contents).expect("writing the file");
}

/// A parent directory without configuration holding `repo-a`, whose log starts at `0000`.
fn polyrepo() -> TempDir {
    let directory = TempDir::new().expect("temporary directory");
    let root = directory.path();
    write(
        root,
        "repo-a/.adrlint.toml",
        "[adr]\ndir = \"adrs\"\npaths = []\nstart = 0\n",
    );
    write(root, "repo-a/adrs/0000-a.md", ADR);
    directory
}

fn run(directory: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
        .current_dir(directory.path())
        .args(args)
        .output()
        .expect("running adrlint")
}

fn report(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn should_lint_a_nested_log_with_its_own_configuration() {
    let directory = polyrepo();
    for args in [
        &["lint", "repo-a/adrs"][..],
        &["lint", "repo-a/adrs/0000-a.md"][..],
        &["lint", "--recursive", "repo-a"][..],
        &["format", "--check", "repo-a/adrs"][..],
    ] {
        let output = run(&directory, args);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {}", report(&output));
    }
}

#[test]
fn should_lock_a_nested_log_into_its_own_manifest() {
    let directory = polyrepo();

    let output = run(&directory, &["lock", "repo-a/adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", report(&output));
    let manifest = fs::read_to_string(directory.path().join("repo-a/adrlint.lock")).expect("repo-a's manifest");
    assert!(manifest.contains("[adrs.\"adrs/0000-a.md\"]"), "{manifest}");
    assert!(!directory.path().join("adrlint.lock").exists());

    let output = run(&directory, &["lock", "--check", "repo-a/adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", report(&output));
}

#[test]
fn should_refuse_paths_from_two_projects_in_one_run() {
    let directory = polyrepo();
    write(
        directory.path(),
        "repo-b/.adrlint.toml",
        "[adr]\ndir = \"adrs\"\npaths = []\n",
    );
    write(directory.path(), "repo-b/adrs/0001-b.md", ADR);

    let output = run(&directory, &["lint", "repo-a/adrs", "repo-b/adrs"]);
    assert_eq!(output.status.code(), Some(2), "{}", report(&output));
    assert!(
        report(&output).contains("different adrlint projects"),
        "{}",
        report(&output)
    );
}
