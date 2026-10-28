use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

fn adrlint() -> Command {
    Command::new(env!("CARGO_BIN_EXE_adrlint"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr").join(name)
}

/// The known-valid ADR, with three trailing spaces on a prose line and no final newline.
///
/// Three spaces are a real finding: exactly two are a hard line break, so the fixture cannot rely on
/// the exception. The missing final newline is the second, independent finding.
fn broken_adr() -> String {
    let source =
        std::fs::read_to_string(fixture("valid/0001-record-architecture-decisions.md")).expect("valid fixture");
    source
        .replacen("that were rejected.", "that were rejected.   ", 1)
        .trim_end_matches('\n')
        .to_owned()
}

fn setup(source: &str, config: Option<&str>) -> (TempDir, PathBuf) {
    let directory = TempDir::new().expect("temporary directory");
    let adr = directory.path().join("docs/adr");
    std::fs::create_dir_all(&adr).expect("creating the ADR directory");
    let path = adr.join("0001-record-architecture-decisions.md");
    std::fs::write(&path, source).expect("writing the ADR");
    if let Some(config) = config {
        std::fs::write(directory.path().join(".adrlint.toml"), config).expect("writing the configuration");
    }
    (directory, path)
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
fn should_report_markdown_findings_from_lint() {
    // rumdl reports MD009 and MD047 as warnings, so raising them to errors is what makes `lint`
    // fail; that is the documented per-rule severity knob.
    let (directory, _) = setup(&broken_adr(), None);
    std::fs::write(
        directory.path().join(".adrlint.toml"),
        "[markdown.rules]\nMD009 = \"error\"\nMD047 = \"error\"\n",
    )
    .expect("writing the configuration");

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = codes(&output);
    assert!(
        codes.iter().any(|code| code == "MD009"),
        "expected MD009, got {codes:?}"
    );
    assert!(
        codes.iter().any(|code| code == "MD047"),
        "expected MD047, got {codes:?}"
    );
}

#[test]
fn should_fail_lint_on_markdown_findings_by_default() {
    let (directory, _) = setup(&broken_adr(), None);

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));

    assert_eq!(
        output.status.code(),
        Some(1),
        "enabled markdown rules gate by default, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = codes(&output);
    assert!(
        codes.iter().any(|code| code == "MD009"),
        "expected MD009, got {codes:?}"
    );
    assert!(
        codes.iter().any(|code| code == "MD047"),
        "expected MD047, got {codes:?}"
    );
}

#[test]
fn should_check_then_format_and_stay_idempotent() {
    let (directory, path) = setup(&broken_adr(), None);
    let broken = std::fs::read(&path).expect("reading the ADR");

    let output = run(adrlint().current_dir(directory.path()).args(["format", "--check"]));
    assert_eq!(output.status.code(), Some(1), "--check should report a pending change");
    assert_eq!(
        std::fs::read(&path).expect("reading the ADR"),
        broken,
        "--check must not write"
    );

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let formatted = std::fs::read(&path).expect("reading the ADR");
    let text = String::from_utf8(formatted.clone()).expect("UTF-8 ADR");
    assert!(!text.contains("rejected.   "), "trailing spaces should be removed");
    assert!(text.ends_with('\n'), "the file should end with a newline");

    let output = run(adrlint().current_dir(directory.path()).args(["format", "--check"]));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(output.status.success());
    assert_eq!(
        std::fs::read(&path).expect("reading the ADR"),
        formatted,
        "a second format must be byte-identical"
    );
}

#[test]
fn should_suppress_every_markdown_finding_when_disabled() {
    let (directory, _) = setup(&broken_adr(), Some("[markdown]\nenabled = false\n"));

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(codes(&output), Vec::<String>::new());
}

#[test]
fn should_suppress_only_the_configured_rule() {
    let (directory, path) = setup(
        &broken_adr(),
        Some("[markdown.rules]\nMD009 = \"off\"\nMD047 = \"error\"\n"),
    );

    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["lint", "--format", "json"]));

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let codes = codes(&output);
    assert!(!codes.iter().any(|code| code == "MD009"), "MD009 is off, got {codes:?}");
    assert!(
        codes.iter().any(|code| code == "MD047"),
        "MD047 is still on, got {codes:?}"
    );
    assert!(
        !codes.iter().any(|code| code == "MD013"),
        "the default-off line length rule must stay off, got {codes:?}"
    );

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(output.status.success());
    let text = std::fs::read_to_string(&path).expect("reading the ADR");
    assert!(
        text.contains("rejected.   "),
        "format must not apply a rule that is off"
    );
    assert!(text.ends_with('\n'), "the remaining rule still applies");
}
