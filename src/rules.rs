use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::adr::Document;
use crate::config::AdrConfig;
use crate::diagnostics::Diagnostic;
use crate::discovery::{AdrFile, Directory};

mod external;
mod links;
mod metadata;
mod structure;
mod style;

pub use external::{Citation, REQUEST_TIMEOUT, check_external_links, external_citations};
pub use links::{check_links, is_image, referenced_images};
pub use metadata::check_metadata;
pub use structure::check_structure;
pub use style::check_style;

/// Applies `[rules]` severities to the collected findings.
///
/// A key matches a finding when it is a prefix of the code, ignoring case; the longest matching key
/// wins, so `MADR03 = "warn"` covers `MADR03a` and `MADR03b` while an explicit `MADR03b = "off"`
/// still removes only that variant.
pub fn apply_overrides(findings: &mut Vec<Diagnostic>, rules: &BTreeMap<String, String>) {
    if rules.is_empty() {
        return;
    }
    findings.retain_mut(|diagnostic| {
        let Some(setting) = longest_rule_setting(rules, diagnostic.code) else {
            return true;
        };
        match setting.trim().to_ascii_lowercase().as_str() {
            "off" => false,
            "warn" => {
                diagnostic.severity = crate::diagnostics::Severity::Warning;
                true
            }
            "error" => {
                diagnostic.severity = crate::diagnostics::Severity::Error;
                true
            }
            _ => true,
        }
    });
}

fn longest_rule_setting<'a>(rules: &'a BTreeMap<String, String>, code: &str) -> Option<&'a String> {
    rules
        .iter()
        .filter(|(prefix, _)| {
            code.get(..prefix.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        })
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, setting)| setting)
}

/// Every finding code the MADR and ADR rule sets can emit.
///
/// Kept next to [`apply_overrides`] because the two must agree: a key is only meaningful when it can
/// prefix one of these codes or one of rumdl's (`MD###`), which [`apply_overrides`] also covers.
const IMPLEMENTED_CODES: &[&str] = &[
    "MADR01a", "MADR01b", "MADR01c", "MADR01d", "MADR02", "MADR03a", "MADR03b", "MADR04", "MADR05", "MADR06",
    "MADR07a", "MADR07b", "MADR08a", "MADR08b", "MADR11", "MADR12a", "MADR12b", "MADR12c", "MADR12d", "MADR21",
    "MADR31", "MADR41a", "MADR41b", "MADR42", "MADR43", "MADR44", "ADR50", "ADR51", "ADR52",
];

/// The configured keys that cannot match any implemented finding.
///
/// This is the startup validation the CLI performs once, before linting. It is public so the CLI
/// (which owns argument handling) can report the keys without this module depending on it; this
/// module never calls it.
///
/// Binding is longest-prefix match ([`apply_overrides`]), so a `[rules]` key is known when it is a
/// case-insensitive prefix of at least one implemented code: `MADR03` configures `MADR03a` and
/// `MADR03b`, and `MADR03b` configures only that variant. A key that is longer than, or diverges
/// from, every implemented code — `MADR99`, `MADR0é` — does nothing and is returned.
///
/// `[rules]` also applies to Markdown findings, so a key that prefixes one of rumdl's codes is known.
///
/// A `[markdown.rules]` key is known when it is exactly one of rumdl's rule codes, ignoring case; the
/// Markdown layer matches settings by exact code, so `MD999` is reported.
///
/// The result is sorted, deduplicated and trimmed; an empty result means every key binds.
pub fn unknown_rule_keys(rules: &BTreeMap<String, String>, markdown: &BTreeMap<String, String>) -> Vec<String> {
    let mut unknown = rules
        .keys()
        .map(|key| key.trim())
        .filter(|key| !is_implemented_prefix(key))
        .map(str::to_owned)
        .chain(
            markdown
                .keys()
                .map(|key| key.trim())
                .filter(|key| !is_markdown_code(key))
                .map(str::to_owned),
        )
        .collect::<Vec<_>>();
    unknown.sort();
    unknown.dedup();
    unknown
}

fn is_implemented_prefix(key: &str) -> bool {
    let prefixes = |code: &str| code.get(..key.len()).is_some_and(|head| head.eq_ignore_ascii_case(key));
    !key.is_empty()
        && (IMPLEMENTED_CODES.iter().any(|code| prefixes(code))
            || crate::markdown::rule_codes().iter().any(|code| prefixes(code)))
}

fn is_markdown_code(key: &str) -> bool {
    crate::markdown::rule_codes().contains(&key.to_ascii_uppercase())
}

/// Runs every document-level rule against one parsed ADR.
///
/// `root` is the project root, used by rules that resolve links. Family modules
/// append their findings here so the CLI never needs to know which rules exist.
pub fn check_document(document: &Document, path: &Path, root: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = check_structure(document, path);
    diagnostics.extend(check_links(document, path, root));
    diagnostics.extend(check_metadata(document, path));
    diagnostics.extend(check_style(document, path));
    diagnostics
}

