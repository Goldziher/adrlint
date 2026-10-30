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
        value, "0.4.0",
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
        "publish_scoop",
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

/// Every file that maps a platform onto a release archive: the workflows, the composite action at
/// the repository root, and the three installers that download the archive.
fn platform_maps() -> Vec<(String, String)> {
    let mut files = action_yaml_files();
    for relative in [
        "npm-package/install.js",
        "pip-package/adrlint/downloader.py",
        "scripts/gh-action-resolve.sh",
        "scripts/gh-action-install.sh",
        "scripts/gh-action-run.sh",
    ] {
        files.push((relative.to_owned(), read(relative)));
    }
    files
}

/// The Windows archive is built natively with MSVC because rumdl's `tikv-jemallocator` dependency
/// is dropped on msvc and `tikv-jemalloc-sys` cannot link for `x86_64-pc-windows-gnu`. A leftover
/// `windows-gnu` reference would publish a triple no installer requests, or request one no build
/// publishes.
#[test]
fn no_release_surface_still_names_the_windows_gnu_target() {
    for (name, text) in platform_maps() {
        for needle in ["windows-gnu", "windows_gnu"] {
            assert!(
                !text.contains(needle),
                "{name} still references `{needle}`; the Windows archive is `x86_64-pc-windows-msvc`"
            );
        }
    }
}

