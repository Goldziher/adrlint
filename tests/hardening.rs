//! Synthetic monorepos: nested logs, ignored build output, hostile files and thousands of folders.
//!
//! Every tree is generated at runtime from the known-valid ADR, so the only reason a run fails is the
//! behaviour under test.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const ADR: &str = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md");

/// A project with no `dir` of its own, so a missing `docs/adr` falls back to discovery.
fn project() -> TempDir {
    let directory = TempDir::new().expect("temporary directory");
    fs::write(directory.path().join(".adrlint.toml"), "[adr]\npaths = []\n").expect("writing the configuration");
    directory
}

fn write(root: &Path, relative: &str, contents: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("creating the folder");
    fs::write(path, contents).expect("writing the file");
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

fn findings(output: &Output) -> Vec<serde_json::Value> {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "parsing JSON diagnostics: {error}\nstdout: {}\nstderr: {}",
            stdout(output),
            stderr(output)
        )
    })
}

fn lint_json(directory: &TempDir) -> Output {
    run(directory, &["lint", "--format", "json"])
}

fn path_of(finding: &serde_json::Value) -> String {
    finding["path"].as_str().unwrap_or_default().replace('\\', "/")
}

/// An ADR that every rule rejects, so a run that reads it cannot stay green.
const BAD: &str = "# Not a decision\n";

fn monorepo() -> TempDir {
    let directory = project();
    let root = directory.path();
    for relative in [
        "services/x/adrs/area/sub/0001-a.md",
        "services/y/adrs/0001-b.md",
        "adrs/team/0001-c.md",
    ] {
        write(root, relative, ADR);
    }
    // Neither of these is a log: `adrs/` only holds sub-logs, and dated documents are not decisions.
    write(root, "adrs/README.md", "- an index, not an ADR\n");
    write(root, "adrs/adr-template.md", "# {title}\n");
    write(root, "docs/specs/2026-07-01-x.md", "- a dated design document\n");
    // Each of these holds a broken ADR that a walk must never reach.
    write(root, "node_modules/pkg/adrs/0001-n.md", BAD);
    write(root, ".hidden/adrs/0001-h.md", BAD);
    write(root, "build/out/adrs/0001-o.md", BAD);
    write(root, ".gitignore", "build/\n");
    directory
}

#[test]
fn should_lint_a_monorepo_and_skip_everything_that_is_not_a_log() {
    let directory = monorepo();
    let output = lint_json(&directory);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(findings(&output).is_empty(), "{}", stdout(&output));
}

#[test]
fn should_reach_a_log_deep_inside_a_service_directory() {
    let directory = monorepo();
    write(
        directory.path(),
        "services/x/adrs/area/sub/0001-a.md",
        ADR.replace("### Consequences", "## Consequences"),
    );

    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let found = findings(&output);
    assert!(!found.is_empty());
    assert!(
        found
            .iter()
            .all(|finding| path_of(finding).ends_with("services/x/adrs/area/sub/0001-a.md")),
        "only the broken deep ADR is reported: {}",
        stdout(&output)
    );
}