/// Runs every directory-level rule, with no image known to be referenced: every non-ADR file is
/// `MADR42`. [`check_directory_with_images`] exempts the images the log's ADRs reference.
pub fn check_directory(directory: &Directory, config: &AdrConfig) -> Vec<Diagnostic> {
    check_directory_with_images(directory, config, &BTreeSet::new())
}

/// Runs every directory-level rule. An image in `referenced` (canonical paths, as
/// [`referenced_images`] returns them for the ADRs of this directory) is an allowed asset, not
/// `MADR42`; an image no ADR references still is.
pub fn check_directory_with_images(
    directory: &Directory,
    config: &AdrConfig,
    referenced: &BTreeSet<PathBuf>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_start(directory, config.start, &mut diagnostics);
    if !config.allow_gaps {
        check_gaps(directory, &mut diagnostics);
    }
    check_foreign(directory, referenced, &mut diagnostics);
    check_duplicates(directory, &mut diagnostics);
    check_names(directory, &mut diagnostics);
    diagnostics
}

/// `MADR41a`: the first ADR number is the configured base.
///
/// `[adr].start` chooses `0000` or `0001` once for the life of the directory. Accepting either
/// value regardless of the configuration would let a directory silently switch bases, so the
/// smallest number must equal the configured base exactly.
fn check_start(directory: &Directory, start: u32, diagnostics: &mut Vec<Diagnostic>) {
    let Some((number, file)) = directory
        .adrs
        .iter()
        .filter_map(|file| file.number.map(|number| (number, file)))
        .min_by_key(|(number, _)| *number)
    else {
        return;
    };
    if number != start {
        diagnostics.push(Diagnostic::error(
            "MADR41a",
            &file.path,
            1,
            format!("expected the first ADR number to be {start:04}, found {number:04}"),
        ));
    }
}

fn check_gaps(directory: &Directory, diagnostics: &mut Vec<Diagnostic>) {
    let mut numbers = directory.adrs.iter().filter_map(|file| file.number).collect::<Vec<_>>();
    numbers.sort_unstable();
    numbers.dedup();

    for pair in numbers.windows(2) {
        let (previous, current) = (pair[0], pair[1]);
        let Some(file) = directory.adrs.iter().find(|file| file.number == Some(current)) else {
            continue;
        };
        for missing in (previous + 1)..current {
            diagnostics.push(Diagnostic::error(
                "MADR41b",
                &file.path,
                1,
                format!("missing ADR number {missing:04} before {}", file.name),
            ));
        }
    }
}

fn check_foreign(directory: &Directory, referenced: &BTreeSet<PathBuf>, diagnostics: &mut Vec<Diagnostic>) {
    for path in &directory.foreign {
        if is_image(path) && std::fs::canonicalize(path).is_ok_and(|canonical| referenced.contains(&canonical)) {
            continue;
        }
        diagnostics.push(Diagnostic::error(
            "MADR42",
            path,
            1,
            "non-ADR file in the ADR directory",
        ));
    }
}

fn check_duplicates(directory: &Directory, diagnostics: &mut Vec<Diagnostic>) {
    let mut by_number: BTreeMap<u32, Vec<&AdrFile>> = BTreeMap::new();
    for file in &directory.adrs {
        if let Some(number) = file.number {
            by_number.entry(number).or_default().push(file);
        }
    }
    for (number, files) in by_number {
        if files.len() < 2 {
            continue;
        }
        for file in &files {
            diagnostics.push(Diagnostic::error(
                "MADR43",
                &file.path,
                1,
                format!("duplicate ADR number {number:04} ({} files)", files.len()),
            ));
        }
    }
}

