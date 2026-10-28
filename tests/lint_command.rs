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
