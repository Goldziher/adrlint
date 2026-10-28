//! The link rules, `MADR12`.
//!
//! A port of madrlint's `Rule12` over the parsed [`Document`]. Every destination is classified and
//! checked offline: relative paths against the ADR's directory, root-relative paths against the
//! project root, anchors against the heading slugs, and absolute filesystem paths rejected because
//! a renderer cannot follow them. External links belong to `MADR11` and are skipped here.
//!
//! Two deliberate divergences from madrlint:
//!
//! - `/path` is root-relative (`MADR12c`), never absolute. madrlint tests for an absolute path
//!   first, so on Unix it reports every root-relative link as `MADR12d` and `MADR12c` never fires.
//! - Only paths a renderer cannot follow (`C:\`, `\\server\share`, `file://`) are `MADR12d`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::adr::{Document, Link};
use crate::diagnostics::Diagnostic;

/// Checks every link and image destination against the file system and the heading slugs.
pub fn check_links(document: &Document, path: &Path, root: &Path) -> Vec<Diagnostic> {
    let slugs = heading_slugs(document);
    let directory = path.parent().unwrap_or_else(|| Path::new(""));

    let mut diagnostics = Vec::new();
    for link in &document.links {
        check_link(link, directory, root, &slugs, path, &mut diagnostics);
    }
    diagnostics
}

/// Where a destination points, which decides the rule that judges it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// `http:`, `https:`, `mailto:` and every other URI scheme: `MADR11`'s business.
    External,
    /// `#fragment`: a heading in the same document.
    Anchor,
    /// `/path`: relative to the project root.
    RootRelative,
    /// A filesystem path such as `C:\...`, `\\server\share` or `file:///...`, which a renderer
    /// cannot follow.
    Absolute,
    /// A path relative to the ADR's own directory.
    Relative,
}

fn check_link(
    link: &Link,
    directory: &Path,
    root: &Path,
    slugs: &BTreeSet<String>,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let url = link.url.as_str();
    if url.is_empty() {
        return;
    }

    match classify(url) {
        Target::External => {}
        Target::Absolute => diagnostics.push(Diagnostic::error(
            "MADR12d",
            path,
            link.line,
            format!("absolute path '{url}' cannot be rendered; use a repository-relative link"),
        )),
        Target::Anchor => {
            if !slugs.contains(&url[1..]) {
                diagnostics.push(Diagnostic::error(
                    "MADR12b",
                    path,
                    link.line,
                    format!("anchor '{url}' does not match any heading in this document"),
                ));
            }
        }
        Target::RootRelative => {
            let target = strip_target(&url[1..]);
            if !root.join(target).exists() {
                diagnostics.push(Diagnostic::error(
                    "MADR12c",
                    path,
                    link.line,
                    format!("root-relative path '{url}' does not exist under the project root"),
                ));
            }
        }
        Target::Relative => {
            let target = strip_target(url);
            if !directory.join(target).exists() {
                diagnostics.push(Diagnostic::error(
                    "MADR12a",
                    path,
                    link.line,
                    format!("local path '{url}' does not exist"),
                ));
            }
        }
    }
}

/// Classifies a destination. Order matters: a fragment is never a path, and a URI scheme outranks
/// the leading `/` of a scheme-less path.
fn classify(url: &str) -> Target {
    if url.starts_with('#') {
        return Target::Anchor;
    }
    if let Some(scheme) = scheme(url) {
        return if scheme.eq_ignore_ascii_case("file") {
            Target::Absolute
        } else {
            Target::External
        };
    }
    if is_windows_absolute(url) {
        return Target::Absolute;
    }
    if url.starts_with('/') {
        return Target::RootRelative;
    }
    Target::Relative
}

/// The URI scheme of `url`, if it has one. A Windows drive letter is not a scheme.
fn scheme(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once(':')?;
    let mut characters = scheme.chars();
    if !characters.next().is_some_and(|first| first.is_ascii_alphabetic()) {
        return None;
    }
    if !characters.all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')) {
        return None;
    }
    // `C:\...` and `C:/...` are drives; `file:` and `https:` are schemes.
    if scheme.len() == 1 && (rest.starts_with('\\') || rest.starts_with('/')) {
        return None;
    }
    Some(scheme)
}

