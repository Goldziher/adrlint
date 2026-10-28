use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn adrlint() -> Command {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
}

fn run(command: &mut Command) -> Output {
    command.output().expect("running adrlint")
}

fn setup(config: &str) -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let adr = directory.path().join("docs/adr");
    std::fs::create_dir_all(&adr).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr/invalid/MADR05/0001-second-title.md"),
        adr.join("0001-second-title.md"),
    )
    .unwrap();
    std::fs::write(directory.path().join(".adrlint.toml"), config).unwrap();
    (directory, adr)
}

#[test]
fn should_turn_a_rule_off() {
    let (directory, _) =
        setup("[adr]\ndir = \"docs/adr\"\n\n[markdown]\nenabled = false\n\n[rules]\nMADR05 = \"off\"\n");
    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"[]\n");
}

#[test]
fn should_downgrade_a_rule_to_a_warning() {
    let (directory, _) =
        setup("[adr]\ndir = \"docs/adr\"\n\n[markdown]\nenabled = false\n\n[rules]\nMADR05 = \"warn\"\n");
    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));
    assert!(output.status.success());
    let diagnostics: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic["code"] == "MADR05" && diagnostic["severity"] == "warning")
    );
}

#[test]
fn should_keep_a_rule_as_an_error_by_default() {
    let (directory, _) = setup("[adr]\ndir = \"docs/adr\"\n\n[markdown]\nenabled = false\n");
    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));
    assert_eq!(output.status.code(), Some(1));
}
