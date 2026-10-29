//! Contract tests for the packaging and release plumbing.
//!
//! Everything here is a pure-text assertion over the checked-in workflows, action and downloaders,
//! so the contract is enforced without network access, a build matrix or a published release.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("could not read {}: {error}", path.display()))
}

/// Every workflow plus the composite action at the repository root: all of them run `uses:` steps.
fn action_yaml_files() -> Vec<(String, String)> {
    let workflows = repo_root().join(".github/workflows");
    let mut paths: Vec<PathBuf> = fs::read_dir(&workflows)
        .unwrap_or_else(|error| panic!("could not list {}: {error}", workflows.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| matches!(path.extension().and_then(|ext| ext.to_str()), Some("yml" | "yaml")))
        .collect();
    paths.sort();

    let mut files: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            (
                path.display().to_string(),
                fs::read_to_string(path).expect("reading a workflow"),
            )
        })
        .collect();
    files.push(("action.yml".to_owned(), read("action.yml")));
    files
}

/// The line and indentation of a `key:` whose value is a block scalar, so its body can be found.
fn block_start(line: &str, key: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix(key)?.strip_prefix(':')?.trim();
    if rest.is_empty() || rest.starts_with('|') || rest.starts_with('>') {
        return Some(line.len() - trimmed.len());
    }
    None
}

/// The bodies of every `run:` block (`run: |` scalars and inline `run: command` lines).
fn run_blocks(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if let Some(indent) = block_start(line, "run") {
            index += 1;
            let mut body = String::new();
            while index < lines.len() {
                let candidate = lines[index];
                if candidate.trim().is_empty() {
                    body.push('\n');
                    index += 1;
                    continue;
                }
                if candidate.len() - candidate.trim_start().len() <= indent {
                    break;
                }
                body.push_str(candidate);
                body.push('\n');
                index += 1;
            }
            blocks.push(body);
            continue;
        }

        if let Some(inline) = trimmed.strip_prefix("run:") {
            blocks.push(inline.to_string());
        }
        index += 1;
    }

    blocks
}

/// Every `uses:` value in a file, including `- uses:` list entries but not mentions in comments.
fn uses_entries(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start().strip_prefix("- ").unwrap_or(line.trim_start());
            trimmed
                .strip_prefix("uses:")?
                .split_whitespace()
                .next()
                .map(str::to_owned)
        })
        .collect()
}

fn is_commit_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
}

/// The block belonging to one top-level job, bounded by the next job key (indented exactly two
/// spaces; deeper indentation belongs to the job's own steps).
fn job_block<'a>(text: &'a str, job: &str) -> &'a str {
    let marker = format!("\n  {job}:");
    let start = text
        .find(&marker)
        .unwrap_or_else(|| panic!("job `{job}` is missing from publish.yaml"));
    let body = start + marker.len();
    let mut offset = body;
    for line in text[body..].split_inclusive('\n') {
        let is_job_key = line.starts_with("  ") && !line.starts_with("   ") && !line.trim().is_empty();
        if is_job_key {
            return &text[start..offset];
        }
        offset += line.len();
    }
    &text[start..]
}