/// A drive-qualified path (`C:\`) or a UNC path (`\\server\share`).
fn is_windows_absolute(url: &str) -> bool {
    let bytes = url.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');
    drive || url.starts_with("\\\\")
}

/// The path part of a destination, without its `?query` or `#fragment`.
fn strip_target(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    &url[..end]
}

/// GitHub/MADR slugs for every heading, de-duplicated with `-1`, `-2`.
///
/// madrlint's `toSlug` also drops `-` and `_` and never de-duplicates; GitHub keeps both and
/// suffixes repeated slugs, and the anchors authors actually write follow GitHub.
fn heading_slugs(document: &Document) -> BTreeSet<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut slugs = BTreeSet::new();
    for heading in &document.headings {
        let base = slug(&heading.text);
        let count = seen.entry(base.clone()).or_insert(0);
        slugs.insert(if *count == 0 { base } else { format!("{base}-{count}") });
        *count += 1;
    }
    slugs
}

/// Lowercases, drops punctuation other than `-` and `_`, and collapses whitespace to `-`.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, '-' | '_') || character.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;

    #[test]
    fn should_drop_punctuation_from_a_slug() {
        assert_eq!(
            slug("Chosen option: \"Rust\", because it is fast."),
            "chosen-option-rust-because-it-is-fast"
        );
        assert_eq!(slug("Pros and Cons of the Options"), "pros-and-cons-of-the-options");
    }

    #[test]
    fn should_keep_existing_hyphens_and_underscores() {
        assert_eq!(slug("Pre-existing hyphens"), "pre-existing-hyphens");
        assert_eq!(slug("snake_case names"), "snake_case-names");
    }

    #[test]
    fn should_collapse_whitespace_and_trim() {
        assert_eq!(slug("  trailing and   inner  "), "trailing-and-inner");
        assert_eq!(slug("line\nbreak"), "line-break");
    }

    #[test]
    fn should_keep_unicode_letters() {
        assert_eq!(slug("Ünïcode Crème"), "ünïcode-crème");
        assert_eq!(slug("emoji 🚀 dropped"), "emoji-dropped");
    }

    #[test]
    fn should_suffix_duplicate_heading_slugs() {
        let document = parse("# Title\n\n## Same\n\ntext\n\n## Same\n\nmore\n\n## Same\n");
        let slugs = heading_slugs(&document);
        assert_eq!(
            slugs.into_iter().collect::<Vec<_>>(),
            ["same", "same-1", "same-2", "title"]
        );
    }

    #[test]
    fn should_classify_every_kind_of_destination() {
        assert_eq!(classify("https://example.com"), Target::External);
        assert_eq!(classify("mailto:team@example.com"), Target::External);
        assert_eq!(classify("data:text/plain,hi"), Target::External);
        assert_eq!(classify("#decision-outcome"), Target::Anchor);
        assert_eq!(classify("/docs/adr/0001.md"), Target::RootRelative);
        assert_eq!(classify("docs/diagram.png"), Target::Relative);
        assert_eq!(classify("./docs/diagram.png"), Target::Relative);
        assert_eq!(classify("C:\\adr\\0001.md"), Target::Absolute);
        assert_eq!(classify("C:/adr/0001.md"), Target::Absolute);
        assert_eq!(classify("\\\\server\\share\\0001.md"), Target::Absolute);
        assert_eq!(classify("file:///var/log/adr.log"), Target::Absolute);
    }

    #[test]
    fn should_strip_a_query_and_a_fragment() {
        assert_eq!(strip_target("docs/a.md"), "docs/a.md");
        assert_eq!(strip_target("docs/a.md#top"), "docs/a.md");
        assert_eq!(strip_target("docs/a.md?raw=1#top"), "docs/a.md");
        assert_eq!(strip_target("/a.md?raw=1"), "/a.md");
    }
}
