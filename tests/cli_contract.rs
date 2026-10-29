//! Contract tests for the commands that write files: `format`, `init` and `new`.

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

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A project with the starter configuration and its accepted ADR.
fn scaffold() -> (TempDir, PathBuf) {
    let directory = TempDir::new().expect("temporary directory");
    let output = run(adrlint().current_dir(directory.path()).arg("init"));
    assert!(output.status.success(), "{}", stderr(&output));
    let adr = directory.path().join("docs/adr/0001-record-architecture-decisions.md");
    (directory, adr)
}

/// Appends a bare URL, which MD034 rewrites into an autolink: a canonical content change.
fn add_bare_url(path: &Path) {
    let source = fs::read_to_string(path).expect("reading the ADR");
    fs::write(path, format!("{source}\nSee https://example.com for details.\n")).expect("writing the ADR");
}

/// Adds trailing spaces to the last line of a paragraph, which is formatting only.
fn add_trailing_spaces(path: &Path) {
    let source = fs::read_to_string(path).expect("reading the ADR");
    let spaced = source.replacen(
        "Describe the context and the problem statement here, in two or three sentences.",
        "Describe the context and the problem statement here, in two or three sentences.   ",
        1,
    );
    assert_ne!(spaced, source, "the fixture must contain the paragraph");
    fs::write(path, spaced).expect("writing the ADR");
}

#[test]
fn should_refuse_to_format_locked_content_into_a_change() {
    let (directory, adr) = scaffold();
    add_bare_url(&adr);
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[markdown.rules]\nMD034 = \"warn\"\n\n[rules]\nMD034 = \"warn\"\n",
    )
    .expect("writing the configuration");

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));
    let locked = fs::read(&adr).expect("reading the locked ADR");

    let output = run(adrlint().current_dir(directory.path()).args(["format", "--check"]));
    assert_eq!(output.status.code(), Some(1), "--check must fail: {}", stderr(&output));
    assert!(
        stderr(&output).contains("refusing to change locked content"),
        "{}",
        stderr(&output)
    );
    assert_eq!(fs::read(&adr).unwrap(), locked, "--check must not write");

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("(use --force)"), "{}", stderr(&output));
    assert_eq!(
        fs::read(&adr).unwrap(),
        locked,
        "format must leave the locked ADR untouched"
    );

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "the lock must survive a refused format: {}",
        stdout(&output)
    );
    assert!(!stdout(&output).contains("[ADR50]"));

    let output = run(adrlint().current_dir(directory.path()).args(["format", "--force"]));
    assert!(output.status.success(), "{}", stderr(&output));
    let formatted = fs::read_to_string(&adr).expect("reading the formatted ADR");
    assert!(
        formatted.contains("<https://example.com>"),
        "MD034 should have rewritten the URL: {formatted}"
    );

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("[ADR50]"),
        "a forced content change must surface as ADR50: {}",
        stdout(&output)
    );
}

#[test]
fn should_write_formatting_the_canonical_hash_ignores() {
    let (directory, adr) = scaffold();

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    add_trailing_spaces(&adr);
    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(
        output.status.success(),
        "a formatting-only change must be written without --force: {}",
        stderr(&output)
    );
    let formatted = fs::read_to_string(&adr).expect("reading the formatted ADR");
    assert!(!formatted.contains("sentences.   "), "MD009 should have run");

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(
        output.status.success(),
        "a formatting-only change must not break the lock: {}",
        stdout(&output)
    );
}

#[test]
fn should_not_let_a_stale_lock_block_formatting() {
    let (directory, adr) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    let manifest_path = directory.path().join("adrlint.lock");
    let manifest = fs::read_to_string(&manifest_path).expect("reading the lock file");
    fs::write(&manifest_path, manifest.replace("canonical = 1", "canonical = 999")).expect("ageing the lock");

    add_trailing_spaces(&adr);
    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(
        output.status.success(),
        "a stale manifest must not block formatting: {}",
        stderr(&output)
    );
    let formatted = fs::read_to_string(&adr).expect("reading the formatted ADR");
    assert!(
        !formatted.contains("sentences.   "),
        "MD009 should have run: {formatted}"
    );
}

