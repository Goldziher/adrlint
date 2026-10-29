//! The immutability manifest: one canonical content hash per locked ADR.
//!
//! The hash is computed over a deterministic serialization of the parsed [`Document`], never over
//! the bytes on disk and never over a formatted rendering. Formatting choices — list markers, ATX
//! versus setext headings, emphasis markers, link style, line wrapping, blank-line runs and CRLF
//! line endings — are excluded, so `adrlint format` never breaks a lock. Content is included, so a
//! changed word, a changed link target or a changed front-matter value does.
//!
//! [`CANONICAL_VERSION`] versions the serializer itself. The lock records the version that produced
//! it, so a serializer change is reported as a stale lock that asks for `adrlint lock` rather than as
//! an edit to every locked ADR.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use saphyr::{ScalarOwned, Tag, YamlOwned};
use serde::{Deserialize, Serialize};

use crate::adr::Document;
use crate::diagnostics::Diagnostic;

pub use crate::adr::parse;

/// Version of the canonical serialization. Bumping it invalidates every stored hash.
pub const CANONICAL_VERSION: u32 = 1;

/// Version of the lock file wire format.
pub const LOCK_VERSION: u32 = 1;

/// A locked ADR's canonical hash differs from the manifest.
pub const CONTENT_CHANGED: &str = "ADR50";

/// A locked ADR is missing from disk.
pub const MISSING: &str = "ADR51";

/// Prefix of the canonical byte stream, so a hash cannot be confused with another tool's.
const PREFIX: &str = "adrlint.canonical";

/// `adrlint.lock`: a sorted manifest keyed by repository-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockFile {
    pub version: u32,
    pub canonical: u32,
    #[serde(default)]
    pub adrs: BTreeMap<String, LockEntry>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            version: LOCK_VERSION,
            canonical: CANONICAL_VERSION,
            adrs: BTreeMap::new(),
        }
    }
}

impl LockFile {
    /// Whether the serializer version that produced this lock is the current one.
    pub fn is_current(&self) -> bool {
        self.canonical == CANONICAL_VERSION
    }

    /// The locked entry for a manifest key, if any.
    pub fn entry(&self, key: &str) -> Option<&LockEntry> {
        self.adrs.get(key)
    }

    /// Inserts or replaces an entry. [`save`] sorts the result.
    pub fn insert(&mut self, key: impl Into<String>, entry: LockEntry) {
        self.adrs.insert(key.into(), entry);
    }

    /// Removes an entry, returning whether it existed.
    pub fn remove(&mut self, key: &str) -> bool {
        self.adrs.remove(key).is_some()
    }
}

/// One locked ADR: its number, its status at lock time and its canonical hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockEntry {
    pub number: u32,
    pub status: String,
    pub blake3: String,
}

impl LockEntry {
    pub fn new(number: u32, status: impl Into<String>, blake3: impl Into<String>) -> Self {
        Self {
            number,
            status: status.into(),
            blake3: blake3.into(),
        }
    }
}

/// Builds the entry to lock for `document`, reading its number and status.
pub fn entry(document: &Document, number: u32) -> LockEntry {
    LockEntry::new(number, status(document), canonical_hash(document))
}

/// The manifest key for `path`: repository-relative, with forward slashes.
///
/// A path already relative to the project root is returned unchanged, so `key` is idempotent. A
/// path outside the root keeps its own components rather than silently shortening to a basename.
pub fn key(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::RootDir => parts.push(String::new()),
            Component::CurDir => {}
            Component::ParentDir => parts.push("..".to_owned()),
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
            Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_string_lossy().into_owned()),
        }
    }
    parts.join("/")
}

