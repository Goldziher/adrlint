//! ADR logs nested under namespace folders, addressed by file, directory, glob or `--recursive`.
//!
//! Every tree numbers each directory from `0001` with no gaps, so a namespace only fails when the
//! rule under test is the one that broke it, never because two namespaces share a number.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// The known-valid MADR ADR, reused for every file in a tree.
const ADR: &str = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md");

/// A project whose configuration pins `dir` and clears `paths`, so the ambient global configuration
/// cannot change what the test resolves.
fn project() -> TempDir {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = []\n",
    )
    .expect("writing the configuration");
    directory
}

fn write_adr(root: &Path, relative: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("creating the namespace");
    fs::write(path, ADR).expect("writing the ADR");
}

/// The four namespaces from the task, each valid and independently numbered.
fn tree() -> TempDir {
    let directory = project();
    for relative in [
        "adrs/team-a/0001-a.md",
        "adrs/team-a/0002-b.md",
        "adrs/team-b/0001-c.md",
        "product/adrs/api/0001-d.md",
        "product/adrs/web/0001-e.md",
    ] {
        write_adr(directory.path(), relative);
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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn diagnostics(output: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parsing JSON diagnostics: {error}\nstdout: {}\nstderr: {}",
            stdout(output),
            stderr(output)
        )
    })
}

fn lint_json(directory: &TempDir, args: &[&str]) -> Output {
    let mut command = vec!["lint", "--format", "json"];
    command.extend_from_slice(args);
    run(directory, &command)
}