#[test]
fn should_summarize_the_files_it_checked() {
    let directory = monorepo();
    let output = run(&directory, &["lint"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("no problems found in 3 files checked"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn should_lock_every_nested_log_with_project_relative_keys() {
    let directory = monorepo();
    let output = run(&directory, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("locked 3 ADR(s)"), "{}", stdout(&output));

    let manifest = fs::read_to_string(directory.path().join("adrlint.lock")).expect("reading the manifest");
    for key in [
        "services/x/adrs/area/sub/0001-a.md",
        "services/y/adrs/0001-b.md",
        "adrs/team/0001-c.md",
    ] {
        assert!(manifest.contains(key), "{key} is a manifest key:\n{manifest}");
    }
    assert!(!manifest.contains('\\'), "keys use forward slashes:\n{manifest}");
    assert!(
        !manifest.contains("node_modules"),
        "ignored logs are not locked:\n{manifest}"
    );

    let output = run(&directory, &["lock", "--check"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
}

#[test]
fn should_flag_only_the_nested_log_whose_locked_content_changed() {
    let directory = monorepo();
    let output = run(&directory, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    write(
        directory.path(),
        "services/y/adrs/0001-b.md",
        ADR.replace("because they are reviewable", "because they can be reviewed"),
    );
    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let changed = findings(&output)
        .into_iter()
        .filter(|finding| finding["code"] == "ADR50")
        .map(|finding| path_of(&finding))
        .collect::<Vec<_>>();
    assert_eq!(changed.len(), 1, "{}", stdout(&output));
    assert!(changed[0].ends_with("services/y/adrs/0001-b.md"), "{changed:?}");

    let output = run(&directory, &["lock", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stdout(&output).contains("[ADR50]"), "{}", stdout(&output));
}

#[test]
fn should_report_a_locked_nested_adr_that_disappears() {
    let directory = monorepo();
    let output = run(&directory, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    fs::remove_dir_all(directory.path().join("services/y")).expect("removing a service");
    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        findings(&output)
            .iter()
            .any(|finding| finding["code"] == "ADR51" && path_of(finding).ends_with("services/y/adrs/0001-b.md")),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_handle_thousands_of_directories_quickly() {
    let directory = project();
    let root = directory.path();
    for index in 0..2000 {
        write(root, &format!("tree/a{index}/b/README.md"), "- notes\n");
    }
    for index in 0..150 {
        write(root, &format!("logs/g{index}/0001-x.md"), ADR);
    }

    let started = std::time::Instant::now();
    let output = lint_json(&directory);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(findings(&output).is_empty());
    assert!(
        started.elapsed() < std::time::Duration::from_secs(120),
        "a large tree must not take minutes: {:?}",
        started.elapsed()
    );

    let output = run(&directory, &["lint"]);
    assert!(
        stderr(&output).contains("no problems found in 150 files checked"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn should_produce_byte_identical_output_for_many_logs() {
    let directory = project();
    for index in 0..40 {
        let body = if index % 3 == 0 {
            ADR.replace("### Confirmation", "## Confirmation")
        } else {
            ADR.to_owned()
        };
        write(directory.path(), &format!("logs/g{index}/0001-x.md"), body);
    }
    let first = lint_json(&directory);
    assert_eq!(first.status.code(), Some(1), "{}", stderr(&first));
    for _ in 0..4 {
        let again = lint_json(&directory);
        assert_eq!(first.stdout, again.stdout, "parallel output must be deterministic");
    }
}

#[test]
fn should_count_overlapping_roots_once() {
    let directory = project();
    for relative in ["adrs/a/0001-a.md", "adrs/a/0002-b.md", "adrs/b/0001-c.md"] {
        write(directory.path(), relative, ADR);
    }
    let output = run(
        &directory,
        &["lint", "--recursive", "adrs", "adrs/a", "adrs", "adrs/b/"],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("no problems found in 3 files checked"),
        "overlapping roots must not repeat files: {}",
        stderr(&output)
    );
}

#[test]
fn should_report_non_utf8_files_without_aborting() {
    let directory = monorepo();
    write(directory.path(), "adrs/team/0002-bytes.md", [0xff_u8, 0xfe, 0x00, 0x41]);

    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let found = findings(&output);
    assert!(
        found
            .iter()
            .any(|finding| finding["code"] == "ADR52" && path_of(finding).ends_with("adrs/team/0002-bytes.md")),
        "{}",
        stdout(&output)
    );

    let output = run(&directory, &["format", "--check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("0002-bytes.md"), "{}", stderr(&output));
}

#[test]
fn should_reject_an_oversized_adr_without_reading_it() {
    let directory = monorepo();
    let path = directory.path().join("adrs/team/0002-huge.md");
    let file = fs::File::create(&path).expect("creating the file");
    file.set_len(17 * 1024 * 1024).expect("making the file sparse");

    let output = lint_json(&directory);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(
        findings(&output).iter().any(|finding| {
            finding["code"] == "ADR52" && finding["message"].as_str().is_some_and(|text| text.contains("limit"))
        }),
        "{}",
        stdout(&output)
    );
}

#[test]
fn should_lint_a_large_adr_under_the_limit() {
    let directory = project();
    let big = format!("{ADR}{}", "* Good, because it is long\n".repeat(20_000));
    write(directory.path(), "adrs/big/0001-big.md", big);
    let output = lint_json(&directory);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );
}

#[test]
fn should_lint_windows_line_endings_and_keep_the_lock_stable() {
    let directory = project();
    write(directory.path(), "adrs/win/0001-a.md", ADR.replace('\n', "\r\n"));
    let output = lint_json(&directory);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}\n{}",
        stdout(&output),
        stderr(&output)
    );

    let output = run(&directory, &["lock"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    write(directory.path(), "adrs/win/0001-a.md", ADR);
    let output = lint_json(&directory);
    assert_eq!(
        output.status.code(),
        Some(0),
        "a line-ending change is not a content change: {}",
        stdout(&output)
    );
}

#[test]
fn should_survive_front_matter_edge_cases() {
    let body = &ADR[ADR.find("# Record").expect("the title")..];
    let cases = [
        format!("---\n---\n{body}"),
        format!("---\nstatus: accepted\n{body}"),
        format!("---\n: : :\n\t- [\n---\n{body}"),
        format!("---\r\nstatus: accepted\r\n---\r\n{body}"),
        format!("\u{feff}---\nstatus: accepted\n---\n{body}"),
        "---\n---\n".to_owned(),
        "---".to_owned(),
        String::new(),
    ];
    for (index, case) in cases.iter().enumerate() {
        let directory = project();
        write(directory.path(), "adrs/fm/0001-case.md", case);
        let output = lint_json(&directory);
        assert!(
            matches!(output.status.code(), Some(0 | 1)),
            "case {index} aborted the run: {}",
            stderr(&output)
        );
        let _ = findings(&output);
    }
}

#[cfg(unix)]
mod unix {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use super::*;

    #[test]
    fn should_not_follow_symlink_loops() {
        let directory = monorepo();
        symlink(".", directory.path().join("loop")).expect("linking the root");
        symlink("..", directory.path().join("adrs/team/up")).expect("linking the parent");

        let output = lint_json(&directory);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}\n{}",
            stdout(&output),
            stderr(&output)
        );
        assert!(findings(&output).is_empty(), "{}", stdout(&output));
    }

    #[test]
    fn should_report_an_unreadable_file_and_keep_going() {
        let directory = monorepo();
        let path = directory.path().join("adrs/team/0002-locked.md");
        fs::write(&path, ADR).expect("writing the ADR");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("hiding the file");
        if fs::read(&path).is_ok() {
            // Running as a user that ignores permissions: nothing to observe.
            return;
        }

        let output = lint_json(&directory);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("restoring the file");
        assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
        assert!(
            findings(&output)
                .iter()
                .any(|finding| finding["code"] == "ADR52" && path_of(finding).ends_with("0002-locked.md")),
            "{}",
            stdout(&output)
        );
    }

    #[test]
    fn should_skip_an_unreadable_directory_and_lint_the_rest() {
        let directory = monorepo();
        let hidden = directory.path().join("services/z/adrs");
        write(directory.path(), "services/z/adrs/0001-z.md", ADR);
        fs::set_permissions(&hidden, fs::Permissions::from_mode(0o000)).expect("hiding the folder");
        if fs::read_dir(&hidden).is_ok() {
            fs::set_permissions(&hidden, fs::Permissions::from_mode(0o755)).expect("restoring the folder");
            return;
        }

        let output = lint_json(&directory);
        fs::set_permissions(&hidden, fs::Permissions::from_mode(0o755)).expect("restoring the folder");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}\n{}",
            stdout(&output),
            stderr(&output)
        );
        assert!(stderr(&output).contains("skipping"), "{}", stderr(&output));
    }
}
