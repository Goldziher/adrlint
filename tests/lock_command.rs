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

/// The scaffold's ADR with its `accepted` status replaced by `status`.
fn set_status(adr: &Path, original: &str, status: &str) {
    fs::write(
        adr,
        original.replacen("status: accepted", &format!("status: {status}"), 1),
    )
    .unwrap();
}

#[test]
fn should_lock_a_superseded_by_status_as_superseded() {
    let (directory, adr) = scaffold();
    let original = fs::read_to_string(&adr).unwrap();
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[adr]\ndir = \"docs/adr\"\npaths = []\n[lock]\nstatuses = [\"accepted\", \"superseded\"]\n",
    )
    .unwrap();
    let manifest_path = directory.path().join("adrlint.lock");
    for status in ["superseded by 0002", "Superseded by ADR-0002", "superseded"] {
        set_status(&adr, &original, status);
        let output = run(adrlint().current_dir(directory.path()).arg("lock"));
        assert!(output.status.success(), "{status}: {}", stderr(&output));
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            manifest.contains(&format!("status = \"{status}\"")),
            "{status} must be locked under `superseded`: {manifest}"
        );
        fs::remove_file(&manifest_path).unwrap();
    }
}

#[test]
fn should_not_lock_a_superseded_by_status_unless_superseded_is_locked() {
    let (directory, adr) = scaffold();
    let original = fs::read_to_string(&adr).unwrap();
    set_status(&adr, &original, "superseded by 0002");

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert_eq!(output.status.code(), Some(2), "{}", stdout(&output));
    assert!(
        stderr(&output).contains("no ADRs matched lock.statuses"),
        "{}",
        stderr(&output)
    );
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

#[test]
fn should_exit_one_and_report_a_stale_lock_once() {
    let (directory, _) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    let manifest_path = directory.path().join("adrlint.lock");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(&manifest_path, manifest.replace("canonical = 1", "canonical = 999")).unwrap();

    let output = run(adrlint().current_dir(directory.path()).args(["lock", "--check"]));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert_eq!(
        stderr(&output).matches("canonical version").count(),
        1,
        "the stale message is reported once: {}",
        stderr(&output)
    );

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert_eq!(
        output.status.code(),
        Some(2),
        "without --check a stale lock still asks for --all: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("--all"), "{}", stderr(&output));
}

#[test]
fn should_reject_a_manifest_key_outside_the_project() {
    let (directory, _) = scaffold();
    fs::write(
        directory.path().join("adrlint.lock"),
        "version = 1\ncanonical = 1\n\n[adrs.\"/etc/passwd\"]\nnumber = 1\nstatus = \"accepted\"\nblake3 = \"x\"\n",
    )
    .unwrap();

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("/etc/passwd"), "{}", stderr(&output));
}

#[cfg(unix)]
#[test]
fn should_refuse_to_lock_a_log_reached_through_a_symlink_outside_the_root() {
    let outside = TempDir::new().expect("temporary directory");
    fs::write(
        outside.path().join("0001-record-architecture-decisions.md"),
        include_str!("../fixtures/adr/valid/0001-record-architecture-decisions.md"),
    )
    .expect("writing the ADR");
    let directory = TempDir::new().expect("temporary directory");
    fs::write(directory.path().join(".adrlint.toml"), "[adr]\npaths = []\n").expect("writing the configuration");
    std::os::unix::fs::symlink(outside.path(), directory.path().join("adrs")).expect("creating a symlink");

    let output = Command::new(env!("CARGO_BIN_EXE_adrlint"))
        .current_dir(directory.path())
        .args(["lock", "adrs"])
        .output()
        .expect("running adrlint");
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("outside the project root"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.path().join("adrlint.lock").exists());
}

/// Runs adrlint under a pseudo-terminal with `script`, so colour is decided by the flag alone.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn run_in_terminal(directory: &Path, args: &[&str]) -> String {
    let binary = env!("CARGO_BIN_EXE_adrlint");
    let mut command = Command::new("script");
    if cfg!(target_os = "macos") {
        command.args(["-q", "/dev/null", binary]).args(args);
    } else {
        let line = std::iter::once(binary)
            .chain(args.iter().copied())
            .map(|part| format!("'{part}'"))
            .collect::<Vec<_>>()
            .join(" ");
        command.args(["-qec", &line, "/dev/null"]);
    }
    let output = command
        .current_dir(directory)
        .env_remove("NO_COLOR")
        .output()
        .expect("running script");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn should_honour_no_color_when_lock_reports_errors() {
    let directory = TempDir::new().unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/adr/invalid/MADR41a");
    let fixtures = fixtures.to_str().expect("a UTF-8 path");

    let coloured = run_in_terminal(directory.path(), &["lock", fixtures]);
    assert!(coloured.contains("[MADR41a]"), "{coloured}");
    assert!(
        coloured.contains('\u{1b}'),
        "a terminal gets colour by default: {coloured:?}"
    );

    let plain = run_in_terminal(directory.path(), &["--no-color", "lock", fixtures]);
    assert!(plain.contains("[MADR41a]"), "{plain}");
    assert!(!plain.contains('\u{1b}'), "--no-color must disable colour: {plain:?}");
}
