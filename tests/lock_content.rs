use std::path::Path;

use adrlint::adr::parse;
use adrlint::lock::{self, LockEntry, LockFile};

fn hash(source: &str) -> String {
    lock::canonical_hash(&parse(source))
}

#[test]
fn should_hash_list_marker_variants_equally() {
    assert_eq!(hash("* one\n* two\n"), hash("- one\n- two\n"));
    assert_eq!(hash("* one\n  * nested\n"), hash("- one\n  - nested\n"));
}

#[test]
fn should_hash_crlf_and_lf_equally() {
    let lf = "---\nstatus: accepted\ntitle: Use Rust\n---\n\n# Title\n\nBody with a\n\n```rust\nfn main() {}\n```\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(hash(lf), hash(&crlf));
}

#[test]
fn should_hash_a_soft_wrap_and_one_line_equally() {
    assert_eq!(hash("one two\nthree four\n"), hash("one two three four\n"));
}

#[test]
fn should_hash_atx_and_setext_headings_equally() {
    assert_eq!(
        hash("# Title\n\n## Section\n\nBody\n"),
        hash("Title\n=====\n\nSection\n-------\n\nBody\n")
    );
}

#[test]
fn should_hash_reordered_front_matter_keys_equally() {
    assert_eq!(
        hash("---\nstatus: accepted\ndate: 2024-01-02\ndeciders: [a, b]\n---\n# Title\n"),
        hash("---\ndeciders: [a, b]\ndate: 2024-01-02\nstatus: accepted\n---\n# Title\n")
    );
}

#[test]
fn should_change_the_hash_when_one_word_changes() {
    assert_ne!(hash("# Title\n\nWe use Rust.\n"), hash("# Title\n\nWe use Go.\n"));
}

#[test]
fn should_change_the_hash_when_a_link_destination_changes() {
    assert_ne!(
        hash("See [the docs](https://example.com/a).\n"),
        hash("See [the docs](https://example.com/b).\n")
    );
}

#[test]
fn should_change_the_hash_when_an_image_destination_changes() {
    assert_ne!(hash("![diagram](docs/a.png)\n"), hash("![diagram](docs/b.png)\n"));
}

#[test]
fn should_change_the_hash_when_a_code_block_changes() {
    assert_ne!(hash("```rust\nlet a = 1;\n```\n"), hash("```rust\nlet a = 2;\n```\n"));
}

#[test]
fn should_change_the_hash_when_the_front_matter_status_changes() {
    assert_ne!(
        hash("---\nstatus: proposed\n---\n# Title\n"),
        hash("---\nstatus: accepted\n---\n# Title\n")
    );
}

#[test]
fn should_round_trip_a_sorted_lock_file() {
    let directory = tempfile::TempDir::new().unwrap();
    let path = directory.path().join("adrlint.lock");

    let mut lock = LockFile::default();
    lock.insert("docs/adr/0002-two.md", LockEntry::new(2, "rejected", "b"));
    lock.insert("docs/adr/0001-one.md", LockEntry::new(1, "accepted", "a"));

    lock::save(&path, &lock).unwrap();
    let rendered = std::fs::read_to_string(&path).unwrap();

    let first = rendered.find("0001-one.md").expect("the 0001 entry");
    let second = rendered.find("0002-two.md").expect("the 0002 entry");
    assert!(first < second, "entries are sorted by path:\n{rendered}");
    assert!(rendered.ends_with('\n'), "the file ends in a newline");
    assert_eq!(
        rendered,
        "version = 1\ncanonical = 1\n\n[adrs.\"docs/adr/0001-one.md\"]\nnumber = 1\nstatus = \"accepted\"\nblake3 = \"a\"\n\n[adrs.\"docs/adr/0002-two.md\"]\nnumber = 2\nstatus = \"rejected\"\nblake3 = \"b\"\n"
    );

    let loaded = lock::load(&path).unwrap();
    assert_eq!(loaded, lock);
    assert!(loaded.is_current());
    assert_eq!(lock::render(&loaded), rendered, "saving twice is idempotent");
}

#[test]
fn should_treat_a_missing_lock_file_as_empty() {
    let directory = tempfile::TempDir::new().unwrap();
    let loaded = lock::load(&directory.path().join("absent.lock")).unwrap();
    assert_eq!(loaded, LockFile::default());
    assert!(loaded.adrs.is_empty());
}

#[test]
fn should_report_adr50_when_the_locked_hash_differs() {
    let document = parse("---\nstatus: accepted\n---\n# Title\n\nBody\n");
    let entry = LockEntry::new(1, "accepted", "not-the-hash");

    let diagnostic = lock::verify(&document, Some(&entry), Path::new("docs/adr/0001-record.md"))
        .expect("a changed document is ADR50");
    assert_eq!(diagnostic.code, "ADR50");
    assert!(diagnostic.severity.is_error());
    assert_eq!(diagnostic.path, Path::new("docs/adr/0001-record.md"));
    assert!(diagnostic.message.contains("not-the-hash"));

    let matching = LockEntry::new(1, "accepted", lock::canonical_hash(&document));
    assert!(lock::verify(&document, Some(&matching), Path::new("docs/adr/0001-record.md")).is_none());
    assert!(lock::verify(&document, None, Path::new("docs/adr/0002-new.md")).is_none());
}

#[test]
fn should_report_adr51_for_a_missing_locked_file() {
    let entry = LockEntry::new(3, "accepted", "abc");
    let diagnostic = lock::missing(Path::new("docs/adr/0003-gone.md"), &entry);
    assert_eq!(diagnostic.code, "ADR51");
    assert!(diagnostic.severity.is_error());
}

#[test]
fn should_report_adr51_only_for_locked_files_missing_from_disk() {
    let directory = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(directory.path().join("docs/adr")).unwrap();
    std::fs::write(directory.path().join("docs/adr/0001-one.md"), "# One\n").unwrap();

    let mut lock = LockFile::default();
    lock.insert("docs/adr/0001-one.md", LockEntry::new(1, "accepted", "a"));
    lock.insert("docs/adr/0002-two.md", LockEntry::new(2, "accepted", "b"));

    let diagnostics = lock::missing_entries(&lock, directory.path());
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "ADR51");
    assert!(diagnostics[0].message.contains("0002"));
}

#[test]
fn should_read_the_status_and_lock_an_entry() {
    let document = parse("---\nstatus: rejected\n---\n# Title\n");
    assert_eq!(lock::status(&document), "rejected");
    assert_eq!(lock::status(&parse("# Title\n")), "");

    let entry = lock::entry(&document, 4);
    assert_eq!(entry.number, 4);
    assert_eq!(entry.status, "rejected");
    assert_eq!(entry.blake3, lock::canonical_hash(&document));
}

#[test]
fn should_remove_an_entry_on_unlock() {
    let mut lock = LockFile::default();
    lock.insert("docs/adr/0001-one.md", LockEntry::new(1, "accepted", "a"));
    assert!(lock::unlock(&mut lock, "docs/adr/0001-one.md"));
    assert!(!lock::unlock(&mut lock, "docs/adr/0001-one.md"));
    assert!(lock.adrs.is_empty());
}
