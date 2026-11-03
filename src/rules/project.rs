//! The project-level entry point: tree-wide and lock rules over an explicit file set.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::adr;
use crate::config::AdrConfig;
use crate::diagnostics::{self, Diagnostic};
use crate::discovery;
use crate::lock::{self, LockFile};

use super::check_directory_with_images;

/// Runs the tree-wide rules (`MADR41`-`MADR44`) and the lock rules (`ADR50`, `ADR51`) over `files`.
///
/// This is the pure counterpart of the CLI's directory walk: it reads no file, walks no directory
/// and makes no network request (`MADR11` stays in [`check_external_links`](super::check_external_links)).
///
/// - `files` is every file of the project that belongs to an ADR log, as `(path, content)`. Files are
///   grouped by parent directory and classified by name exactly as [`discovery::scan`] does: `.md`
///   files are ADR candidates, `README.md` and `index.md` are indexes, anything else is foreign
///   (`MADR42`; its content is ignored, and no image is treated as referenced). Paths are relative to
///   the project root, because lock keys are.
/// - `lock` is the parsed manifest ([`lock::parse_manifest`] parses text in memory), or `None` for
///   no enforcement. A manifest whose serializer version is stale verifies nothing, as in the CLI.
/// - The result is sorted by [`diagnostics::sort`] and does not depend on the order of `files`.
///
/// Per-document rules are not run here; call [`check_document`](super::check_document) per file.
pub fn check_project(files: &[(PathBuf, &str)], lock: Option<&LockFile>, config: &AdrConfig) -> Vec<Diagnostic> {
    let sources = files
        .iter()
        .map(|(path, source)| (path.as_path(), *source))
        .collect::<BTreeMap<&Path, &str>>();

    let mut by_directory = BTreeMap::<&Path, Vec<PathBuf>>::new();
    for path in sources.keys() {
        by_directory
            .entry(path.parent().unwrap_or_else(|| Path::new("")))
            .or_default()
            .push(path.to_path_buf());
    }

    let no_images = BTreeSet::new();
    let mut findings = Vec::new();
    let mut adr_paths = Vec::new();
    for (directory, paths) in by_directory {
        let directory = discovery::classify(directory, paths);
        findings.extend(check_directory_with_images(&directory, config, &no_images));
        adr_paths.extend(directory.adrs.into_iter().map(|file| file.path));
    }

    if let Some(manifest) = lock.filter(|manifest| manifest.is_current()) {
        for path in &adr_paths {
            let Some(entry) = manifest.entry(&lock::key(Path::new(""), path)) else {
                continue;
            };
            let document = adr::parse(sources[path.as_path()]);
            findings.extend(lock::verify(&document, Some(entry), path));
        }
        findings.extend(lock::missing_where(manifest, |key| {
            sources.contains_key(Path::new(key))
        }));
    }

    diagnostics::sort(&mut findings);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::LockEntry;
    use crate::rules::check_directory;

    const ADR: &str = "# Use Rust\n\n## Context and Problem Statement\n\nText.\n";
    const CHANGED: &str = "# Use Go\n\n## Context and Problem Statement\n\nText.\n";

    fn files(names: &[&str]) -> Vec<(PathBuf, &'static str)> {
        names.iter().map(|name| (PathBuf::from(name), ADR)).collect()
    }

    fn summary(findings: &[Diagnostic]) -> Vec<(String, &'static str)> {
        findings
            .iter()
            .map(|finding| (finding.path.display().to_string(), finding.code))
            .collect()
    }

    fn locked(path: &str, source: &str) -> LockFile {
        let mut manifest = LockFile::default();
        manifest.insert(
            path,
            LockEntry::new(1, "accepted", lock::canonical_hash(&adr::parse(source))),
        );
        manifest
    }

    fn pair(path: &str, code: &'static str) -> (String, &'static str) {
        (path.to_owned(), code)
    }

    #[test]
    fn should_report_gap_duplicate_and_bad_name_without_touching_disk() {
        let input = files(&[
            "docs/adr/0001-one.md",
            "docs/adr/0004-four.md",
            "docs/adr/0004-also-four.md",
            "docs/adr/5-bad.md",
            "docs/adr/notes.txt",
        ]);
        let findings = check_project(&input, None, &AdrConfig::default());
        assert_eq!(
            summary(&findings),
            [
                // Numbers 0002 and 0003 are missing; each gap is reported once, on the first 0004 file.
                pair("docs/adr/0004-also-four.md", "MADR41b"),
                pair("docs/adr/0004-also-four.md", "MADR41b"),
                pair("docs/adr/0004-also-four.md", "MADR43"),
                pair("docs/adr/0004-four.md", "MADR43"),
                pair("docs/adr/5-bad.md", "MADR44"),
                pair("docs/adr/notes.txt", "MADR42"),
            ]
        );
    }

    #[test]
    fn should_keep_indexes_out_of_the_log() {
        let input = files(&["docs/adr/README.md", "docs/adr/0001-one.md"]);
        assert!(check_project(&input, None, &AdrConfig::default()).is_empty());
    }

    #[test]
    fn should_check_each_directory_on_its_own() {
        let input = files(&["a/0001-one.md", "a/0002-two.md", "b/0001-one.md", "b/0003-three.md"]);
        let findings = check_project(&input, None, &AdrConfig::default());
        assert_eq!(summary(&findings), [pair("b/0003-three.md", "MADR41b")]);
    }

    #[test]
    fn should_report_a_changed_locked_adr_from_an_in_memory_lock() {
        let manifest = locked("docs/adr/0001-one.md", ADR);
        let changed = vec![(PathBuf::from("docs/adr/0001-one.md"), CHANGED)];
        let findings = check_project(&changed, Some(&manifest), &AdrConfig::default());
        assert_eq!(summary(&findings), [pair("docs/adr/0001-one.md", "ADR50")]);

        let unchanged = files(&["docs/adr/0001-one.md"]);
        assert!(check_project(&unchanged, Some(&manifest), &AdrConfig::default()).is_empty());
    }

    #[test]
    fn should_report_a_locked_adr_missing_from_the_file_set() {
        let manifest = locked("docs/adr/0002-two.md", ADR);
        let input = files(&["docs/adr/0001-one.md"]);
        let findings = check_project(&input, Some(&manifest), &AdrConfig::default());
        assert_eq!(summary(&findings), [pair("docs/adr/0002-two.md", "ADR51")]);
    }

    #[test]
    fn should_not_verify_against_a_stale_manifest() {
        let mut manifest = locked("docs/adr/0002-two.md", ADR);
        manifest.canonical += 1;
        let input = files(&["docs/adr/0001-one.md"]);
        assert!(check_project(&input, Some(&manifest), &AdrConfig::default()).is_empty());
    }

    #[test]
    fn should_parse_a_lock_from_text() {
        let expected = locked("docs/adr/0001-one.md", ADR);
        let parsed = lock::parse_manifest(&lock::render(&expected).unwrap()).unwrap();
        assert_eq!(parsed, expected);
        assert!(lock::parse_manifest("adrs = 3").is_err());
        let escaping =
            "version = 1\ncanonical = 1\n[adrs.\"/etc/passwd\"]\nnumber = 1\nstatus = \"a\"\nblake3 = \"x\"\n";
        assert!(lock::parse_manifest(escaping).is_err());
    }

    #[test]
    fn should_order_findings_independently_of_input_order() {
        let mut input = files(&[
            "docs/adr/0004-four.md",
            "docs/adr/0001-one.md",
            "docs/adr/0004-also-four.md",
            "docs/adr/5-bad.md",
            "other/0003-three.md",
        ]);
        let manifest = locked("docs/adr/0009-gone.md", ADR);
        let forward = check_project(&input, Some(&manifest), &AdrConfig::default());
        input.reverse();
        let backward = check_project(&input, Some(&manifest), &AdrConfig::default());
        assert_eq!(forward, backward);
        let mut sorted = forward.clone();
        diagnostics::sort(&mut sorted);
        assert_eq!(forward, sorted);
        assert!(!forward.is_empty());
    }

    #[test]
    fn should_match_check_directory_over_the_same_contents() {
        let temp = tempfile::tempdir().unwrap();
        let names = [
            "0001-one.md",
            "0004-four.md",
            "0004-also-four.md",
            "5-bad.md",
            "notes.txt",
            "README.md",
        ];
        for name in names {
            std::fs::write(temp.path().join(name), ADR).unwrap();
        }
        let config = AdrConfig::default();

        let directory = discovery::scan(temp.path()).unwrap();
        let mut from_disk = check_directory(&directory, &config);
        diagnostics::sort(&mut from_disk);

        let input = names
            .iter()
            .map(|name| (temp.path().join(name), ADR))
            .collect::<Vec<_>>();
        let from_memory = check_project(&input, None, &config);

        assert!(!from_disk.is_empty());
        assert_eq!(from_memory, from_disk);
    }
}