/// Reads a lock file. A missing file is an empty lock, because presence is opt-in.
///
/// Manifest keys are validated before use: every key must stay inside the project root, so a
/// hand-written entry such as `/etc/passwd` cannot be used to probe for files outside it.
pub fn load(path: &Path) -> Result<LockFile> {
    match fs::read_to_string(path) {
        Ok(source) => {
            let lock: LockFile =
                toml::from_str(&source).with_context(|| format!("parsing the lock file at {}", path.display()))?;
            validate_keys(&lock).with_context(|| format!("invalid lock file at {}", path.display()))?;
            Ok(lock)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(LockFile::default()),
        Err(error) => Err(error).with_context(|| format!("reading the lock file at {}", path.display())),
    }
}

fn validate_keys(lock: &LockFile) -> Result<()> {
    for key in lock.adrs.keys() {
        validate_key(key)?;
    }
    Ok(())
}

/// Rejects a manifest key that is empty, absolute, traverses upward or carries a Windows prefix.
fn validate_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!("lock file key must not be empty");
    }
    if key.starts_with('/') || key.starts_with('\\') {
        bail!("lock file key {key:?} must be relative to the project root");
    }
    let mut characters = key.chars();
    if let (Some(first), Some(':')) = (characters.next(), characters.next())
        && first.is_ascii_alphabetic()
    {
        bail!("lock file key {key:?} must not start with a drive prefix");
    }
    let path = Path::new(key);
    if path.is_absolute() {
        bail!("lock file key {key:?} must be relative to the project root");
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        bail!("lock file key {key:?} must not escape the project root");
    }
    if key.split(['/', '\\']).any(|part| part == "..") {
        bail!("lock file key {key:?} must not escape the project root");
    }
    Ok(())
}

/// Writes a lock file as sorted, stable TOML with a single trailing newline.
///
/// The write is atomic and refused when `path` is a symlink, so the manifest cannot be used to
/// rewrite a file elsewhere on disk.
pub fn save(path: &Path, lock: &LockFile) -> Result<()> {
    if is_symlink(path)? {
        bail!(
            "refusing to write the lock file through the symlink at {}",
            path.display()
        );
    }
    let rendered = render(lock).with_context(|| format!("rendering the lock file at {}", path.display()))?;
    write_atomic(path, rendered.as_bytes()).with_context(|| format!("writing the lock file at {}", path.display()))
}

