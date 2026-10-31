use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn adrlint() -> Command {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr").join(name)
}

fn invalid(name: &str) -> PathBuf {
    fixture("invalid").join(name)
}

fn run(command: &mut Command) -> Output {
    command.output().expect("running adrlint")
}

fn codes(output: &Output) -> Vec<String> {
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("parsing JSON diagnostics");
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().expect("diagnostic code").to_owned())
        .collect()
}

#[test]
fn should_pass_a_valid_directory() {
    let output = run(adrlint().arg("lint").arg(fixture("valid")));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn should_report_each_directory_rule() {
    for (case, code) in [
        ("MADR41a", "MADR41a"),
        ("MADR41b", "MADR41b"),
        ("MADR42", "MADR42"),
        ("MADR43", "MADR43"),
        ("MADR44", "MADR44"),
    ] {
        let output = run(adrlint().arg("lint").arg("--format").arg("json").arg(invalid(case)));
        assert_eq!(
            output.status.code(),
            Some(1),
            "fixture {case} should fail, stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            codes(&output).iter().any(|found| found == code),
            "fixture {case} should report {code}, got {:?}",
            codes(&output)
        );
    }
}

#[test]
fn should_emit_github_annotations() {
    let output = run(adrlint()
        .arg("lint")
        .arg("--format")
        .arg("github")
        .arg(invalid("MADR42")));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("::error file="));
    assert!(stdout.contains("[MADR42]"));
}

#[test]
fn should_scaffold_and_extend_a_directory() {
    let directory = TempDir::new().unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("init"));
    assert!(output.status.success());
    assert!(directory.path().join(".adrlint.toml").is_file());
    assert!(
        directory
            .path()
            .join("docs/adr/0001-record-architecture-decisions.md")
            .is_file()
    );

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(output.status.success());

    let output = run(adrlint().current_dir(directory.path()).args(["new", "Use PostgreSQL"]));
    assert!(output.status.success());
    assert!(directory.path().join("docs/adr/0002-use-postgresql.md").is_file());

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(output.status.success());
}

#[test]
fn should_honour_an_explicit_configuration() {
    let directory = TempDir::new().unwrap();
    let adr = directory.path().join("decisions");
    std::fs::create_dir_all(&adr).unwrap();
    std::fs::write(
        directory.path().join("custom.toml"),
        "[adr]\ndir = \"decisions\"\nstart = 0\n",
    )
    .unwrap();
    std::fs::copy(
        fixture("valid/0001-record-architecture-decisions.md"),
        adr.join("0000-record-architecture-decisions.md"),
    )
    .unwrap();

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["--config", "custom.toml", "lint"]));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn should_allow_an_image_an_adr_references_but_report_an_unreferenced_one() {
    let directory = TempDir::new().expect("temporary directory");
    let adr = include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md").replace(
        "## Decision Drivers",
        "![Context](./context.png)\n\nSee [the diagram](diagrams/../flow.svg).\n\n## Decision Drivers",
    );
    let log = directory.path().join("docs/adr");
    std::fs::create_dir_all(&log).expect("creating the log");
    std::fs::write(log.join("0001-record-architecture-decisions.md"), adr).expect("writing the ADR");
    std::fs::create_dir_all(log.join("diagrams")).expect("creating a folder");
    for image in ["context.png", "flow.svg", "unused.png", "notes.txt"] {
        std::fs::write(log.join(image), b"asset").expect("writing an asset");
    }

    let output = run(adrlint().arg("lint").arg("--format").arg("json").arg(&log));
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("parsing JSON");
    let mut foreign = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "MADR42")
        .map(|diagnostic| {
            Path::new(diagnostic["path"].as_str().expect("a path"))
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    foreign.sort();
    assert_eq!(
        foreign,
        ["notes.txt", "unused.png"],
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn should_allow_an_image_only_the_log_index_references() {
    let directory = TempDir::new().expect("temporary directory");
    let log = directory.path().join("docs/adr");
    std::fs::create_dir_all(&log).expect("creating the log");
    std::fs::write(
        log.join("0001-record-architecture-decisions.md"),
        include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md"),
    )
    .expect("writing the ADR");
    std::fs::write(log.join("README.md"), "# Decisions\n\n![Overview](overview.png)\n").expect("writing the index");
    for image in ["overview.png", "unused.png"] {
        std::fs::write(log.join(image), b"asset").expect("writing an asset");
    }

    let output = run(adrlint().arg("lint").arg("--format").arg("json").arg(&log));
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("parsing JSON");
    let reported = diagnostics
        .iter()
        .map(|diagnostic| {
            let name = Path::new(diagnostic["path"].as_str().expect("a path"))
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .into_owned();
            (diagnostic["code"].as_str().expect("a code").to_owned(), name)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        reported,
        [("MADR42".to_owned(), "unused.png".to_owned())],
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn should_leave_an_upper_case_index_extension_out_of_the_log() {
    let directory = TempDir::new().expect("temporary directory");
    let log = directory.path().join("docs/adr");
    std::fs::create_dir_all(&log).expect("creating the log");
    std::fs::write(
        log.join("0001-record-architecture-decisions.md"),
        include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md"),
    )
    .expect("writing the ADR");
    std::fs::write(log.join("README.MD"), "# Decisions\n").expect("writing the index");

    let output = run(adrlint().arg("lint").arg("--format").arg("json").arg(&log));
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
}

fn write_log(directory: &TempDir, name: &str) -> PathBuf {
    let log = directory.path().join("adrs");
    std::fs::create_dir_all(&log).expect("creating the log");
    std::fs::write(
        log.join(name),
        include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md"),
    )
    .expect("writing the ADR");
    log
}

#[test]
fn should_accept_a_zero_based_log_discovered_without_configuration() {
    let directory = TempDir::new().expect("temporary directory");
    write_log(&directory, "0000-record-architecture-decisions.md");

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn should_accept_a_one_based_log_discovered_without_configuration() {
    let directory = TempDir::new().expect("temporary directory");
    write_log(&directory, "0001-record-architecture-decisions.md");

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn should_report_a_zero_based_log_against_an_explicit_start_of_one() {
    let directory = TempDir::new().expect("temporary directory");
    write_log(&directory, "0000-record-architecture-decisions.md");
    std::fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"adrs\"\nstart = 1\n",
    )
    .expect("writing the configuration");

    let output = run(adrlint()
        .current_dir(directory.path())
        .arg("lint")
        .arg("--format")
        .arg("json"));
    assert_eq!(output.status.code(), Some(1));
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).expect("parsing JSON");
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic["code"] == "MADR41a"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