fn check_names(directory: &Directory, diagnostics: &mut Vec<Diagnostic>) {
    for file in &directory.adrs {
        if !file.valid_name {
            diagnostics.push(Diagnostic::error(
                "MADR44",
                &file.path,
                1,
                "invalid file name; expected NNNN-brief-description.md",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str) -> AdrFile {
        AdrFile {
            path: PathBuf::from(name),
            name: name.to_owned(),
            number: crate::discovery::parse_number(name),
            valid_name: crate::discovery::is_valid_name(name),
        }
    }

    fn directory(names: &[&str]) -> Directory {
        Directory {
            path: PathBuf::from("docs/adr"),
            adrs: names.iter().map(|name| file(name)).collect(),
            foreign: Vec::new(),
        }
    }

    fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
        diagnostics.iter().map(|diagnostic| diagnostic.code).collect()
    }

    #[test]
    fn should_accept_a_contiguous_directory() {
        let diagnostics = check_directory(
            &directory(&["0001-one.md", "0002-two.md", "0003-three.md"]),
            &AdrConfig::default(),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn should_accept_the_configured_start() {
        for (start, names) in [
            (0, &["0000-zero.md", "0001-one.md"][..]),
            (1, &["0001-one.md", "0002-two.md"][..]),
        ] {
            let config = AdrConfig {
                start,
                ..AdrConfig::default()
            };
            let diagnostics = check_directory(&directory(names), &config);
            assert!(
                diagnostics.is_empty(),
                "start {start} should accept {names:?}, found {:?}",
                codes(&diagnostics)
            );
        }
    }

    #[test]
    fn should_report_a_first_number_that_is_not_the_configured_start() {
        for (start, names) in [(0, &["0001-one.md"][..]), (1, &["0000-zero.md", "0001-one.md"][..])] {
            let config = AdrConfig {
                start,
                ..AdrConfig::default()
            };
            let diagnostics = check_directory(&directory(names), &config);
            assert_eq!(
                codes(&diagnostics),
                ["MADR41a"],
                "start {start} should reject {names:?}"
            );
        }
    }

    #[test]
    fn should_report_a_start_above_one() {
        let diagnostics = check_directory(&directory(&["0003-three.md", "0004-four.md"]), &AdrConfig::default());
        assert_eq!(codes(&diagnostics), ["MADR41a"]);
    }

    #[test]
    fn should_report_a_gap_once_per_missing_number() {
        let diagnostics = check_directory(&directory(&["0001-one.md", "0004-four.md"]), &AdrConfig::default());
        assert_eq!(codes(&diagnostics), ["MADR41b", "MADR41b"]);
    }

    #[test]
    fn should_allow_gaps_when_configured() {
        let config = AdrConfig {
            allow_gaps: true,
            ..AdrConfig::default()
        };
        let diagnostics = check_directory(&directory(&["0001-one.md", "0004-four.md"]), &config);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn should_report_every_file_in_a_collision() {
        let diagnostics = check_directory(&directory(&["0001-one.md", "0001-also-one.md"]), &AdrConfig::default());
        assert_eq!(codes(&diagnostics), ["MADR43", "MADR43"]);
    }

    #[test]
    fn should_report_an_invalid_name() {
        let diagnostics = check_directory(&directory(&["0001-one.md", "1-two.md"]), &AdrConfig::default());
        assert_eq!(codes(&diagnostics), ["MADR44"]);
    }

    #[test]
    fn should_report_foreign_files() {
        let mut dir = directory(&["0001-one.md"]);
        dir.foreign.push(PathBuf::from("docs/adr/notes.txt"));
        let diagnostics = check_directory(&dir, &AdrConfig::default());
        assert_eq!(codes(&diagnostics), ["MADR42"]);
    }

    fn settings(keys: &[&str]) -> BTreeMap<String, String> {
        keys.iter().map(|key| ((*key).to_owned(), "warn".to_owned())).collect()
    }

    #[test]
    fn should_accept_every_rule_key_that_prefixes_an_implemented_code() {
        let rules = settings(&["MADR01a", "MADR03", "MADR08b", "MADR12", "ADR50", "adr5", "MADR"]);
        assert_eq!(unknown_rule_keys(&rules, &settings(&[])), Vec::<String>::new());
    }

    #[test]
    fn should_know_every_implemented_code() {
        assert!(unknown_rule_keys(&settings(IMPLEMENTED_CODES), &settings(&[])).is_empty());
    }

    #[test]
    fn should_report_rule_keys_that_match_nothing() {
        let rules = settings(&["MADR99", "MADR0é", "MADR03z", "MD999"]);
        assert_eq!(
            unknown_rule_keys(&rules, &settings(&[])),
            ["MADR03z", "MADR0é", "MADR99", "MD999"]
        );
    }

    #[test]
    fn should_accept_a_markdown_code_in_the_rules_table() {
        // `apply_overrides` applies `[rules]` to rumdl findings too, so the key is not unknown.
        let rules = settings(&["MD009", "md013", "MD0", "MD"]);
        assert_eq!(unknown_rule_keys(&rules, &settings(&[])), Vec::<String>::new());
    }

    #[test]
    fn should_accept_every_rumdl_markdown_code() {
        assert!(unknown_rule_keys(&BTreeMap::new(), &settings(&["MD004", "md013", "MD000"])).is_empty());
    }

    #[test]
    fn should_report_malformed_or_unknown_markdown_codes() {
        let markdown = settings(&["MD13", "MD0133", "MD0é3", "MD-01", "MD999", ""]);
        assert_eq!(
            unknown_rule_keys(&BTreeMap::new(), &markdown),
            ["", "MD-01", "MD0133", "MD0é3", "MD13", "MD999"]
        );
    }

    #[test]
    fn should_sort_and_deduplicate_unknown_keys_across_tables() {
        let rules = settings(&["MD99", "MADR99"]);
        assert_eq!(unknown_rule_keys(&rules, &settings(&["MD99"])), ["MADR99", "MD99"]);
    }
}