fn line_number(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` does not appear in the file"))
}

#[test]
fn no_expression_is_interpolated_inside_a_run_block() {
    for (name, text) in action_yaml_files() {
        for body in run_blocks(&text) {
            assert!(
                !body.contains("${{"),
                "{name}: a `run:` block interpolates an expression; pass the value through `env:` and read \
                 the quoted shell variable instead:\n{body}"
            );
        }
    }
}

#[test]
fn every_action_reference_is_pinned_to_a_commit() {
    for (name, text) in action_yaml_files() {
        for reference in uses_entries(&text) {
            if reference.starts_with("./") {
                continue;
            }
            let (repository, revision) = reference
                .rsplit_once('@')
                .unwrap_or_else(|| panic!("{name}: `uses: {reference}` carries no ref"));
            assert!(
                !repository.is_empty(),
                "{name}: `uses: {reference}` names no repository"
            );
            assert!(
                is_commit_sha(revision),
                "{name}: `uses: {reference}` must be pinned to a full 40-character commit SHA"
            );
        }
    }
}

#[test]
fn the_action_test_pins_the_release_version() {
    let text = read(".github/workflows/action.yml");
    let line = text
        .lines()
        .find(|line| line.trim_start().starts_with("ADRLINT_ACTION_TEST_VERSION:"))
        .expect("the action test workflow declares ADRLINT_ACTION_TEST_VERSION");
    let value = line
        .split_once(':')
        .expect("a key/value pair")
        .1
        .trim()
        .trim_matches('"');
    assert_eq!(
        value, "0.1.0",
        "the action test must default to the release it exercises"
    );
}

#[test]
fn the_release_workflow_validates_the_tag_before_building() {
    let text = read(".github/workflows/publish.yaml");

    // Anchored: an unanchored pattern would accept `v1.2.3; rm -rf /` and friends.
    let pattern = "^v[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.]+)?$";
    let validation = line_number(&text, pattern);
    let first_build = line_number(&text, "cargo build --release");
    assert!(
        validation < first_build,
        "the tag must be validated before the first build"
    );

    for line in text.lines() {
        let trimmed = line.trim();
        assert!(
            !(trimmed.contains("curl") && trimmed.contains("| sh")),
            "publish.yaml pipes a downloaded script into a shell: {trimmed}"
        );
    }

    for line in text.lines() {
        if line.contains("github.com/Goldziher/homebrew-tap") {
            assert!(
                !line.contains("x-access-token"),
                "the tap clone must not embed the token in the clone URL: {line}"
            );
        }
    }
    assert!(
        text.contains("GIT_CONFIG_KEY_0=http.extraheader"),
        "the tap push must authenticate through an http.extraheader built from the environment"
    );
}

#[test]
fn every_build_and_publish_job_requires_the_verify_job() {
    let text = read(".github/workflows/publish.yaml");

    assert!(
        !text.contains("result != 'failure'"),
        "a `!= 'failure'` guard treats a skipped dependency as success"
    );

    let verify = job_block(&text, "verify");
    for command in [
        "cargo fmt --all -- --check",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test --locked",
        "cargo-deny-action",
    ] {
        assert!(verify.contains(command), "the verify job does not run `{command}`");
    }

    for job in [
        "build-macos",
        "build-windows",
        "build-linux",
        "publish_crates",
        "publish_npm",
        "publish_pypi",
        "publish_homebrew",
    ] {
        let block = job_block(&text, job);
        let needs = block
            .lines()
            .find(|line| line.trim_start().starts_with("needs:"))
            .unwrap_or_else(|| panic!("job `{job}` declares no needs"));
        assert!(
            needs.contains("verify"),
            "job `{job}` does not depend on verify: {needs}"
        );
        assert!(
            block.contains("== 'success'"),
            "job `{job}` must require an explicit success result from its dependencies"
        );
    }
}

#[test]
fn the_npm_downloader_verifies_before_it_extracts() {
    let script = read("npm-package/install.js");

    let checksum = line_number(&script, "Checksum mismatch for ${archiveName}");
    assert!(checksum < line_number(&script, "await tar.extract("));
    assert!(checksum < line_number(&script, "zip.extractEntryTo("));

    assert!(
        script.contains("const MAX_DOWNLOAD_BYTES = 100 * 1024 * 1024;"),
        "the downloader must cap the response body it buffers"
    );
    assert!(
        script.contains("received > MAX_DOWNLOAD_BYTES"),
        "the size cap must be enforced while the body streams in, not only from Content-Length"
    );

    // A cached binary is re-verified against the manifest and its install-time digest before reuse.
    let reuse = line_number(&script, "if (await cachedBinaryIsValid(");
    assert!(reuse < line_number(&script, "const archive = await httpsGet("));
    assert!(script.contains("sha256(fs.readFileSync(binaryPath)) === recordedBinary"));
}

#[test]
fn the_python_downloader_verifies_before_it_extracts() {
    let module = read("pip-package/adrlint/downloader.py");

    let docstring: String = module.lines().take(12).collect::<Vec<_>>().join("\n");
    assert!(
        docstring.contains("ADRLINT_BINARY"),
        "the module docstring must document the ADRLINT_BINARY override"
    );

    let checksum = line_number(&module, "expected = _expected_digest(version, asset_name)");
    assert!(checksum < line_number(&module, "_extract(archive_path, ext, extracted)"));
    assert!(module.contains("mode=0o700"), "the cache directory must be private");
    assert!(
        module.contains("tempfile.mkstemp(") && module.contains("os.replace(staging, destination)"),
        "cache writes must land on a fresh exclusive file and be renamed into place"
    );

    let reuse = line_number(
        &module,
        "if _cache_is_valid(binary_path, record_path, __version__, asset_name):",
    );
    assert!(reuse < line_number(&module, "archive, asset_name, ext, archive_digest = _download("));
    assert!(module.contains("_sha256_file(binary_path) == recorded_binary"));
}

#[test]
fn the_action_installer_extracts_only_the_expected_member() {
    let script = read("scripts/gh-action-install.sh");

    let checksum = line_number(&script, "actual=\"$(sha256 \"$ASSET\")\"");
    let extraction = line_number(&script, "tar -xzf \"$ASSET\" -C \"$extract_dir\" \"$bin_name\"");
    assert!(
        checksum < extraction,
        "the archive checksum gate must precede extraction"
    );

    assert!(
        !script.contains("extractall"),
        "the installer must not unpack every member of the archive"
    );
    assert!(
        !script.contains("-C \"$BIN_DIR\""),
        "the archive must be unpacked into a private directory, not straight into BIN_DIR"
    );
    assert!(
        script.contains("find \"$extract_dir\" -type f -name \"$bin_name\""),
        "the installer must locate the expected member after extraction"
    );
}

/// Guard: the helpers above must actually find the constructs they assert about.
#[test]
fn the_text_parsers_find_the_constructs_they_assert_about() {
    let text = read(".github/workflows/publish.yaml");
    assert!(!run_blocks(&text).is_empty());
    assert!(uses_entries(&text).contains(&"actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1".to_owned()));
    assert!(job_block(&text, "meta").contains("Validate the release tag"));
    assert!(Path::new(&repo_root().join("action.yml")).is_file());
}