#[test]
fn should_refuse_a_content_change_to_a_locked_adr_under_a_stale_lock() {
    let (directory, adr) = scaffold();
    fs::write(
        directory.path().join(".adrlint.toml"),
        "[markdown.rules]\nMD034 = \"warn\"\n\n[rules]\nMD034 = \"warn\"\n",
    )
    .expect("writing the configuration");
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    let manifest_path = directory.path().join("adrlint.lock");
    let manifest = fs::read_to_string(&manifest_path).expect("reading the lock file");
    fs::write(&manifest_path, manifest.replace("canonical = 1", "canonical = 999")).expect("ageing the lock");
    add_bare_url(&adr);
    let before = fs::read(&adr).expect("reading the ADR");

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("(use --force)"), "{}", stderr(&output));
    assert_eq!(
        fs::read(&adr).unwrap(),
        before,
        "a stale lock must not let format change content"
    );

    let output = run(adrlint().current_dir(directory.path()).args(["format", "--force"]));
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(fs::read_to_string(&adr).unwrap().contains("<https://example.com>"));
}

#[test]
fn should_write_a_reflow_of_a_locked_adr_that_already_drifted() {
    let (directory, adr) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    // A content edit after locking: ADR50 until it is unlocked.
    let source = fs::read_to_string(&adr).expect("reading the ADR");
    fs::write(
        &adr,
        source.replacen("two or three sentences", "two or four sentences", 1),
    )
    .expect("editing");
    add_trailing_spaces_after(&adr, "two or four sentences.");

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(
        output.status.success(),
        "a formatting-only fix must not be refused because of earlier drift: {}",
        stderr(&output)
    );
    let formatted = fs::read_to_string(&adr).expect("reading the formatted ADR");
    assert!(
        !formatted.contains("sentences.   "),
        "MD009 should have run: {formatted}"
    );
}

fn add_trailing_spaces_after(path: &Path, text: &str) {
    let source = fs::read_to_string(path).expect("reading the ADR");
    let spaced = source.replacen(text, &format!("{text}   "), 1);
    assert_ne!(spaced, source, "the ADR must contain {text:?}");
    fs::write(path, spaced).expect("writing the ADR");
}

#[cfg(unix)]
#[test]
fn should_escape_a_directory_containing_quotes() {
    let directory = TempDir::new().expect("temporary directory");
    let dir = "decisions \"draft\"";
    let output = run(adrlint().current_dir(directory.path()).args(["init", "--dir", dir]));
    assert!(output.status.success(), "{}", stderr(&output));

    let rendered = fs::read_to_string(directory.path().join(".adrlint.toml")).expect("reading the configuration");
    let parsed: toml::Value = toml::from_str(&rendered).expect("the rendered configuration must parse");
    assert_eq!(parsed["adr"]["dir"].as_str(), Some(dir));
    assert!(directory.path().join(dir).is_dir(), "the ADR directory exists");
}

#[cfg(unix)]
#[test]
fn should_not_follow_a_symlinked_adr_when_formatting() {
    let (directory, adr) = scaffold();
    let outside = directory.path().join("outside.md");
    fs::rename(&adr, &outside).expect("moving the ADR out of the directory");
    std::os::unix::fs::symlink(&outside, &adr).expect("linking the ADR");
    add_trailing_spaces(&outside);
    let target = fs::read(&outside).expect("reading the target");

    let output = run(adrlint().current_dir(directory.path()).arg("format"));
    assert!(!output.status.success(), "a symlinked ADR must be refused");
    assert!(stderr(&output).contains("symlink"), "{}", stderr(&output));
    assert_eq!(
        fs::read(&outside).unwrap(),
        target,
        "the link target must not be modified"
    );
}