#[test]
fn should_lint_every_namespace_when_recursive() {
    let directory = tree();
    let output = lint_json(&directory, &["--recursive", "adrs", "product"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        diagnostics(&output).is_empty(),
        "every namespace is valid on its own: {}",
        stdout(&output)
    );
}

#[test]
fn should_not_descend_without_recursive() {
    let directory = project();
    // Two files sharing a number would fail MADR43 if the nested directory were scanned.
    write_adr(directory.path(), "adrs/team-a/0001-a.md");
    write_adr(directory.path(), "adrs/team-a/0001-b.md");

    let output = lint_json(&directory, &["adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        diagnostics(&output).is_empty(),
        "a non-recursive run lints only the named directory: {}",
        stdout(&output)
    );
}

#[test]
fn should_report_a_gap_in_one_namespace_only() {
    let directory = tree();
    fs::rename(
        directory.path().join("adrs/team-a/0002-b.md"),
        directory.path().join("adrs/team-a/0003-c.md"),
    )
    .expect("creating the gap");

    let output = lint_json(&directory, &["--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));

    let found = diagnostics(&output);
    let gaps: Vec<&serde_json::Value> = found
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "MADR41b")
        .collect();
    assert_eq!(gaps.len(), 1, "one gap, reported once: {}", stdout(&output));
    let path = gaps[0]["path"].as_str().expect("a diagnostic path");
    assert!(path.contains("team-a"), "the gap belongs to team-a: {path}");
    assert!(
        gaps[0]["message"].as_str().unwrap_or_default().contains("0002"),
        "the missing number is named: {}",
        gaps[0]["message"]
    );
    assert!(
        found
            .iter()
            .all(|diagnostic| !diagnostic["path"].as_str().unwrap_or_default().contains("team-b")),
        "team-b stays clean: {}",
        stdout(&output)
    );
}

#[test]
fn should_expand_a_glob_across_namespaces() {
    let directory = tree();
    let output = lint_json(&directory, &["adrs/*"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));

    // Prove the second namespace was reached, not merely that `adrs/*` resolved to nothing.
    write_adr(directory.path(), "adrs/team-b/0003-c.md");
    let output = lint_json(&directory, &["adrs/*"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let found = diagnostics(&output);
    assert!(
        found.iter().any(|diagnostic| {
            diagnostic["code"] == "MADR41b" && diagnostic["path"].as_str().unwrap_or_default().contains("team-b")
        }),
        "the glob expanded team-b: {}",
        stdout(&output)
    );
}

#[test]
fn should_use_the_configured_paths() {
    let directory = tree();
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = [\"adrs/*\", \"product/adrs/*\"]\n",
    )
    .expect("writing the configuration");

    // `docs/adr` does not exist, so a run that fell back to `dir` would fail instead of linting.
    let output = lint_json(&directory, &["--recursive"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_check_the_parent_directory_of_a_file() {
    let directory = tree();
    fs::rename(
        directory.path().join("adrs/team-a/0002-b.md"),
        directory.path().join("adrs/team-a/0001-b.md"),
    )
    .expect("creating the duplicate");

    let output = lint_json(&directory, &["adrs/team-a/0001-b.md"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));

    let found = diagnostics(&output);
    let duplicates: Vec<&serde_json::Value> = found
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "MADR43")
        .collect();
    assert_eq!(
        duplicates.len(),
        2,
        "a duplicate is reported once per file: {}",
        stdout(&output)
    );
    let paths: Vec<&str> = duplicates
        .iter()
        .filter_map(|diagnostic| diagnostic["path"].as_str())
        .collect();
    assert!(
        paths.iter().any(|path| path.ends_with("0001-a.md")),
        "the other duplicate is reported too: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.ends_with("0001-b.md")),
        "the named file is reported: {paths:?}"
    );
}

#[test]
fn should_format_recursively_and_idempotently() {
    let directory = tree();
    let target = directory.path().join("adrs/team-a/0001-a.md");
    let source = fs::read_to_string(&target).expect("reading the ADR");
    let spaced = source.replacen("that were rejected.", "that were rejected.   ", 1);
    assert_ne!(spaced, source, "the fixture must contain the paragraph");
    fs::write(&target, spaced).expect("adding trailing spaces");

    let output = run(&directory, &["format", "--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("reformatted"),
        "the nested file is formatted: {}",
        stdout(&output)
    );
    let formatted = fs::read_to_string(&target).expect("reading the formatted ADR");
    assert!(
        !formatted.contains("rejected.   "),
        "MD009 must trim the trailing spaces: {}",
        stdout(&output)
    );

    let output = run(&directory, &["format", "--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).is_empty(),
        "a second run changes nothing: {}",
        stdout(&output)
    );
    assert_eq!(fs::read_to_string(&target).unwrap(), formatted, "format is idempotent");
}

#[test]
fn should_lock_every_namespace_recursively() {
    let directory = tree();
    let output = run(&directory, &["lock", "--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("locked 3 ADR(s)"),
        "both namespaces are locked: {}",
        stdout(&output)
    );

    let manifest = fs::read_to_string(directory.path().join("adrlint.lock")).expect("reading the manifest");
    for key in [
        "adrs/team-a/0001-a.md",
        "adrs/team-a/0002-b.md",
        "adrs/team-b/0001-c.md",
    ] {
        assert!(
            manifest.contains(key),
            "the manifest key is relative to the project root: {key} missing from\n{manifest}"
        );
    }
    assert!(
        !manifest.contains("product/"),
        "a namespace outside the named roots is not locked:\n{manifest}"
    );
}

#[test]
fn should_produce_byte_identical_json_across_runs() {
    let directory = tree();
    fs::rename(
        directory.path().join("adrs/team-a/0002-b.md"),
        directory.path().join("adrs/team-a/0003-c.md"),
    )
    .expect("creating the gap");

    let first = lint_json(&directory, &["--recursive", "adrs", "product"]);
    let second = lint_json(&directory, &["--recursive", "adrs", "product"]);
    assert!(!diagnostics(&first).is_empty(), "the tree has a finding to order");
    assert_eq!(first.stdout, second.stdout, "diagnostic order is deterministic");
}

#[test]
fn should_reject_an_invalid_glob() {
    let directory = tree();
    let output = run(&directory, &["lint", "["]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("invalid glob pattern"),
        "the failure names the problem: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains('['),
        "the failure names the pattern: {}",
        stderr(&output)
    );
}

#[test]
fn should_reject_an_absolute_configured_path() {
    let directory = tree();
    let absolute = directory.path().join("adrs");
    let rendered = format!(
        "[adr]\npaths = [{}]\n",
        toml::Value::String(absolute.to_string_lossy().into_owned())
    );
    fs::write(directory.path().join(".adrlint.toml"), rendered).expect("writing the configuration");

    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("adr.paths must be relative"),
        "the configuration is rejected at load: {}",
        stderr(&output)
    );
}

#[test]
fn should_reject_a_glob_that_matches_nothing() {
    let directory = tree();
    let output = run(&directory, &["lint", "adrs/none/*"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("nothing matched adrs/none/*"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn should_scan_an_explicit_empty_directory_without_findings() {
    let directory = tree();
    fs::create_dir_all(directory.path().join("adrs/empty")).expect("creating the directory");
    let output = lint_json(&directory, &["--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        diagnostics(&output).is_empty(),
        "an empty directory legitimately has no findings: {}",
        stdout(&output)
    );
}

#[test]
fn should_keep_namespaces_behind_skipped_directories_out_of_a_recursive_run() {
    let directory = tree();
    // A duplicate would fail MADR43 if these directories were descended into.
    write_adr(directory.path(), "adrs/target/0001-generated.md");
    write_adr(directory.path(), "adrs/target/0001-duplicate.md");
    write_adr(directory.path(), "adrs/node_modules/0001-vendored.md");
    write_adr(directory.path(), "adrs/node_modules/0001-duplicate.md");

    let output = lint_json(&directory, &["--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let found = diagnostics(&output);
    assert!(
        found.iter().all(|diagnostic| {
            let path = diagnostic["path"].as_str().unwrap_or_default();
            !path.contains("target/") && !path.contains("node_modules/")
        }),
        "build and dependency directories are never descended into: {}",
        stdout(&output)
    );
}

#[test]
fn should_not_treat_a_recursive_root_as_an_adr_directory() {
    let directory = tree();
    fs::write(directory.path().join("README.md"), "# Project\n").expect("writing a README");
    fs::write(directory.path().join(".adrlint.toml"), "[adr]\ndir = \"docs/adr\"\n").expect("writing config");

    let output = lint_json(&directory, &["--recursive", "."]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "the repository root is a search base, not an ADR log: {}",
        stdout(&output)
    );
    assert!(
        diagnostics(&output).is_empty(),
        "a root README or config must not be reported as a foreign ADR file: {}",
        stdout(&output)
    );
}

#[test]
fn should_ignore_a_markdown_only_directory_in_a_recursive_walk() {
    let directory = tree();
    write_adr(directory.path(), "docs/notes/README.md");

    let output = lint_json(&directory, &["--recursive", "."]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_report_a_foreign_file_in_a_discovered_adr_directory() {
    let directory = tree();
    fs::write(directory.path().join("adrs/team-a/README.md"), "# Team notes\n").expect("writing a README");

    let output = lint_json(&directory, &["--recursive", "adrs"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        diagnostics(&output).iter().any(|diagnostic| {
            diagnostic["code"] == "MADR44"
                && diagnostic["path"]
                    .as_str()
                    .is_some_and(|path| path.replace('\\', "/").ends_with("adrs/team-a/README.md"))
        }),
        "a directory that holds ADRs still rejects a Markdown file that is not an ADR: {}",
        stdout(&output)
    );
}

/// A project with no `dir` of its own, so a missing `docs/adr` falls back to discovery.
fn unconfigured_tree() -> TempDir {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(directory.path().join(".adrlint.toml"), "[adr]\npaths = []\n").expect("writing the configuration");
    for relative in ["adrs/team-a/0001-a.md", "service/adrs/area/sub/0001-b.md"] {
        write_adr(directory.path(), relative);
    }
    directory
}

fn linted_paths(output: &Output) -> Vec<String> {
    let mut paths: Vec<String> = diagnostics(output)
        .iter()
        .filter_map(|diagnostic| diagnostic["path"].as_str().map(|path| path.replace('\\', "/")))
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn should_discover_nested_logs_without_configuration() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join("service/adrs/area/sub/0003-gap.md"), ADR).expect("writing an ADR after a gap");

    let output = lint_json(&directory, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        diagnostics(&output)
            .iter()
            .any(|diagnostic| diagnostic["code"] == "MADR41b"),
        "the deeply nested log is discovered and checked: {}",
        stdout(&output)
    );
}

#[test]
fn should_still_require_a_configured_dir() {
    let directory = project();
    write_adr(directory.path(), "adrs/team-a/0001-a.md");

    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(stderr(&output).contains("does not exist"), "{}", stderr(&output));
}

#[test]
fn should_report_when_discovery_finds_no_log() {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(directory.path().join(".adrlint.toml"), "[adr]\npaths = []\n").expect("writing the configuration");

    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(stderr(&output).contains("no ADR log found"), "{}", stderr(&output));
}

#[test]
fn should_not_treat_dated_documents_as_an_adr_log() {
    let directory = unconfigured_tree();
    write_adr(directory.path(), "docs/specs/2026-07-01-integration-design.md");
    fs::write(
        directory.path().join("docs/specs/2026-07-02-other.md"),
        "- not an ADR\n",
    )
    .expect("writing a dated doc");

    let output = lint_json(&directory, &["--recursive", "."]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_skip_markdown_a_hook_passes_from_outside_any_log() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join("adrs/README.md"), "- an index, not an ADR\n").expect("writing the index");
    fs::write(directory.path().join("adrs/adr-template.md"), "# {title}\n").expect("writing the template");

    let output = lint_json(&directory, &["adrs/README.md", "adrs/adr-template.md"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_lint_the_log_of_a_numbered_file_a_hook_passes() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join("adrs/README.md"), "- an index, not an ADR\n").expect("writing the index");
    fs::write(directory.path().join("adrs/team-a/0003-gap.md"), ADR).expect("writing an ADR after a gap");

    let output = lint_json(&directory, &["adrs/README.md", "adrs/team-a/0003-gap.md"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        linted_paths(&output).iter().all(|path| path.contains("adrs/team-a/")),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_search_configured_paths_recursively_when_configured() {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\npaths = [\"adrs\", \"service/adrs\"]\nrecursive = true\n",
    )
    .expect("writing the configuration");
    for relative in [
        "adrs/team-a/0001-a.md",
        "service/adrs/area/0001-b.md",
        "service/adrs/area/0003-c.md",
    ] {
        write_adr(directory.path(), relative);
    }

    let output = lint_json(&directory, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        linted_paths(&output)
            .iter()
            .any(|path| path.contains("service/adrs/area/")),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_walk_a_nested_root_that_an_outer_root_ignores() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join(".gitignore"), "build/\n").expect("writing .gitignore");
    write_adr(directory.path(), "build/gen/adrs/0001-a.md");
    write_adr(directory.path(), "build/gen/adrs/0003-c.md");

    let outer_only = lint_json(&directory, &["--recursive", "."]);
    assert!(
        !linted_paths(&outer_only).iter().any(|path| path.contains("build/")),
        "the outer walk itself honours .gitignore: {}",
        stdout(&outer_only)
    );
    let output = lint_json(&directory, &["--recursive", ".", "build/gen/adrs"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        linted_paths(&output)
            .iter()
            .any(|path| path.contains("build/gen/adrs/")),
        "the explicitly named root under an ignored directory is still linted: {}",
        stdout(&output)
    );
}

#[test]
fn should_honour_gitignore_but_not_other_ignore_files() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join(".ignore"), "adrs/\n").expect("writing .ignore");
    fs::write(directory.path().join(".gitignore"), "generated/\n").expect("writing .gitignore");
    write_adr(directory.path(), "generated/adrs/0002-gap.md");
    fs::write(directory.path().join("adrs/team-a/0003-gap.md"), ADR).expect("writing an ADR after a gap");

    let output = lint_json(&directory, &[]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let linted = linted_paths(&output);
    assert!(
        linted.iter().any(|path| path.contains("adrs/team-a/")),
        "{}",
        stdout(&output)
    );
    assert!(
        !linted.iter().any(|path| path.contains("generated/")),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_lint_the_remaining_log_of_a_deleted_file_a_hook_passes() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join("adrs/team-a/0003-gap.md"), ADR).expect("writing an ADR after a gap");

    let output = lint_json(&directory, &["adrs/team-a/0002-deleted.md", "adrs/gone/0001-x.md"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        linted_paths(&output).iter().all(|path| path.contains("adrs/team-a/")),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_report_a_recursive_root_without_any_log() {
    let directory = unconfigured_tree();
    fs::create_dir_all(directory.path().join("notes")).expect("creating a directory");
    fs::write(directory.path().join("notes/adr-1.md"), "# not numbered\n").expect("writing a note");

    let output = run(&directory, &["lint", "--recursive", "notes"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(stderr(&output).contains("no ADR log found"), "{}", stderr(&output));
}

#[test]
fn should_never_rewrite_a_non_adr_file_inside_a_log() {
    let directory = unconfigured_tree();
    let readme = directory.path().join("adrs/team-a/README.md");
    fs::write(&readme, "- an index  \n-   with sloppy markdown\n").expect("writing a README");

    let output = run(&directory, &["lint", "--fix"]);
    assert_ne!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(&readme).expect("reading the README"),
        "- an index  \n-   with sloppy markdown\n"
    );
}

#[test]
fn should_reject_a_mistyped_path() {
    let directory = unconfigured_tree();

    let output = run(&directory, &["lint", "adrz"]);
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(stderr(&output).contains("adrz"), "{}", stderr(&output));
}

#[test]
fn should_pass_a_recursive_hook_run_that_touched_only_an_index() {
    let directory = unconfigured_tree();
    fs::write(directory.path().join("adrs/README.md"), "- an index\n").expect("writing the index");

    let output = lint_json(&directory, &["--recursive", "adrs/README.md"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(diagnostics(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_read_an_existing_path_with_brackets_literally() {
    let directory = unconfigured_tree();
    write_adr(directory.path(), "adrs/[draft]/0001-a.md");

    let output = lint_json(&directory, &["adrs/[draft]/0001-a.md"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

#[test]
fn should_report_a_configured_recursive_root_without_any_log() {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = []\nrecursive = true\n",
    )
    .expect("writing the configuration");
    fs::create_dir_all(directory.path().join("docs/adr")).expect("creating the ADR folder");
    fs::write(directory.path().join("docs/adr/adr-1.md"), "# not numbered\n").expect("writing a note");

    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("no ADR log found"), "{}", stderr(&output));
}

#[test]
fn should_expand_a_configured_glob_under_a_root_with_brackets() {
    let outer = TempDir::new().expect("temporary directory");
    let root = outer.path().join("[client] repo");
    fs::create_dir_all(&root).expect("creating the project");
    fs::write(root.join(".adrlint.toml"), "[adr]\npaths = [\"adrs/*\"]\n").expect("writing the configuration");
    write_adr(&root, "adrs/team-a/0001-a.md");
    write_adr(&root, "adrs/team-b/0001-b.md");

    let output = Command::new(env!("CARGO_BIN_EXE_adrlint"))
        .current_dir(&root)
        .args(["--no-color", "lint"])
        .output()
        .expect("running adrlint");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stderr(&output).contains("2 files checked"), "{}", stderr(&output));
}
