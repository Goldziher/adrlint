//! The poly hook catalog (`poly-hooks.toml`) and the pre-commit hooks (`.pre-commit-hooks.yaml`)
//! describe the same three gates, so a change to one must not leave the other behind.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use toml::Value;

/// Each poly hook id and the pre-commit hook id that runs the same `adrlint` command.
const CORRESPONDING: [(&str, &str); 3] = [
    ("adrlint-lint", "adrlint-lint"),
    ("adrlint-format-check", "adrlint"),
    ("adrlint-lock-check", "adrlint-lock"),
];

/// Keys poly 0.31 accepts on a catalog hook that this catalog uses; anything else is a typo poly rejects.
const HOOK_KEYS: [&str; 8] = [
    "id",
    "stages",
    "args",
    "files",
    "pass_filenames",
    "always_run",
    "workspace",
    "paths",
];

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
}

fn catalog_hooks() -> Vec<Value> {
    let catalog: Value = toml::from_str(&read("poly-hooks.toml")).expect("poly-hooks.toml parses");
    assert_eq!(
        catalog["version"].as_integer(),
        Some(1),
        "poly supports catalog version 1 only"
    );
    catalog["hooks"].as_array().expect("a [[hooks]] array").clone()
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|item| item.as_str().expect("a string").to_owned())
        .collect()
}

/// The `id` and `entry` of every hook in `.pre-commit-hooks.yaml`, in order.
fn pre_commit_entries() -> BTreeMap<String, String> {
    let mut entries = BTreeMap::new();
    let mut id = None;
    for line in read(".pre-commit-hooks.yaml").lines() {
        let line = line.trim_start_matches("- ").trim();
        if let Some(value) = line.strip_prefix("id: ") {
            id = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("entry: ") {
            entries.insert(id.take().expect("an id before the entry"), value.to_owned());
        }
    }
    entries
}

fn hook<'a>(hooks: &'a [Value], id: &str) -> &'a Value {
    hooks
        .iter()
        .find(|hook| hook["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("the catalog has no hook {id}"))
}

#[test]
fn the_catalog_ids_correspond_to_the_pre_commit_hooks() {
    let hooks = catalog_hooks();
    let entries = pre_commit_entries();
    assert_eq!(entries.len(), CORRESPONDING.len(), "{entries:?}");
    assert_eq!(hooks.len(), CORRESPONDING.len());

    for (poly_id, pre_commit_id) in CORRESPONDING {
        let hook = hook(&hooks, poly_id);
        let entry = &entries[pre_commit_id];
        let subcommand = entry.strip_prefix("adrlint ").expect("an adrlint entry");
        let args = strings(&hook["args"]);
        assert_eq!(
            args[0],
            subcommand.split(' ').next().expect("a subcommand"),
            "{poly_id} must run the same subcommand as pre-commit's {pre_commit_id}"
        );
    }
}

#[test]
fn the_catalog_uses_only_keys_poly_accepts_and_runs_through_a_guarded_path() {
    for hook in catalog_hooks() {
        let id = hook["id"].as_str().expect("an id");
        let table = hook.as_table().expect("a hook table");
        for key in table.keys() {
            assert!(HOOK_KEYS.contains(&key.as_str()), "{id} has an unknown key {key}");
        }
        assert_eq!(strings(&hook["stages"]), ["pre-commit"], "{id}");
        let paths = hook["paths"].as_array().expect("[[hooks.paths]]");
        assert_eq!(paths.len(), 1, "{id}");
        assert_eq!(paths[0]["channel"].as_str(), Some("system"), "{id}");
        assert_eq!(paths[0]["check"].as_str(), Some("command -v adrlint"), "{id}");
        assert_eq!(paths[0]["run"].as_str(), Some("adrlint"), "{id}");
    }
}

#[test]
fn the_catalog_hooks_never_rewrite_files() {
    let hooks = catalog_hooks();
    assert_eq!(strings(&hook(&hooks, "adrlint-lint")["args"]), ["lint"]);
    assert_eq!(
        strings(&hook(&hooks, "adrlint-format-check")["args"]),
        ["format", "--check"]
    );
    assert_eq!(
        strings(&hook(&hooks, "adrlint-lock-check")["args"]),
        ["lock", "--check"]
    );
}

#[test]
fn the_lock_check_always_runs_without_filenames() {
    let hooks = catalog_hooks();
    let lock = hook(&hooks, "adrlint-lock-check");
    assert_eq!(lock["always_run"].as_bool(), Some(true));
    assert_eq!(lock["pass_filenames"].as_bool(), Some(false));
    for id in ["adrlint-lint", "adrlint-format-check"] {
        assert_eq!(hook(&hooks, id)["pass_filenames"].as_bool(), Some(true), "{id}");
    }
}

#[test]
fn the_file_hooks_cover_the_same_directories_as_pre_commit() {
    let hooks = catalog_hooks();
    let expected = [
        "{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md",
        "**/{adr,adrs,decisions,doc/adr,docs/adr}/**/*.md",
    ];
    for id in ["adrlint-lint", "adrlint-format-check"] {
        assert_eq!(strings(&hook(&hooks, id)["files"]), expected, "{id}");
    }
    assert!(
        read(".pre-commit-hooks.yaml").contains(r"files: (^|/)(adr|adrs|decisions|doc/adr|docs/adr)/.*\.md$"),
        "the pre-commit pattern changed; update the catalog globs to match"
    );
}