#[test]
fn the_windows_archive_is_built_natively_with_msvc() {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, "build-windows");

    assert!(
        block.contains("runs-on: windows-latest"),
        "the Windows archive must be built on a Windows runner, not cross-compiled"
    );
    assert!(
        block.contains("TRIPLE: x86_64-pc-windows-msvc"),
        "build-windows must target x86_64-pc-windows-msvc"
    );
    assert!(
        block.contains("targets: x86_64-pc-windows-msvc"),
        "the Windows job must install the msvc target"
    );
    assert!(
        block.contains("cargo build --release --bin adrlint --target $env:TRIPLE"),
        "build-windows must build the binary for its target"
    );

    // The mingw cross-build is what failed on 0.1.0; none of its toolchain setup may return. The
    // job's own comment explains why msvc is required, so only executable lines are checked.
    let commands: String = block
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for stale in ["mingw", "x86_64-w64-mingw32", "apt-get"] {
        assert!(
            !commands.contains(stale),
            "build-windows still runs `{stale}`-related setup; the job builds natively with MSVC"
        );
    }

    assert!(
        block.contains("Compress-Archive"),
        "the Windows archive must be packaged with Compress-Archive"
    );
    assert!(
        block.contains(r#""target/$env:TRIPLE/release/adrlint.exe""#),
        "Compress-Archive must package the produced adrlint.exe"
    );
    assert!(
        block.contains(r#""adrlint-$env:TRIPLE.zip""#),
        "the archive must be named adrlint-<triple>.zip"
    );
    assert!(
        block.contains("gh release upload"),
        "build-windows must upload its archive to the release"
    );
}

#[test]
fn every_installer_requests_the_msvc_windows_archive() {
    let npm = read("npm-package/install.js");
    assert!(
        npm.contains(r#""Windows_NT:x64": "x86_64-pc-windows-msvc","#),
        "the npm wrapper must resolve Windows x64 to the msvc archive"
    );
    assert!(
        npm.contains(r#"const archiveName = `adrlint-${triple}.${isWindows ? "zip" : "tar.gz"}`;"#),
        "the npm wrapper must still request a .zip on Windows"
    );

    let python = read("pip-package/adrlint/downloader.py");
    assert!(
        python.contains(r#"return "x86_64-pc-windows-msvc""#),
        "the pip wrapper must resolve Windows amd64 to the msvc archive"
    );
    assert!(
        python.contains(r#"ext = "zip" if "windows" in triple else "tar.gz""#),
        "the pip wrapper must still request a .zip on Windows"
    );

    let resolve = read("scripts/gh-action-resolve.sh");
    assert!(
        resolve.contains(r#"X64) triple="x86_64-pc-windows-msvc" ;;"#),
        "the GitHub Action must resolve Windows X64 to the msvc archive"
    );

    let install = read("scripts/gh-action-install.sh");
    assert!(
        install.contains(r#"bin_name="adrlint.exe""#),
        "the GitHub Action must publish adrlint.exe on Windows"
    );
    assert!(
        install.contains("zip)"),
        "the GitHub Action installer must still handle the .zip archive"
    );
}

#[test]
fn the_release_asset_lists_name_the_msvc_archive() {
    let text = read(".github/workflows/publish.yaml");
    let archive = "adrlint-x86_64-pc-windows-msvc.zip";

    for job in ["meta", "checksums", "finalize_release"] {
        assert!(
            job_block(&text, job).contains(archive),
            "job `{job}` does not expect {archive} on the release"
        );
    }

    assert!(
        job_block(&text, "verify-binstall").contains(r#"{"os": "windows-latest", "target": "x86_64-pc-windows-msvc"}"#),
        "the binstall smoke test must install the msvc archive"
    );
}

#[test]
fn the_scoop_manifest_matches_the_published_assets() {
    let script = read("scripts/update-scoop-manifest.sh");

    assert!(script.starts_with("#!/usr/bin/env bash"));
    assert!(
        script.contains("set -euo pipefail"),
        "the manifest generator must fail on any error"
    );

    assert!(
        script.contains(r#"TRIPLE="x86_64-pc-windows-msvc""#),
        "the Scoop manifest must ship the msvc archive"
    );
    assert!(
        script.contains(r#"ARCHIVE="adrlint-${TRIPLE}.zip""#),
        "the Scoop manifest must point at the release zip"
    );
    assert!(
        script.contains(r#"BASE="https://github.com/Goldziher/adrlint/releases/download/v${VERSION}""#),
        "the manifest URL must be the release download URL for the version"
    );

    // The pinned URL and hash describe the asset the release workflow uploads.
    assert!(
        script.contains(r#""url": "${BASE}/${ARCHIVE}""#),
        "architecture.64bit.url must be the release archive"
    );
    assert!(
        script.contains(r#""hash": "${HASH}""#),
        "architecture.64bit.hash must come from the checksums file"
    );
    assert!(
        script.contains("if (n == f) print $1"),
        "the hash must be looked up by archive name in the checksums file"
    );

    for field in [
        r#""bin": "adrlint.exe","#,
        r#""license": "MIT","#,
        r#""homepage": "https://github.com/Goldziher/adrlint","#,
    ] {
        assert!(script.contains(field), "the manifest is missing {field}");
    }

    assert!(
        script.contains(r#""github": "https://github.com/Goldziher/adrlint""#),
        "checkver must follow the GitHub repository"
    );
    assert!(
        script.contains(r#"releases/download/v\$version/${ARCHIVE}"#),
        "autoupdate.architecture.64bit.url must substitute Scoop's $version placeholder"
    );
    assert!(
        script.contains(r#"releases/download/v\$version/adrlint_\$version_checksums.txt"#),
        "autoupdate.hash.url must point at the release checksums file"
    );

    // A missing entry is a release defect, not something to paper over with an empty hash.
    let missing = line_number(&script, "missing checksum for ${ARCHIVE} in ${SUMS}");
    let tail = &script[missing..];
    let end = tail
        .find("\nfi")
        .unwrap_or_else(|| panic!("the missing-checksum branch is not closed by `fi`"));
    assert!(
        tail[..end].contains("exit 1"),
        "a missing checksum must fail the script instead of emitting a partial manifest"
    );
    assert!(
        script.contains(r#"[[ "$HASH" =~ ^[0-9a-f]{64}$ ]]"#),
        "the manifest must refuse a malformed digest"
    );

    // Deterministic: nothing may depend on the clock or on randomness.
    for nondeterministic in ["$(date", "$RANDOM"] {
        assert!(
            !script.contains(nondeterministic),
            "the manifest generator uses `{nondeterministic}`, so its output is not reproducible"
        );
    }
}

#[test]
fn the_scoop_job_publishes_only_from_a_finalized_release() {
    let text = read(".github/workflows/publish.yaml");
    let block = job_block(&text, "publish_scoop");

    let needs = block
        .lines()
        .find(|line| line.trim_start().starts_with("needs:"))
        .expect("publish_scoop declares needs");
    for dependency in ["meta", "verify", "finalize_release"] {
        assert!(
            needs.contains(dependency),
            "publish_scoop must depend on `{dependency}`: {needs}"
        );
    }

    assert!(
        block.contains("!contains(needs.meta.outputs.version, '-rc')"),
        "a prerelease must not update the Scoop bucket"
    );
    assert!(
        block.contains("      contents: read"),
        "publish_scoop only reads its own release and pushes with a PAT"
    );
    assert!(
        !block.contains("contents: write"),
        "publish_scoop must not ask for write access to this repository"
    );

    assert!(
        block.contains(r#"gh release download "$tag" -p "adrlint_${version}_checksums.txt""#),
        "publish_scoop must download the published checksums file"
    );
    assert!(
        block.contains(r#"./scripts/update-scoop-manifest.sh "$version" checksums.txt > adrlint.json"#),
        "publish_scoop must generate the manifest with the script"
    );
    assert!(
        block.contains("GIT_CONFIG_KEY_0=http.extraheader"),
        "the bucket push must authenticate through an http.extraheader built from the environment"
    );
    assert!(
        block.contains("GIT_CONFIG_VALUE_0="),
        "the bucket push must pass the credential through GIT_CONFIG_VALUE_0"
    );
    assert!(
        block.contains("bucket/adrlint.json"),
        "publish_scoop must write bucket/adrlint.json"
    );
    assert!(
        block.contains("if git diff --cached --quiet; then"),
        "publish_scoop must only commit when the manifest changed"
    );

    for line in block.lines() {
        if line.contains("Goldziher/scoop-bucket") {
            assert!(
                !line.contains("x-access-token"),
                "the bucket clone must not embed the token in the clone URL: {line}"
            );
        }
    }
}

#[test]
fn the_scoop_script_is_executable() {
    let path = repo_root().join("scripts/update-scoop-manifest.sh");
    assert!(path.is_file(), "scripts/update-scoop-manifest.sh is missing");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("could not stat {}: {error}", path.display()))
            .permissions()
            .mode();
        assert_ne!(
            mode & 0o111,
            0,
            "scripts/update-scoop-manifest.sh is not executable (mode {mode:o})"
        );
    }
}

/// Guard: the helpers above must actually find the constructs they assert about.
#[test]
fn the_text_parsers_find_the_constructs_they_assert_about() {
    let text = read(".github/workflows/publish.yaml");
    assert!(!run_blocks(&text).is_empty());
    assert!(uses_entries(&text).contains(&"actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1".to_owned()));
    assert!(job_block(&text, "meta").contains("Validate the release tag"));
    assert!(job_block(&text, "publish_scoop").contains("Update Scoop manifest"));
    assert!(
        platform_maps()
            .iter()
            .any(|(name, _)| name == "scripts/gh-action-resolve.sh")
    );
    assert!(Path::new(&repo_root().join("action.yml")).is_file());
}
