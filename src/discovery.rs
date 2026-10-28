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

pub fn scan(dir: &Path) -> io::Result<Directory> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    entries.sort();

    let mut adrs = Vec::new();
    let mut foreign = Vec::new();
    for path in entries {
        if path.is_dir() {
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
