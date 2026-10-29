use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdrFile {
    pub path: PathBuf,
    pub name: String,
    pub number: Option<u32>,
    pub valid_name: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    pub path: PathBuf,
    pub adrs: Vec<AdrFile>,
    pub foreign: Vec<PathBuf>,
}

/// Lists one directory: its Markdown files as ADR candidates and everything else as foreign files.
///
/// Entry types come from the directory read itself, so only a symlink costs an extra `stat`.
pub fn scan(dir: &Path) -> io::Result<Directory> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| {
            entry.map(|entry| {
                let path = entry.path();
                let is_dir = match entry.file_type() {
                    Ok(kind) if !kind.is_symlink() => kind.is_dir(),
                    _ => path.is_dir(),
                };
                (path, is_dir)
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();

    let mut adrs = Vec::new();
    let mut foreign = Vec::new();
    for (path, is_dir) in entries {
        if is_dir {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            foreign.push(path);
            continue;
        };
        if !name.ends_with(".md") {
            foreign.push(path);
            continue;
        }
        adrs.push(AdrFile {
            path: path.clone(),
            name: name.to_owned(),
            number: parse_number(name),
            valid_name: is_valid_name(name),
        });
    }

    Ok(Directory {
        path: dir.to_path_buf(),
        adrs,
        foreign,
    })
}

pub fn parse_number(name: &str) -> Option<u32> {
    let bytes = name.as_bytes();
    if bytes.len() < 5 || bytes[4] != b'-' || !bytes[..4].iter().all(u8::is_ascii_digit) {
        return None;
    }
    name[..4].parse().ok()
}

/// Whether a file name claims a place in an ADR log: Markdown with a numeric prefix that is not a date.
///
/// The prefix alone decides, so a malformed name such as `0001_use_rust.md`, `001-use-rust.md` or
/// `00001-use-rust.md` still belongs to the log and reaches `MADR44`. The prefix is four or more digits,
/// or three digits followed by `-` or `_`, so `404.md` is not an ADR. A dated document such as
/// `2026-07-01-design.md` (plans, specs, meeting notes) does not belong either: its `YYYY-MM-DD-`
/// prefix is a date, not a decision number.
pub fn is_numbered(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let bytes = stem.as_bytes();
    let digits = bytes.iter().take_while(|byte| byte.is_ascii_digit()).count();
    let prefixed = digits >= 4 || (digits == 3 && matches!(bytes.get(3), Some(b'-' | b'_')));
    prefixed && !is_dated(bytes)
}

fn is_dated(bytes: &[u8]) -> bool {
    let digits =
        |range: std::ops::Range<usize>| bytes.get(range).is_some_and(|part| part.iter().all(u8::is_ascii_digit));
    bytes.get(4) == Some(&b'-')
        && digits(5..7)
        && bytes.get(7) == Some(&b'-')
        && digits(8..10)
        && bytes.get(10) == Some(&b'-')
}

pub fn is_valid_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let bytes = stem.as_bytes();
    if bytes.len() < 6 || !bytes[..4].iter().all(u8::is_ascii_digit) || bytes[4] != b'-' {
        return false;
    }

    let slug = &stem[5..];
    if slug.is_empty() || slug.starts_with('-') || slug.ends_with('-') {
        return false;
    }
    let mut previous_hyphen = false;
    for byte in slug.bytes() {
        if byte == b'-' {
            if previous_hyphen {
                return false;
            }
            previous_hyphen = true;
        } else if byte.is_ascii_alphanumeric() {
            previous_hyphen = false;
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_four_digit_prefix() {
        assert_eq!(parse_number("0007-use-rust.md"), Some(7));
        assert_eq!(parse_number("0000-record.md"), Some(0));
    }

    #[test]
    fn should_reject_names_without_a_four_digit_prefix() {
        assert_eq!(parse_number("7-use-rust.md"), None);
        assert_eq!(parse_number("0007.md"), None);
        assert_eq!(parse_number("00a7-use-rust.md"), None);
        assert_eq!(parse_number("0007_use_rust.md"), None);
    }

    #[test]
    fn should_treat_a_numbered_name_as_part_of_the_log() {
        assert!(is_numbered("0001-use-rust.md"));
        assert!(is_numbered("0001_use_rust.md"));
        assert!(is_numbered("2026-use-rust.md"));
        assert!(is_numbered("001-use-rust.md"));
        assert!(is_numbered("00001-use-rust.md"));
        assert!(is_numbered("001_use_rust.md"));
        assert!(is_numbered("0001.md"));
    }

    #[test]
    fn should_not_treat_a_dated_document_or_other_markdown_as_an_adr() {
        assert!(!is_numbered("2026-07-01-integration-design.md"));
        assert!(!is_numbered("README.md"));
        assert!(!is_numbered("adr-template.md"));
        assert!(!is_numbered("0001-use-rust.txt"));
        assert!(!is_numbered("01-use-rust.md"));
        assert!(!is_numbered("404.md"));
    }

    #[test]
    fn should_accept_madr_file_names() {
        assert!(is_valid_name("0001-use-CC0-or-MIT-as-license.md"));
        assert!(is_valid_name("0000-random-ass.md"));
        assert!(is_valid_name("0042-choose-a-database.md"));
    }

    #[test]
    fn should_reject_malformed_file_names() {
        assert!(!is_valid_name("42-choose-a-database.md"));
        assert!(!is_valid_name("00042-choose-a-database.md"));
        assert!(!is_valid_name("0001-choose_a_database.md"));
        assert!(!is_valid_name("0001-choose a database.md"));
        assert!(!is_valid_name("0001--choose-a-database.md"));
        assert!(!is_valid_name("0001-choose-a-database-.md"));
        assert!(!is_valid_name("0001-choose-a-database.txt"));
        assert!(!is_valid_name("0001.md"));
    }
}
