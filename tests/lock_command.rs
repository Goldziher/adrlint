use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn adrlint() -> Command {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("running adrlint")
}

fn manifest_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn scaffold() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("init"));
    assert!(output.status.success(), "{}", stderr(&output));
    let adr = directory.path().join("docs/adr/0001-record-architecture-decisions.md");
    (directory, adr)
}

#[test]
fn should_lock_accepted_adrs_and_detect_content_changes() {
    let (directory, adr) = scaffold();
    let key = manifest_key(directory.path(), &adr);

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));
    let manifest = fs::read_to_string(directory.path().join("adrlint.lock")).unwrap();
    assert!(manifest.contains(&key.to_string()));
    assert!(manifest.contains("status = \"accepted\""));

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(output.status.success(), "{}", stderr(&output));

    let output = run(adrlint().current_dir(directory.path()).args(["lock", "--check"]));
    assert!(output.status.success(), "{}", stderr(&output));

    let original = fs::read_to_string(&adr).unwrap();
    let reformatted = original.replace("* ", "- ");
    fs::write(&adr, &reformatted).unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    let linted = stdout(&output);
    assert!(
        linted.contains("[MADR31]"),
        "MADR31 must reject the changed markers: {linted}"
    );
    assert!(
        !linted.contains("[ADR50]"),
        "formatting only must not break the lock: {linted}"
    );

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(
        output.status.success(),
        "formatting back to '*' must leave the lock intact: {}",
        stdout(&output)
    );

    fs::write(&adr, original.replace("two or three", "three or four")).unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[ADR50]"), "{}", stdout(&output));

    let output = run(adrlint().current_dir(directory.path()).args(["lock", "--check"]));
    assert_eq!(output.status.code(), Some(1));

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["unlock", &key, "--reason", "fix a typo"]));
    assert!(output.status.success(), "{}", stderr(&output));
    let manifest = fs::read_to_string(directory.path().join("adrlint.lock")).unwrap();
    assert!(!manifest.contains(&key.to_string()));

    fs::write(&adr, &original).unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(output.status.success(), "{}", stderr(&output));

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn should_report_a_locked_adr_that_disappears() {
    let (directory, adr) = scaffold();
    let key = manifest_key(directory.path(), &adr);
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    fs::remove_file(&adr).unwrap();
    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("[ADR51]"), "{}", stdout(&output));

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["unlock", &key, "--reason", "record removed"]));
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn should_refuse_to_lock_a_directory_with_errors() {
    let directory = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr/invalid/MADR41a");
    let output = run(adrlint().current_dir(directory.path()).arg("lock").arg(fixtures));
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("refusing to lock"));
}

#[test]
fn should_reject_an_empty_unlock_reason() {
    let (directory, adr) = scaffold();
    let key = manifest_key(directory.path(), &adr);
    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["unlock", &key, "--reason", "  "]));
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("must not be empty"));
}
