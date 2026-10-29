//! The documented configuration precedence: nearest project file, then each ancestor, then the
//! global file, then built-in defaults.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use adrlint::config;
use tempfile::TempDir;

/// Serialises the tests that change the process working directory.
static CWD: Mutex<()> = Mutex::new(());

/// Changes the working directory for the lifetime of the value and restores it on drop, including
/// while unwinding from a failed assertion.
struct InDir {
    previous: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl InDir {
    fn new(path: &Path) -> Self {
        let guard = CWD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::current_dir().expect("the current directory");
        std::env::set_current_dir(path).expect("changing the current directory");
        Self {
            previous,
            _guard: guard,
        }
    }
}

impl Drop for InDir {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("creating the configuration directory");
    }
    fs::write(path, contents).expect("writing the configuration");
}

fn global_path(workspace: &TempDir) -> PathBuf {
    workspace.path().join("global/config.toml")
}

#[test]
fn should_inherit_an_ancestor_key_and_override_only_the_nearest_one() {
    let workspace = TempDir::new().expect("temporary directory");
    let root = workspace.path().join("project");
    let leaf = root.join("crates/cli");
    write(
        &root.join(".adrlint.toml"),
        "[rules]\nMADR05 = \"off\"\nMADR07 = \"error\"\n",
    );
    write(&leaf.join(".adrlint.toml"), "[rules]\nMADR05 = \"warn\"\n");
    let global = global_path(&workspace);

    let _cwd = InDir::new(&leaf);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.rules.get("MADR05").map(String::as_str), Some("warn"));
    assert_eq!(
        loaded.config.rules.get("MADR07").map(String::as_str),
        Some("error"),
        "a key the nearest file does not set is inherited"
    );
}

#[test]
fn should_apply_three_levels_of_configuration() {
    let workspace = TempDir::new().expect("temporary directory");
    let root = workspace.path().join("project");
    let middle = root.join("middle");
    let leaf = middle.join("leaf");
    write(&root.join(".adrlint.toml"), "[rules]\nMADR05 = \"off\"\n");
    write(&middle.join(".adrlint.toml"), "[markdown]\nenabled = false\n");
    write(&leaf.join(".adrlint.toml"), "[adr]\nstart = 0\n");
    let global = global_path(&workspace);

    let _cwd = InDir::new(&leaf);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.rules.get("MADR05").map(String::as_str), Some("off"));
    assert!(!loaded.config.markdown.enabled);
    assert_eq!(loaded.config.adr.start, 0);
}

#[test]
fn should_apply_the_global_file_below_a_project_key() {
    let workspace = TempDir::new().expect("temporary directory");
    let project = workspace.path().join("project");
    write(
        &global_path(&workspace),
        "[rules]\nMADR05 = \"off\"\nMADR06 = \"warn\"\n\n[markdown]\nenabled = false\n",
    );
    write(&project.join(".adrlint.toml"), "[rules]\nMADR06 = \"error\"\n");

    let _cwd = InDir::new(&project);
    let loaded = config::load_with_global(None, Some(&global_path(&workspace))).expect("loading");

    assert_eq!(
        loaded.config.rules.get("MADR05").map(String::as_str),
        Some("off"),
        "a global key no project file sets applies"
    );
    assert_eq!(
        loaded.config.rules.get("MADR06").map(String::as_str),
        Some("error"),
        "a project key beats the global one"
    );
    assert!(!loaded.config.markdown.enabled);
}

#[test]
fn should_not_merge_the_deprecated_file_when_the_preferred_one_exists() {
    let workspace = TempDir::new().expect("temporary directory");
    let project = workspace.path().join("project");
    write(&project.join(".adrlint.toml"), "[rules]\nMADR05 = \"off\"\n");
    write(
        &project.join("adrlint.toml"),
        "[rules]\nMADR06 = \"warn\"\n\n[adr]\ndir = \"legacy\"\n",
    );
    let global = global_path(&workspace);

    let _cwd = InDir::new(&project);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.rules.get("MADR05").map(String::as_str), Some("off"));
    assert_eq!(
        loaded.config.rules.get("MADR06"),
        None,
        "the deprecated file is not merged when the preferred name exists"
    );
    assert_eq!(
        loaded.config.adr.dir,
        Path::new("docs/adr"),
        "the deprecated file's [adr] table is ignored too"
    );
}

#[test]
fn should_merge_rule_maps_key_by_key_across_layers() {
    let workspace = TempDir::new().expect("temporary directory");
    let root = workspace.path().join("project");
    let leaf = root.join("nested");
    write(
        &root.join(".adrlint.toml"),
        "[rules]\nMADR05 = \"off\"\nMADR06 = \"warn\"\nMADR07 = \"error\"\n",
    );
    write(&leaf.join(".adrlint.toml"), "[rules]\nMADR06 = \"off\"\n");
    let global = global_path(&workspace);

    let _cwd = InDir::new(&leaf);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.rules.len(), 3, "keys are folded, not replaced");
    assert_eq!(loaded.config.rules.get("MADR05").map(String::as_str), Some("off"));
    assert_eq!(
        loaded.config.rules.get("MADR06").map(String::as_str),
        Some("off"),
        "the nearest layer wins per key"
    );
    assert_eq!(loaded.config.rules.get("MADR07").map(String::as_str), Some("error"));
}

#[test]
fn should_merge_markdown_rule_maps_key_by_key_across_layers() {
    let workspace = TempDir::new().expect("temporary directory");
    let root = workspace.path().join("project");
    let leaf = root.join("nested");
    write(
        &root.join(".adrlint.toml"),
        "[markdown.rules]\nMD009 = \"error\"\nMD010 = \"error\"\n",
    );
    write(&leaf.join(".adrlint.toml"), "[markdown.rules]\nMD010 = \"off\"\n");
    let global = global_path(&workspace);

    let _cwd = InDir::new(&leaf);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.markdown.setting("MD009"), Some("error"));
    assert_eq!(loaded.config.markdown.setting("MD010"), Some("off"));
    // The built-in defaults survive a layer that does not name them.
    assert_eq!(loaded.config.markdown.setting("MD013"), Some("off"));
    assert_eq!(loaded.config.markdown.setting("MD033"), Some("off"));
}

#[test]
fn should_replace_the_lock_statuses_as_a_whole() {
    let workspace = TempDir::new().expect("temporary directory");
    let project = workspace.path().join("project");
    write(&project.join(".adrlint.toml"), "[lock]\nstatuses = [\"proposed\"]\n");
    let global = global_path(&workspace);

    let _cwd = InDir::new(&project);
    let loaded = config::load_with_global(None, Some(&global)).expect("loading the configuration");

    assert_eq!(loaded.config.lock.statuses, vec!["proposed".to_owned()]);
}