#[cfg(unix)]
#[test]
fn should_refuse_to_overwrite_a_symlinked_configuration_even_with_force() {
    let directory = TempDir::new().expect("temporary directory");
    let target = directory.path().join("outside.toml");
    fs::write(&target, "untouched\n").expect("writing the target");
    std::os::unix::fs::symlink(&target, directory.path().join(".adrlint.toml")).expect("linking the config");

    let output = run(adrlint().current_dir(directory.path()).args(["init", "--force"]));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("symlink"), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&target).unwrap(), "untouched\n");
}

#[cfg(unix)]
#[test]
fn should_not_write_lock_through_a_symlinked_manifest() {
    let (directory, _) = scaffold();
    let outside = directory.path().join("outside.toml");
    fs::write(&outside, "version = 1\ncanonical = 1\n").expect("writing the target");
    std::os::unix::fs::symlink(&outside, directory.path().join("adrlint.lock")).expect("linking the manifest");

    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("symlink"), "{}", stderr(&output));
    assert_eq!(
        fs::read_to_string(&outside).unwrap(),
        "version = 1\ncanonical = 1\n",
        "the link target must not be modified"
    );
}

#[test]
fn should_reject_a_title_with_a_newline() {
    let (directory, _) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).args(["new", "Use\nPostgreSQL"]));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("control characters"), "{}", stderr(&output));
    assert_eq!(
        fs::read_dir(directory.path().join("docs/adr")).unwrap().count(),
        1,
        "no ADR is created for an invalid title"
    );
}

#[test]
fn should_reject_a_title_with_a_control_character() {
    let (directory, _) = scaffold();
    let output = run(adrlint()
        .current_dir(directory.path())
        .args(["new", "Use\u{7}PostgreSQL"]));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("control characters"), "{}", stderr(&output));
}

#[test]
fn should_reject_a_title_without_an_ascii_slug() {
    let (directory, _) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).args(["new", "!!!"]));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("at least one ASCII letter or digit"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn should_reject_an_empty_title() {
    let (directory, _) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).args(["new", "   "]));
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("must not be empty"), "{}", stderr(&output));
}

#[test]
fn should_truncate_a_long_slug_at_a_hyphen() {
    let (directory, _) = scaffold();
    let title = "very ".repeat(40);
    let output = run(adrlint().current_dir(directory.path()).args(["new", title.trim()]));
    assert!(output.status.success(), "{}", stderr(&output));

    let created = fs::read_dir(directory.path().join("docs/adr"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .find(|name| name.starts_with("0002-"))
        .expect("the new ADR");
    let stem = created.strip_suffix(".md").expect("a Markdown file");
    let slug = &stem[5..];
    assert!(slug.len() <= 80, "slug is {} bytes: {slug}", slug.len());
    assert!(!slug.ends_with('-'), "the truncation lands on a hyphen boundary");
}

#[test]
fn should_verify_nothing_against_a_stale_lock() {
    let (directory, _) = scaffold();
    let output = run(adrlint().current_dir(directory.path()).arg("lock"));
    assert!(output.status.success(), "{}", stderr(&output));

    let manifest_path = directory.path().join("adrlint.lock");
    let manifest = fs::read_to_string(&manifest_path).expect("reading the lock file");
    let aged = manifest
        .replace("canonical = 1", "canonical = 999")
        .replace("0001-record-architecture-decisions.md", "0009-gone.md");
    assert_ne!(aged, manifest);
    fs::write(&manifest_path, aged).expect("ageing the lock");

    let output = run(adrlint().current_dir(directory.path()).arg("lint"));
    assert!(!stdout(&output).contains("[ADR51]"), "{}", stdout(&output));
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    assert!(stderr(&output).contains("canonical version 999"), "{}", stderr(&output));
}
