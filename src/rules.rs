use std::collections::BTreeMap;

use crate::config::AdrConfig;
use crate::diagnostics::Diagnostic;
use crate::discovery::{AdrFile, Directory};

mod structure;

pub use structure::check_structure;

pub fn check_directory(directory: &Directory, config: &AdrConfig) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_start(directory, &mut diagnostics);
    if !config.allow_gaps {
        check_gaps(directory, &mut diagnostics);
    }
    check_foreign(directory, &mut diagnostics);
    check_duplicates(directory, &mut diagnostics);
    check_names(directory, &mut diagnostics);
    diagnostics
}

fn check_start(directory: &Directory, diagnostics: &mut Vec<Diagnostic>) {
    let Some((number, file)) = directory
        .adrs
        .iter()
        .filter_map(|file| file.number.map(|number| (number, file)))
        .min_by_key(|(number, _)| *number)
    else {
        return;
    };
    if number > 1 {
        diagnostics.push(Diagnostic::error(
            "MADR41a",
            &file.path,
            1,
            format!("expected the first ADR number to be 0000 or 0001, found {number:04}"),
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

fn check_foreign(directory: &Directory, diagnostics: &mut Vec<Diagnostic>) {
    for path in &directory.foreign {
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
    fn should_accept_starting_at_zero() {
        let diagnostics = check_directory(&directory(&["0000-zero.md", "0001-one.md"]), &AdrConfig::default());
        assert!(diagnostics.is_empty());
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
}