fn is_symlink(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_symlink()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to a temporary file beside `path`, then renames it into place.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("adrlint.lock");
    let temporary = parent.join(format!(".{name}.{}.{counter}.tmp", std::process::id()));

    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .with_context(|| format!("creating {}", temporary.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("writing {}", temporary.display()))?;
        file.flush()
            .with_context(|| format!("flushing {}", temporary.display()))?;
        fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// Renders a lock file. Entries are sorted by their [`BTreeMap`] key and the output ends in exactly
/// one newline, so a re-lock that changes nothing produces an identical file.
pub fn render(lock: &LockFile) -> Result<String> {
    let mut rendered = toml::to_string_pretty(lock).context("serializing the lock file")?;
    while rendered.ends_with('\n') {
        rendered.pop();
    }
    rendered.push('\n');
    Ok(rendered)
}

/// Removes `key` from the manifest, returning whether an entry existed.
pub fn unlock(lock: &mut LockFile, key: &str) -> bool {
    lock.remove(key)
}

/// The ADR status from the parsed front matter, or an empty string when absent.
pub fn status(document: &Document) -> String {
    document
        .front_matter
        .as_ref()
        .and_then(|front_matter| front_matter.value.as_ref())
        .and_then(|value| value.as_mapping_get("status"))
        .map(status_text)
        .unwrap_or_default()
}

/// Compares a document with its optional manifest entry.
///
/// Returns `ADR50` when the entry exists and the canonical hash differs. An unlocked document
/// produces no finding: a new ADR is not a violation until it is locked.
pub fn verify(document: &Document, entry: Option<&LockEntry>, path: &Path) -> Option<Diagnostic> {
    let entry = entry?;
    let actual = canonical_hash(document);
    if actual == entry.blake3 {
        return None;
    }
    Some(Diagnostic::error(
        CONTENT_CHANGED,
        path,
        1,
        format!(
            "locked ADR content changed since it was locked (locked {}, now {}); unlock it with a reason before editing",
            entry.blake3, actual
        ),
    ))
}

/// The `ADR51` findings for every locked path that is no longer on disk, in manifest order.
///
/// `root` is the project root the manifest keys are relative to.
pub fn missing_entries(lock: &LockFile, root: &Path) -> Vec<Diagnostic> {
    lock.adrs
        .iter()
        .filter(|(key, _)| !root.join(key).exists())
        .map(|(key, entry)| missing(Path::new(key), entry))
        .collect()
}

/// The `ADR51` finding for a locked path that is no longer on disk.
pub fn missing(path: &Path, entry: &LockEntry) -> Diagnostic {
    Diagnostic::error(
        MISSING,
        path,
        1,
        format!("locked ADR {:04} ({}) is missing from disk", entry.number, entry.status),
    )
}

/// The lowercase hexadecimal BLAKE3 hash of the document's canonical serialization.
///
/// Stable across runs and across CRLF and LF input.
pub fn canonical_hash(document: &Document) -> String {
    let mut writer = Canonical::new();
    write_document(&mut writer, document);
    blake3::hash(writer.out.as_bytes()).to_hex().to_string()
}

/// A deterministic, length-prefixed byte stream.
///
/// Every variable-length field is written as `name:<byte length>:<bytes>`, so adjacent fields can
/// never run into each other. Numbers run to the end of their line, which the name delimits.
struct Canonical {
    out: String,
}

impl Canonical {
    fn new() -> Self {
        Self {
            out: format!("{PREFIX}:{CANONICAL_VERSION}\n"),
        }
    }

    fn empty() -> Self {
        Self { out: String::new() }
    }

    fn tag(&mut self, name: &str) {
        self.out.push_str(name);
        self.out.push('\n');
    }

    fn text(&mut self, name: &str, value: &str) {
        let value = normalise(value);
        self.out.push_str(name);
        self.out.push(':');
        self.out.push_str(&value.len().to_string());
        self.out.push(':');
        self.out.push_str(&value);
        self.out.push('\n');
    }

    fn number(&mut self, name: &str, value: u64) {
        self.out.push_str(name);
        self.out.push(':');
        self.out.push_str(&value.to_string());
        self.out.push('\n');
    }

    fn signed(&mut self, name: &str, value: i64) {
        self.out.push_str(name);
        self.out.push(':');
        self.out.push_str(&value.to_string());
        self.out.push('\n');
    }

    fn flag(&mut self, name: &str, value: bool) {
        self.number(name, u64::from(value));
    }

    fn bits(&mut self, name: &str, value: u64) {
        self.out.push_str(name);
        self.out.push(':');
        self.out.push_str(&format!("{value:016x}"));
        self.out.push('\n');
    }
}

/// Normalises line endings so a CRLF checkout and an LF checkout hash the same.
fn normalise(value: &str) -> Cow<'_, str> {
    if value.contains('\r') {
        Cow::Owned(value.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(value)
    }
}

fn write_document(writer: &mut Canonical, document: &Document) {
    write_front_matter(writer, document.front_matter.as_ref());
    write_headings(writer, document);
    write_links(writer, document);
    write_list_items(writer, document);
    write_blocks(writer, document);
}

fn write_front_matter(writer: &mut Canonical, front_matter: Option<&crate::adr::FrontMatter>) {
    let Some(front_matter) = front_matter else {
        writer.tag("front-matter:none");
        return;
    };
    let Some(value) = front_matter.value.as_ref() else {
        writer.tag("front-matter:raw");
        writer.text("raw", &normalise_raw(&front_matter.raw));
        return;
    };
    writer.tag("front-matter:yaml");
    write_yaml(writer, value);
}

/// The fallback for invalid YAML: trailing whitespace and blank-line runs are formatting, so they
/// are dropped, but any real edit still changes the hash.
fn normalise_raw(raw: &str) -> String {
    raw.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_matches('\n')
        .to_owned()
}

fn write_yaml(writer: &mut Canonical, value: &YamlOwned) {
    match value {
        YamlOwned::Value(scalar) => write_scalar(writer, scalar),
        YamlOwned::Representation(text, _, tag) => {
            writer.tag("yaml-representation");
            if let Some(tag) = tag {
                writer.text("tag", &tag_text(tag));
            }
            writer.text("value", text);
        }
        YamlOwned::Sequence(items) => {
            writer.tag("yaml-sequence");
            writer.number("len", items.len() as u64);
            for item in items {
                write_yaml(writer, item);
            }
        }
        YamlOwned::Mapping(mapping) => write_mapping(writer, mapping),
        YamlOwned::Tagged(tag, inner) => {
            writer.tag("yaml-tagged");
            writer.text("tag", &tag_text(tag));
            write_yaml(writer, inner);
        }
        YamlOwned::Alias(index) => {
            writer.tag("yaml-alias");
            writer.number("index", *index as u64);
        }
        YamlOwned::BadValue => writer.tag("yaml-bad-value"),
    }
}

/// Front matter is a mapping in practice, but a stray sequence or scalar is still content.
fn write_mapping(writer: &mut Canonical, mapping: &saphyr::MappingOwned) {
    writer.tag("yaml-mapping");
    writer.number("len", mapping.len() as u64);

    let mut entries = mapping
        .iter()
        .map(|(key, value)| (canonical_of_yaml(key), canonical_of_yaml(value)))
        .collect::<Vec<_>>();
    entries.sort();
    for (key, value) in &entries {
        writer.text("key", key);
        writer.text("value", value);
    }
}

fn write_scalar(writer: &mut Canonical, scalar: &ScalarOwned) {
    match scalar {
        ScalarOwned::Null => writer.tag("yaml-null"),
        ScalarOwned::Boolean(value) => writer.flag("yaml-bool", *value),
        ScalarOwned::Integer(value) => writer.signed("yaml-int", *value),
        // A float is serialized as its bit pattern: no locale, no rounding, no debug output.
        ScalarOwned::FloatingPoint(value) => writer.bits("yaml-float-bits", value.0.to_bits()),
        ScalarOwned::String(value) => writer.text("yaml-string", value),
    }
}

fn canonical_of_yaml(value: &YamlOwned) -> String {
    let mut writer = Canonical::empty();
    write_yaml(&mut writer, value);
    writer.out
}

fn tag_text(tag: &Tag) -> String {
    format!("{}{}", tag.handle, tag.suffix)
}

fn status_text(value: &YamlOwned) -> String {
    match value {
        YamlOwned::Value(ScalarOwned::String(text)) => text.clone(),
        YamlOwned::Value(ScalarOwned::Boolean(value)) => value.to_string(),
        YamlOwned::Value(ScalarOwned::Integer(value)) => value.to_string(),
        YamlOwned::Value(ScalarOwned::FloatingPoint(value)) => value.0.to_string(),
        YamlOwned::Representation(text, _, _) => text.clone(),
        _ => String::new(),
    }
}

fn write_headings(writer: &mut Canonical, document: &Document) {
    writer.number("heading-count", document.headings.len() as u64);
    for heading in &document.headings {
        writer.tag("heading");
        writer.number("level", u64::from(heading.level));
        writer.text("text", &heading.text);
    }
}

fn write_links(writer: &mut Canonical, document: &Document) {
    writer.number("link-count", document.links.len() as u64);
    for link in &document.links {
        writer.tag("link");
        writer.text(
            "kind",
            match link.kind {
                crate::adr::LinkKind::Link => "link",
                crate::adr::LinkKind::Image => "image",
            },
        );
        writer.text("destination", &link.url);
        writer.text("title", &link.title);
        writer.text("label", &link.text);
    }
}

fn write_list_items(writer: &mut Canonical, document: &Document) {
    writer.number("item-count", document.list_items.len() as u64);
    for item in &document.list_items {
        writer.tag("list-item");
        writer.flag("ordered", item.ordered);
        writer.number("depth", item.depth as u64);
        writer.text("text", &item.text);
    }
}

fn write_blocks(writer: &mut Canonical, document: &Document) {
    writer.number("block-count", document.blocks.len() as u64);
    for block in &document.blocks {
        write_block(writer, block);
    }
}

fn write_block(writer: &mut Canonical, block: &crate::adr::Block) {
    use crate::adr::Block;
    match block {
        Block::Paragraph { text, .. } => writer.text("paragraph", text),
        Block::List { ordered, depth, .. } => {
            writer.tag("list");
            writer.flag("ordered", *ordered);
            writer.number("depth", *depth as u64);
        }
        Block::CodeBlock { text, .. } => {
            // The info string is a rendering hint, like a fence style: rumdl's MD040 may add it, and
            // that must not turn a formatter run into a content edit. The body is content.
            writer.tag("code-block");
            writer.text("code", text);
        }
        Block::Table { header, rows, .. } => {
            writer.tag("table");
            writer.number("columns", header.len() as u64);
            for cell in header {
                writer.text("header-cell", cell);
            }
            writer.number("rows", rows.len() as u64);
            for row in rows {
                writer.number("cells", row.len() as u64);
                for cell in row {
                    writer.text("cell", cell);
                }
            }
        }
        Block::Html { text, .. } => writer.text("html", text),
        Block::Rule { .. } => writer.tag("rule"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(source: &str) -> String {
        canonical_hash(&parse(source))
    }

    fn single_entry() -> LockFile {
        let mut lock = LockFile::default();
        lock.insert(
            "docs/adr/0001-record.md",
            LockEntry::new(1, "accepted", hash("# Title\n\nBody\n")),
        );
        lock
    }

    #[test]
    fn should_hash_a_document_stably() {
        let source = "# Title\n\n## Context\n\nBody with [a link](https://example.com).\n";
        assert_eq!(hash(source), hash(source));
        assert_eq!(hash(source).len(), 64);
        assert!(
            hash(source)
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
            "the hash is lowercase hex"
        );
    }

    #[test]
    fn should_ignore_list_and_emphasis_markers() {
        assert_eq!(hash("* one\n* two\n"), hash("- one\n- two\n"));
        assert_eq!(hash("**bold**\n"), hash("__bold__\n"));
    }

    #[test]
    fn should_ignore_an_ordered_list_start_number() {
        assert_eq!(hash("1. one\n2. two\n"), hash("3. one\n4. two\n"));
    }

    #[test]
    fn should_keep_a_hard_break() {
        assert_ne!(hash("one  \ntwo\n"), hash("one two\n"));
    }

    #[test]
    fn should_hash_typed_front_matter_values() {
        assert_ne!(hash("---\ncount: 1\n---\n# T\n"), hash("---\ncount: \"1\"\n---\n# T\n"));
        assert_ne!(
            hash("---\nflag: true\n---\n# T\n"),
            hash("---\nflag: false\n---\n# T\n")
        );
    }

    #[test]
    fn should_ignore_front_matter_key_order() {
        assert_eq!(
            hash("---\nstatus: accepted\ndate: 2024-01-02\n---\n# T\n"),
            hash("---\ndate: 2024-01-02\nstatus: accepted\n---\n# T\n")
        );
    }

    #[test]
    fn should_normalise_invalid_front_matter() {
        assert_eq!(
            hash("---\ntitle: ok\n  bad: indent  \n---\n# T\n"),
            hash("---\ntitle: ok\n  bad: indent\n---\n# T\n")
        );
        assert_ne!(
            hash("---\ntitle: ok\n  bad: indent\n---\n# T\n"),
            hash("---\ntitle: ok\n  worse: indent\n---\n# T\n")
        );
    }

    #[test]
    fn should_ignore_a_code_block_language() {
        assert_eq!(hash("```rust\nlet a = 1;\n```\n"), hash("```text\nlet a = 1;\n```\n"));
    }

    #[test]
    fn should_ignore_table_alignment_and_fence_style() {
        assert_eq!(
            hash("| a | b |\n| --- | --- |\n| 1 | 2 |\n"),
            hash("| a | b |\n|:---:| ---:|\n| 1 | 2 |\n")
        );
        assert_eq!(hash("```rust\nlet a = 1;\n```\n"), hash("~~~rust\nlet a = 1;\n~~~\n"));
    }

    #[test]
    fn should_read_the_front_matter_status() {
        assert_eq!(status(&parse("---\nstatus: accepted\n---\n# T\n")), "accepted");
        assert_eq!(status(&parse("# T\n")), "");
        assert_eq!(status(&parse("---\ntitle: no status\n---\n# T\n")), "");
    }

    #[test]
    fn should_build_an_entry_from_a_document() {
        let document = parse("---\nstatus: rejected\n---\n# T\n");
        let entry = entry(&document, 7);
        assert_eq!(entry.number, 7);
        assert_eq!(entry.status, "rejected");
        assert_eq!(entry.blake3, canonical_hash(&document));
    }

    #[test]
    fn should_remove_an_entry_only_once() {
        let mut lock = single_entry();
        assert!(unlock(&mut lock, "docs/adr/0001-record.md"));
        assert!(!unlock(&mut lock, "docs/adr/0001-record.md"));
        assert!(lock.adrs.is_empty());
    }

    #[test]
    fn should_report_a_missing_locked_file() {
        let entry = LockEntry::new(1, "accepted", "abc");
        let diagnostic = missing(Path::new("docs/adr/0001-record.md"), &entry);
        assert_eq!(diagnostic.code, MISSING);
        assert!(diagnostic.message.contains("0001"));
    }

    #[test]
    fn should_detect_locked_files_that_are_gone() {
        let directory = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(directory.path().join("docs/adr")).unwrap();
        std::fs::write(directory.path().join("docs/adr/0001-record.md"), "# T\n").unwrap();

        let mut lock = LockFile::default();
        lock.insert("docs/adr/0001-record.md", LockEntry::new(1, "accepted", "a"));
        lock.insert("docs/adr/0002-gone.md", LockEntry::new(2, "accepted", "b"));

        let diagnostics = missing_entries(&lock, directory.path());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, MISSING);
        assert!(diagnostics[0].message.contains("0002"));
    }

    #[test]
    fn should_key_paths_relative_to_the_root() {
        assert_eq!(
            key(Path::new("/repo"), Path::new("/repo/docs/adr/0001-record.md")),
            "docs/adr/0001-record.md"
        );
        assert_eq!(
            key(Path::new("/repo"), Path::new("docs/adr/0001-record.md")),
            "docs/adr/0001-record.md"
        );
        assert_eq!(
            key(Path::new("/repo"), Path::new("/other/0001-record.md")),
            "/other/0001-record.md"
        );
    }

    #[test]
    fn should_detect_that_a_lock_is_stale() {
        let mut lock = single_entry();
        assert!(lock.is_current());
        lock.canonical = CANONICAL_VERSION + 1;
        assert!(!lock.is_current());
    }

    #[test]
    fn should_render_sorted_keys_and_exactly_one_trailing_newline() {
        let mut lock = LockFile::default();
        lock.insert("docs/adr/0002-second.md", LockEntry::new(2, "accepted", "b"));
        lock.insert("docs/adr/0001-first.md", LockEntry::new(1, "accepted", "a"));

        let rendered = render(&lock).expect("a lock file serializes");
        assert!(rendered.ends_with('\n'));
        assert!(!rendered.ends_with("\n\n"), "exactly one trailing newline");
        let first = rendered.find("0001-first").expect("first entry");
        let second = rendered.find("0002-second").expect("second entry");
        assert!(first < second, "entries are sorted by path");

        let parsed: LockFile = toml::from_str(&rendered).expect("the rendered lock parses");
        assert_eq!(parsed, lock);
    }

    #[test]
    fn should_reject_an_absolute_manifest_key() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("adrlint.lock");
        fs::write(
            &path,
            "version = 1\ncanonical = 1\n\n[adrs.\"/etc/passwd\"]\nnumber = 1\nstatus = \"accepted\"\nblake3 = \"x\"\n",
        )
        .unwrap();

        let error = format!("{:#}", load(&path).unwrap_err());
        assert!(error.contains("/etc/passwd"), "{error}");
    }

    #[test]
    fn should_reject_a_manifest_key_that_traverses_upward() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("adrlint.lock");
        fs::write(
            &path,
            "version = 1\ncanonical = 1\n\n[adrs.\"../outside.md\"]\nnumber = 1\nstatus = \"accepted\"\nblake3 = \"x\"\n",
        )
        .unwrap();

        let error = format!("{:#}", load(&path).unwrap_err());
        assert!(error.contains("../outside.md"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn should_refuse_to_save_through_a_symlink() {
        let directory = tempfile::TempDir::new().unwrap();
        let target = directory.path().join("outside.toml");
        fs::write(&target, "untouched\n").unwrap();
        let link = directory.path().join("adrlint.lock");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let error = save(&link, &single_entry()).unwrap_err().to_string();
        assert!(error.contains("symlink"), "{error}");
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched\n");
    }
}
