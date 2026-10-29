use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use clap::Parser;
use ignore::{WalkBuilder, WalkState};

use adrlint::adr::{self, Document};
use adrlint::cli::{Cli, Command, FormatArgs, InitArgs, LintArgs, LockArgs, NewArgs, UnlockArgs};
use adrlint::lock::{self, LockFile};
use adrlint::report::OutputFormat;
use adrlint::{config, diagnostics, discovery, fix, markdown, parallel, report, rules, template};

/// Titles produce one slug segment of at most this many bytes.
const MAX_SLUG_LEN: usize = 80;

/// ADRs larger than this are reported instead of parsed.
const MAX_ADR_BYTES: u64 = 16 * 1024 * 1024;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("adrlint: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let Cli {
        command,
        config,
        no_color,
    } = cli;
    let config = config.as_deref();
    match command {
        Command::Lint(args) => lint(config, args, !no_color),
        Command::Format(args) => format_command(config, args),
        Command::Init(args) => init(config, args),
        Command::New(args) => new(config, args),
        Command::Lock(args) => lock_command(config, args, !no_color),
        Command::Unlock(args) => unlock_command(config, args),
    }
}

fn lint(config_path: Option<&Path>, args: LintArgs, color: bool) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    report_unknown_rule_keys(&loaded);
    let directories = resolve_directories(&args.paths, &loaded, args.recursive)?;
    let named = named_files(&args.paths);

    let mut refused = false;
    if args.fix {
        let mode = FixMode {
            check: false,
            force: args.force,
            structural: true,
        };
        let outcome = apply_fixes(&loaded, &directories, named.as_ref(), &mode)?;
        for path in &outcome.changed {
            eprintln!("fixed {}", shown(path).display());
        }
        refused = !outcome.refused.is_empty() || outcome.failed;
    }

    let mut collected = collect(&loaded, &directories, true)?;
    show_paths(&mut collected.findings);
    if let Some(canonical) = collected.stale {
        report_stale_lock(&lock_path(&loaded), canonical);
    }
    report::print(&collected.findings, args.format, color)?;
    if args.format == OutputFormat::Text {
        eprintln!("{}", report::summarize(&collected.findings, collected.files));
    }

    // A fix that was refused or not written leaves the file as it was: the run did not do what was asked.
    if diagnostics::has_errors(&collected.findings) || refused {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Formats every ADR reachable from `paths`, writing only the files that change.
///
/// `--check` is the CI form: nothing is written, and a file that would change makes the command exit
/// `1` so the run fails the build rather than the diff.
///
/// A formatting fix that changes the canonical content of a locked ADR (MD034 wrapping a bare URL,
/// for instance) is refused unless `--force` is passed. Formatting the canonical serializer ignores
/// is still written: a reflow or a marker change never breaks a lock.
fn format_command(config_path: Option<&Path>, args: FormatArgs) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    let directories = resolve_directories(&args.paths, &loaded, args.recursive)?;
    let named = named_files(&args.paths);
    let mode = FixMode {
        check: args.check,
        force: args.force,
        structural: false,
    };
    let outcome = apply_fixes(&loaded, &directories, named.as_ref(), &mode)?;

    for path in &outcome.changed {
        if args.check {
            println!("would reformat {}", shown(path).display());
        } else {
            println!("reformatted {}", shown(path).display());
        }
    }

    if outcome.failed || !outcome.refused.is_empty() || (args.check && !outcome.changed.is_empty()) {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// How a fixing run treats files.
struct FixMode {
    /// Report what would change without writing.
    check: bool,
    /// Write even when a fix changes the canonical content of a locked ADR.
    force: bool,
    /// Also apply the structural MADR fixes, not just rumdl's.
    structural: bool,
}

/// What a fixing run did, in directory and file order.
#[derive(Default)]
struct FixOutcome {
    changed: Vec<PathBuf>,
    refused: Vec<PathBuf>,
    failed: bool,
}

/// The decision for one file, computed without touching the disk.
enum Planned {
    Unchanged,
    Unreadable(diagnostics::Diagnostic),
    Refused,
    /// The file as it was read, and its fixed form.
    Write {
        source: String,
        fixed: String,
    },
}

/// The files to fix when the paths name only files, as a hook passing its staged files does.
///
/// A file is still linted with its whole log, but only the named files are rewritten: a hook must not
/// reformat a sibling ADR with unstaged changes. Any directory or pattern among the paths fixes whole
/// logs, as does running with no paths.
fn named_files(paths: &[PathBuf]) -> Option<BTreeSet<PathBuf>> {
    if paths.is_empty()
        || paths
            .iter()
            .any(|path| path.is_dir() || (is_glob(path) && !path.exists()))
    {
        return None;
    }
    Some(paths.iter().filter_map(|path| fs::canonicalize(path).ok()).collect())
}

/// Fixes every numbered ADR in `directories`, or only the `named` ones.
///
/// Each file is read once and its fixed form is computed in parallel; the writes then happen in file
/// order, so the report is identical across runs.
fn apply_fixes(
    loaded: &config::Loaded,
    directories: &[PathBuf],
    named: Option<&BTreeSet<PathBuf>>,
    mode: &FixMode,
) -> Result<FixOutcome> {
    let manifest = if loaded.config.lock.enabled {
        lock::load(&lock_path(loaded))?
    } else {
        LockFile::default()
    };
    let scanned = scan_all(directories)?;
    // Only numbered ADRs are rewritten: a README or template inside a log is reported, never changed.
    let files = scanned
        .iter()
        .flat_map(|directory| directory.adrs.iter())
        .filter(|file| discovery::is_numbered(&file.name))
        .filter(|file| named.is_none_or(|named| fs::canonicalize(&file.path).is_ok_and(|path| named.contains(&path))))
        .collect::<Vec<_>>();
    // Every plan is computed before anything is written: a file that cannot be planned stops the run,
    // and a file that cannot be read is reported and leaves every file untouched. A refused fix to a
    // locked ADR is reported and skipped; the other files are still written.
    let planned = parallel::map(&files, |file| plan_fix(loaded, &manifest, mode, &file.path))
        .context("starting worker threads")?
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let unreadable = planned.iter().any(|plan| matches!(plan, Planned::Unreadable(_)));

    let mut outcome = FixOutcome::default();
    for (file, plan) in files.iter().zip(planned) {
        match plan {
            Planned::Unchanged => {}
            Planned::Unreadable(finding) => {
                eprintln!("adrlint: {}: {}", shown(&finding.path).display(), finding.message);
                outcome.failed = true;
            }
            Planned::Refused => {
                eprintln!(
                    "refusing to change locked content of {} (use --force)",
                    shown(&file.path).display()
                );
                outcome.refused.push(file.path.clone());
            }
            Planned::Write { source, fixed } => {
                if mode.check {
                    outcome.changed.push(file.path.clone());
                } else if unreadable {
                    // Nothing is written while any file cannot be read.
                } else if rewrite_if_unchanged(&file.path, &source, &fixed)? {
                    outcome.changed.push(file.path.clone());
                } else {
                    eprintln!(
                        "adrlint: {} changed while it was being fixed; not rewriting it",
                        shown(&file.path).display()
                    );
                    outcome.failed = true;
                }
            }
        }
    }
    if unreadable && !mode.check {
        eprintln!("adrlint: no file was rewritten because a file could not be read");
    }
    Ok(outcome)
}

/// Replaces `path` with `fixed` only if it still holds `source`, the text the fix was planned from.
///
/// An editor or a second run saving the file in between keeps its write instead of losing it.
fn rewrite_if_unchanged(path: &Path, source: &str, fixed: &str) -> Result<bool> {
    let current = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    if current != source.as_bytes() {
        return Ok(false);
    }
    write_atomic(path, fixed.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

fn plan_fix(loaded: &config::Loaded, manifest: &LockFile, mode: &FixMode, path: &Path) -> Result<Planned> {
    // Checked while planning, so a symlinked ADR stops the run before any file is written.
    reject_symlink(path)?;
    let source = match read_adr(path) {
        Ok(source) => source,
        Err(finding) => return Ok(Planned::Unreadable(*finding)),
    };
    let fixed = fix::fix_source(&source, path, &loaded.config.markdown, mode.structural)?;
    if fixed == source {
        return Ok(Planned::Unchanged);
    }
    if !mode.force && changes_locked_content(loaded, manifest, path, &source, &fixed) {
        return Ok(Planned::Refused);
    }
    Ok(Planned::Write { source, fixed })
}

/// Whether writing `formatted` over `source` would change the canonical content of a locked ADR.
///
/// The fix is compared with the file as it is, both hashed by the current serializer, not with the
/// manifest: an ADR that already drifted (ADR50) can still be reflowed, and a manifest written by
/// another serializer version still protects its entries from a content change.
fn changes_locked_content(
    loaded: &config::Loaded,
    manifest: &LockFile,
    path: &Path,
    source: &str,
    formatted: &str,
) -> bool {
    if manifest.entry(&lock::key(&loaded.root, path)).is_none() {
        return false;
    }
    lock::canonical_hash(&adr::parse(formatted)) != lock::canonical_hash(&adr::parse(source))
}

/// Everything one lint run learned.
struct Collected {
    findings: Vec<diagnostics::Diagnostic>,
    documents: Vec<(PathBuf, Document)>,
    /// The canonical version of a manifest written by another serializer version.
    stale: Option<u32>,
    /// ADR files examined, including ones that could not be read.
    files: usize,
}

/// One file's findings, the images it references and, when it could be read, its parsed document.
struct Checked {
    findings: Vec<diagnostics::Diagnostic>,
    images: Vec<PathBuf>,
    document: Option<(PathBuf, Document)>,
}

/// Lists every directory in parallel, in the order given.
fn scan_all(directories: &[PathBuf]) -> Result<Vec<discovery::Directory>> {
    parallel::map(directories, |directory| {
        discovery::scan(directory).with_context(|| format!("reading ADR directory {}", directory.display()))
    })
    .context("starting worker threads")?
    .into_iter()
    .collect()
}

/// Lints every ADR in `directories`.
///
/// The directories are listed and the files checked in parallel; each file is read exactly once for
/// the MADR rules, the Markdown rules and the source lines the report quotes. Results are merged in
/// input order and sorted, so the output does not depend on thread timing.
fn collect(loaded: &config::Loaded, directories: &[PathBuf], check_lock: bool) -> Result<Collected> {
    let scanned = scan_all(directories)?;
    let mut findings = Vec::new();
    let files = scanned
        .iter()
        .flat_map(|directory| directory.adrs.iter())
        .collect::<Vec<_>>();
    let mut documents = Vec::with_capacity(files.len());
    // The images each log's ADRs reference, keyed by the log: those are assets, not foreign files.
    let mut images = std::collections::BTreeMap::<PathBuf, BTreeSet<PathBuf>>::new();
    for (file, checked) in files
        .iter()
        .zip(parallel::map(&files, |file| check_file(loaded, &file.path)).context("starting worker threads")?)
    {
        findings.extend(checked.findings);
        documents.extend(checked.document);
        if let Some(parent) = file.path.parent()
            && !checked.images.is_empty()
        {
            images.entry(parent.to_path_buf()).or_default().extend(checked.images);
        }
    }
    let none = BTreeSet::new();
    for directory in &scanned {
        let referenced = images.get(&directory.path).unwrap_or(&none);
        findings.extend(rules::check_directory_with_images(
            directory,
            &loaded.config.adr,
            referenced,
        ));
    }
    let mut stale = None;

    if check_lock && loaded.config.lock.enabled {
        let manifest = lock::load(&lock_path(loaded))?;
        if manifest.is_current() {
            for (path, document) in &documents {
                let key = lock::key(&loaded.root, path);
                if let Some(entry) = manifest.entry(&key)
                    && let Some(diagnostic) = lock::verify(document, Some(entry), path)
                {
                    findings.push(diagnostic);
                }
            }
            findings.extend(lock::missing_entries(&manifest, &loaded.root));
        } else {
            // A stale manifest verifies nothing until it is refreshed, missing files included.
            stale = Some(manifest.canonical);
        }
    }

    rules::apply_overrides(&mut findings, &loaded.config.rules);
    diagnostics::add_default_help(&mut findings);
    diagnostics::sort(&mut findings);
    Ok(Collected {
        findings,
        documents,
        stale,
        files: files.len(),
    })
}

fn check_file(loaded: &config::Loaded, path: &Path) -> Checked {
    let source = match read_adr(path) {
        Ok(source) => source,
        Err(finding) => {
            return Checked {
                findings: vec![*finding],
                images: Vec::new(),
                document: None,
            };
        }
    };
    let document = adr::parse(&source);
    let mut findings = rules::check_document(&document, path, &loaded.root);
    findings.extend(markdown::check(&source, path, &loaded.config.markdown));
    diagnostics::locate(&mut findings, &source);
    fix::mark_fixable(&mut findings, &document, &source);
    // rumdl's MD004 rewrites every list marker to `*`, which is what MADR31 asks for.
    let markdown = &loaded.config.markdown;
    if markdown.enabled && !markdown.is_off("MD004") {
        for finding in findings.iter_mut().filter(|finding| finding.code == "MADR31") {
            finding.fixable = true;
        }
    }
    // `--fix` and `format` rewrite numbered ADRs only, so a README or template finding is not fixable.
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !discovery::is_numbered(&name) {
        for finding in &mut findings {
            finding.fixable = false;
        }
    }
    Checked {
        findings,
        images: rules::referenced_images(&document, path, &loaded.root),
        document: Some((path.to_path_buf(), document)),
    }
}

/// Warns once about `[rules]` and `[markdown.rules]` keys that cannot match a finding.
///
/// A typo is otherwise a silent no-op: the run looks clean while the intended override is ignored.
fn report_unknown_rule_keys(loaded: &config::Loaded) {
    for key in rules::unknown_rule_keys(&loaded.config.rules, &loaded.config.markdown.rules) {
        eprintln!("adrlint: warning: rule key {key:?} matches no implemented rule");
    }
}

/// Reads an ADR, turning an IO or UTF-8 failure into a finding instead of aborting the whole run.
///
/// A file over [`MAX_ADR_BYTES`] is reported rather than read: no decision record is that large, and
/// parsing it would stall the run.
fn read_adr(path: &Path) -> std::result::Result<String, Box<diagnostics::Diagnostic>> {
    let unreadable = |error: std::io::Error| {
        Box::new(diagnostics::Diagnostic::error(
            "ADR52",
            path,
            1,
            format!("cannot read the ADR: {error}"),
        ))
    };
    let length = fs::metadata(path).map_err(unreadable)?.len();
    if length > MAX_ADR_BYTES {
        return Err(Box::new(diagnostics::Diagnostic::error(
            "ADR52",
            path,
            1,
            format!("the ADR is {length} bytes, over the {MAX_ADR_BYTES} byte limit"),
        )));
    }
    let bytes = fs::read(path).map_err(unreadable)?;
    String::from_utf8(bytes).map_err(|error| {
        Box::new(diagnostics::Diagnostic::error(
            "ADR52",
            path,
            1,
            format!("the ADR is not valid UTF-8: {error}"),
        ))
    })
}

/// The path to show the user: relative to the working directory when it lies under it.
///
/// ADR directories are canonicalised so overlapping roots can be compared, which makes every path
/// absolute. A relative path keeps GitHub annotations and editor links working and drops a Windows
/// `\\?\` prefix; a path outside the working directory stays absolute.
fn shown(path: &Path) -> PathBuf {
    static WORKING_DIRECTORY: OnceLock<Option<PathBuf>> = OnceLock::new();
    let working = WORKING_DIRECTORY.get_or_init(|| std::env::current_dir().and_then(fs::canonicalize).ok());
    match working.as_deref().and_then(|working| path.strip_prefix(working).ok()) {
        Some(relative) if !relative.as_os_str().is_empty() => relative.to_path_buf(),
        _ => path.to_path_buf(),
    }
}

/// Rewrites every finding's path to the form [`shown`] prints.
fn show_paths(findings: &mut [diagnostics::Diagnostic]) {
    for finding in findings {
        finding.path = shown(&finding.path);
    }
}

/// Reports a manifest written by another canonical serializer version exactly once.
fn report_stale_lock(path: &Path, canonical: u32) {
    eprintln!(
        "adrlint: {} uses canonical version {}; run `adrlint lock --all` to refresh it",
        path.display(),
        canonical
    );
}

fn lock_path(loaded: &config::Loaded) -> PathBuf {
    loaded.root.join(&loaded.config.lock.path)
}

/// The ADR directories the paths (or the configuration, or discovery) name, sorted and unique.
///
/// Each root is walked at most once: discovery hands back the directories it already found, and with
/// `--recursive` a root that lies inside another root is covered by that root's walk.
fn resolve_directories(paths: &[PathBuf], loaded: &config::Loaded, recursive: bool) -> Result<Vec<PathBuf>> {
    let (sources, discovered) = roots(paths, loaded)?;
    if let Some(directories) = discovered {
        return Ok(directories);
    }
    let recursive = recursive || loaded.config.adr.recursive;
    let mut resolved = Vec::new();
    for source in sources {
        resolved.extend(expand_source(&source)?);
    }
    if !recursive {
        return Ok(resolved.into_iter().collect::<BTreeSet<_>>().into_iter().collect());
    }
    // Every root is walked: a nested root may sit under a directory another root's walk ignores, so
    // pruning by path would drop it. The directory set removes the overlap.
    let roots = resolved.into_iter().collect::<BTreeSet<_>>();
    let mut directories = BTreeSet::new();
    for root in &roots {
        directories.extend(descend(root));
    }
    // Paths a hook passed that all sit outside any log leave no root, which is not an error. A root
    // that was named, on the command line or in the configuration, and holds no log is.
    if directories.is_empty() && !roots.is_empty() {
        bail!(
            "no ADR log found under {} (no directory there holds a NNNN-*.md file)",
            roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(directories.into_iter().collect())
}

/// One source as the user wrote it, plus the absolute form to read.
///
/// The pattern belongs to the written form: a project root that itself contains `[` must not turn a
/// literal configured path into a pattern, nor break a configured pattern joined onto it.
struct Source {
    path: PathBuf,
    /// The glob to expand, with any root it was joined onto escaped; `None` for a literal path.
    pattern: Option<String>,
}

impl Source {
    /// A path the user wrote, relative to the working directory.
    fn written(path: PathBuf) -> Self {
        // A path that exists is literal even if it contains `[`, as a staged `adrs/[draft]/0001-x.md` does.
        let pattern = (is_glob(&path) && !path.exists()).then(|| path.to_string_lossy().into_owned());
        Self { path, pattern }
    }

    /// A configured path, relative to the project root.
    fn configured(root: &Path, written: &Path) -> Self {
        let path = root.join(written);
        let pattern = (is_glob(written) && !path.exists())
            .then(|| escape_glob_root(root).join(written).to_string_lossy().into_owned());
        Self { path, pattern }
    }
}

/// `root` with every normal component escaped for `glob`, so `/tmp/[client] repo` matches itself.
///
/// The prefix and root components are kept verbatim: escaping the `?` of a canonical Windows
/// `\\?\C:\` prefix would turn it into a pattern.
fn escape_glob_root(root: &Path) -> PathBuf {
    root.components()
        .map(|component| match component {
            std::path::Component::Normal(name) => {
                std::ffi::OsString::from(glob::Pattern::escape(&name.to_string_lossy()))
            }
            other => other.as_os_str().to_owned(),
        })
        .collect()
}

/// The sources to resolve, highest precedence first: CLI paths, then `[adr].paths`, then `[adr].dir`,
/// or, when none of those applies, the ADR directories discovered under the project root.
///
/// CLI paths are interpreted relative to the working directory; configured paths are relative to the
/// project root, so they are joined here rather than by the caller. With nothing named and no
/// `docs/adr`, the project root is searched for ADR logs, so a repository with `adrs/<area>/` folders
/// or a service's own `adrs/` works without configuration; that one walk is returned as the
/// directories, not walked again. A `dir` a configuration names must exist.
fn roots(paths: &[PathBuf], loaded: &config::Loaded) -> Result<(Vec<Source>, Option<Vec<PathBuf>>)> {
    if !paths.is_empty() {
        return Ok((paths.iter().map(|path| Source::written(path.clone())).collect(), None));
    }
    if !loaded.config.adr.paths.is_empty() {
        let sources = loaded
            .config
            .adr
            .paths
            .iter()
            .map(|path| Source::configured(&loaded.root, path))
            .collect();
        return Ok((sources, None));
    }
    let directory = loaded.root.join(&loaded.config.adr.dir);
    if directory.is_dir() {
        return Ok((vec![Source::configured(&loaded.root, &loaded.config.adr.dir)], None));
    }
    if loaded.config.adr.dir_configured {
        bail!(
            "ADR directory {} does not exist; run `adrlint init` first",
            directory.display()
        );
    }
    let found = descend(&loaded.root);
    if found.is_empty() {
        bail!(
            "no ADR log found under {} (no `docs/adr`, and no directory holds a NNNN-*.md file); run `adrlint init` first",
            loaded.root.display()
        );
    }
    Ok((Vec::new(), Some(found)))
}

/// Expands one source into ADR directories.
///
/// A source with glob metacharacters is expanded; every match is classified like a literal path, so a
/// matching file contributes its parent directory and a matching directory becomes a root. A pattern
/// that is invalid or that matches nothing is an error: silently linting nothing would look green.
fn expand_source(source: &Source) -> Result<Vec<PathBuf>> {
    if let Some(pattern) = &source.pattern {
        let matches =
            glob::glob(pattern).map_err(|error| anyhow::anyhow!("invalid glob pattern {pattern}: {error}"))?;
        let mut directories = Vec::new();
        let mut matched = false;
        for entry in matches {
            let path = entry.with_context(|| format!("reading glob match of {pattern}"))?;
            matched = true;
            directories.extend(classify(&path)?);
        }
        if !matched {
            bail!("nothing matched {pattern}");
        }
        return Ok(directories);
    }
    Ok(classify(&source.path)?.into_iter().collect())
}

fn is_glob(path: &Path) -> bool {
    path.to_string_lossy()
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

/// The ADR directory a path belongs to: itself when it is a directory, its parent when it is a file.
///
/// A file is always linted with its directory, so a gap or duplicate next to it is still reported. A
/// file that is not an ADR in a directory that is not an ADR log (`adrs/README.md`, an ADR template)
/// belongs to no log and is skipped: a hook passes every changed Markdown file under `adrs/`.
fn classify(path: &Path) -> Result<Option<PathBuf>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        // A hook can pass an ADR the commit deletes: lint the log it left, if there still is one. Any
        // other missing path is an error, so a mistyped path cannot look like a clean run.
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && path.extension().is_some_and(|extension| extension == "md") =>
        {
            let parent = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            if !parent.is_dir() || !contains_numbered_adr(parent)? {
                return Ok(None);
            }
            return fs::canonicalize(parent)
                .map(Some)
                .with_context(|| format!("resolving {}", parent.display()));
        }
        Err(error) => return Err(error).with_context(|| format!("reading path {}", path.display())),
    };
    let directory = if metadata.is_dir() {
        path.to_path_buf()
    } else {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !discovery::is_numbered(&name) && !contains_numbered_adr(&parent)? {
            return Ok(None);
        }
        parent
    };
    fs::canonicalize(&directory)
        .map(Some)
        .with_context(|| format!("resolving {}", directory.display()))
}

/// Directories `--recursive` never descends into.
const SKIPPED_DIRECTORIES: [&str; 3] = [".git", "target", "node_modules"];

/// Every directory under `root`, including `root`, that directly contains a numbered ADR file.
///
/// A recursive root is a search base, not an ADR log: `adrlint lint --recursive .` must not treat the
/// repository root's `README.md` and `.adrlint.toml` as foreign files in an ADR directory. Only a
/// directory holding a `NNNN`-prefixed Markdown file is scanned, and a directory with no ADR at all
/// contributes nothing. Hidden directories, `target`, `node_modules` and whatever `.gitignore` excludes
/// are skipped, symlinks are not followed, and the walk runs in parallel, so a monorepo root is fast.
///
/// A path the walk cannot read (a directory without permission, a broken `.gitignore`) is reported on
/// stderr and skipped: one unreadable folder must not hide every other ADR log.
fn descend(root: &Path) -> Vec<PathBuf> {
    let found = Mutex::new(BTreeSet::new());
    let skipped = Mutex::new(BTreeSet::<String>::new());
    let found_ref = &found;
    let skipped_ref = &skipped;
    WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .ignore(false)
        .parents(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0 || !SKIPPED_DIRECTORIES.contains(&entry.file_name().to_string_lossy().as_ref())
        })
        .build_parallel()
        .run(|| {
            // Entries of one directory arrive together, so remembering the last parent keeps the
            // shared set off the hot path.
            let mut last: Option<PathBuf> = None;
            Box::new(move |entry| {
                match entry {
                    Ok(entry) => {
                        let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
                        if is_file
                            && discovery::is_numbered(&entry.file_name().to_string_lossy())
                            && let Some(parent) = entry.path().parent()
                            && last.as_deref() != Some(parent)
                        {
                            found_ref.lock().expect("discovery set").insert(parent.to_path_buf());
                            last = Some(parent.to_path_buf());
                        }
                    }
                    Err(error) => {
                        skipped_ref.lock().expect("discovery errors").insert(error.to_string());
                    }
                }
                WalkState::Continue
            })
        });
    for message in skipped.into_inner().expect("discovery errors") {
        eprintln!("adrlint: warning: skipping a path under {}: {message}", root.display());
    }
    found.into_inner().expect("discovery set").into_iter().collect()
}

/// Whether a directory directly holds a `NNNN-*.md`-shaped file.
///
/// The prefix is all that matters: a malformed name such as `0001_use_rust.md` still belongs to the
/// log and must reach `MADR44`, while `README.md` does not make its parent an ADR directory.
fn contains_numbered_adr(directory: &Path) -> Result<bool> {
    for entry in fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))? {
        let entry = entry.with_context(|| format!("reading {}", directory.display()))?;
        if entry.file_type().is_ok_and(|file_type| file_type.is_dir()) {
            continue;
        }
        if discovery::is_numbered(&entry.file_name().to_string_lossy()) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn init(config_path: Option<&Path>, args: InitArgs) -> Result<ExitCode> {
    let cwd = std::env::current_dir().context("reading the current directory")?;
    let path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.join(".adrlint.toml"));
    if is_symlink(&path)? {
        bail!("{} is a symlink; refusing to overwrite it", path.display());
    }
    if path.exists() && !args.force {
        bail!("{} already exists; pass --force to overwrite it", path.display());
    }

    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(&cwd)
        .to_path_buf();
    let directory = args.dir.unwrap_or_else(|| PathBuf::from("docs/adr"));
    config::validate_dir(&directory).context("invalid --dir")?;
    let adr_directory = root.join(&directory);

    write_atomic(&path, config::render(&directory, args.start).as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    fs::create_dir_all(&adr_directory).with_context(|| format!("creating {}", adr_directory.display()))?;

    let scanned = discovery::scan(&adr_directory)?;
    if scanned.adrs.is_empty() {
        let name = format!("{:04}-record-architecture-decisions.md", args.start);
        let adr_path = adr_directory.join(name);
        write_new(
            &adr_path,
            &template::render("Record architecture decisions", "accepted"),
        )?;
        println!("created {}", adr_path.display());
    }
    println!("created {}", path.display());
    Ok(ExitCode::SUCCESS)
}

fn new(config_path: Option<&Path>, args: NewArgs) -> Result<ExitCode> {
    let title = validate_title(&args.title)?;

    let loaded = config::load(config_path)?;
    let directory = loaded.root.join(&loaded.config.adr.dir);
    if !directory.is_dir() {
        bail!(
            "ADR directory {} does not exist; run `adrlint init` first",
            directory.display()
        );
    }

    let scanned = discovery::scan(&directory)?;
    let next = scanned
        .adrs
        .iter()
        .filter_map(|file| file.number)
        .max()
        .map_or(loaded.config.adr.start, |number| number + 1);
    if next > 9999 {
        bail!("the ADR directory has reached the maximum number, 9999");
    }

    let name = format!("{next:04}-{}.md", truncate_slug(&template::slugify(title)));
    let path = directory.join(name);
    write_new(&path, &template::render(title, "proposed"))?;
    println!("created {}", path.display());
    Ok(ExitCode::SUCCESS)
}

/// Validates a new ADR title before it reaches a file name.
fn validate_title(raw: &str) -> Result<&str> {
    let title = raw.trim();
    if title.is_empty() {
        bail!("the ADR title must not be empty");
    }
    if title.chars().any(char::is_control) {
        bail!("the ADR title must not contain control characters");
    }
    if !title.chars().any(|character| character.is_ascii_alphanumeric()) {
        bail!("the ADR title must contain at least one ASCII letter or digit");
    }
    Ok(title)
}

/// Truncates a slug to [`MAX_SLUG_LEN`] ASCII bytes, cutting at a hyphen when one is available.
fn truncate_slug(slug: &str) -> &str {
    if slug.len() <= MAX_SLUG_LEN {
        return slug;
    }
    let boundary = slug[..MAX_SLUG_LEN]
        .rfind('-')
        .filter(|index| *index > 0)
        .unwrap_or(MAX_SLUG_LEN);
    &slug[..boundary]
}

fn lock_command(config_path: Option<&Path>, args: LockArgs, color: bool) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    if !loaded.config.lock.enabled {
        bail!("locking is disabled in the configuration; set lock.enabled = true to use it");
    }

    let directories = resolve_directories(&args.paths, &loaded, args.recursive)?;
    let Collected {
        mut findings,
        documents,
        stale,
        ..
    } = collect(&loaded, &directories, true)?;
    show_paths(&mut findings);
    if diagnostics::has_errors(&findings) {
        report::print(&findings, OutputFormat::Text, color)?;
        if args.check {
            return Ok(ExitCode::from(1));
        }
        let errors = findings.iter().filter(|finding| finding.severity.is_error()).count();
        bail!("refusing to lock: fix {errors} error(s) first");
    }

    let manifest_path = lock_path(&loaded);
    let existing = lock::load(&manifest_path)?;
    if let Some(canonical) = stale {
        report_stale_lock(&manifest_path, canonical);
        if args.check {
            return Ok(ExitCode::from(1));
        }
        if !args.all || !args.paths.is_empty() {
            bail!(
                "{} uses canonical version {}; run `adrlint lock --all` without paths to re-lock every ADR",
                manifest_path.display(),
                canonical
            );
        }
    }

    let mut candidate = existing.clone();
    candidate.version = lock::LOCK_VERSION;
    candidate.canonical = lock::CANONICAL_VERSION;
    let mut locked = 0;
    for (path, document) in &documents {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(number) = discovery::parse_number(name) else {
            continue;
        };
        let status = lock::status(document);
        if !args.all && !config::is_locked_status(&loaded.config.lock, &status) {
            continue;
        }
        // A log reached through a symlink can resolve outside the project; its key could not be loaded.
        if path.strip_prefix(&loaded.root).is_err() {
            bail!(
                "refusing to lock {}: it is outside the project root {}",
                path.display(),
                loaded.root.display()
            );
        }
        candidate.insert(lock::key(&loaded.root, path), lock::entry(document, number));
        locked += 1;
    }

    if locked == 0 && existing.adrs.is_empty() {
        bail!("no ADRs matched lock.statuses; pass --all to lock every ADR");
    }

    if args.check {
        if candidate == existing {
            println!("{} is up to date", manifest_path.display());
            return Ok(ExitCode::SUCCESS);
        }
        report_lock_changes(&existing, &candidate);
        return Ok(ExitCode::from(1));
    }

    lock::save(&manifest_path, &candidate)?;
    println!("locked {locked} ADR(s) in {}", manifest_path.display());
    Ok(ExitCode::SUCCESS)
}

fn report_lock_changes(existing: &LockFile, candidate: &LockFile) {
    for (key, entry) in &candidate.adrs {
        match existing.entry(key) {
            None => eprintln!("would add {key}"),
            Some(current) if current != entry => eprintln!("would update {key}"),
            Some(_) => {}
        }
    }
    for key in existing.adrs.keys() {
        if candidate.entry(key).is_none() {
            eprintln!("would remove {key}");
        }
    }
}

fn unlock_command(config_path: Option<&Path>, args: UnlockArgs) -> Result<ExitCode> {
    let reason = args.reason.trim();
    if reason.is_empty() {
        bail!("--reason must not be empty");
    }

    let loaded = config::load(config_path)?;
    let manifest_path = lock_path(&loaded);
    let mut manifest = lock::load(&manifest_path)?;
    let path = if args.path.is_absolute() {
        args.path
    } else {
        loaded.root.join(&args.path)
    };
    let key = lock::key(&loaded.root, &path);
    if !lock::unlock(&mut manifest, &key) {
        bail!("no lock entry for {}; expected manifest key '{}'", path.display(), key);
    }
    lock::save(&manifest_path, &manifest)?;
    println!("unlocked {key}: {reason}");
    Ok(ExitCode::SUCCESS)
}

fn write_new(path: &Path, contents: &str) -> Result<()> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            bail!("{} already exists", path.display());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("creating {}", path.display()));
        }
    };
    file.write_all(contents.as_bytes())
        .with_context(|| format!("writing {}", path.display()))
}

fn is_symlink(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(metadata.file_type().is_symlink()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
    }
}

/// Refuses to write through a symlink, so a formatting run cannot rewrite a file outside the ADR
/// directory.
fn reject_symlink(path: &Path) -> Result<()> {
    if is_symlink(path)? {
        bail!("refusing to write through the symlink at {}", path.display());
    }
    Ok(())
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to a temporary file beside `path`, then renames it into place.
///
/// A rename is atomic on the same filesystem, so an interrupted write cannot leave a half-written
/// ADR or configuration behind.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("adrlint");
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
        // A rename replaces the file, so the original's permissions are carried over explicitly.
        if let Ok(metadata) = fs::metadata(path) {
            fs::set_permissions(&temporary, metadata.permissions())
                .with_context(|| format!("setting the permissions of {}", temporary.display()))?;
        }
        fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_report_an_oversized_adr_instead_of_reading_it() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("0001-huge.md");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_ADR_BYTES + 1).unwrap();
        let finding = read_adr(&path).unwrap_err();
        assert_eq!(finding.code, "ADR52");
        assert!(finding.message.contains("byte limit"), "{}", finding.message);
    }

    #[test]
    fn should_escape_glob_metacharacters_in_a_root() {
        let root = Path::new("/tmp/[client] repo");
        let pattern = escape_glob_root(root).join("adrs/*");
        assert_eq!(pattern, Path::new("/tmp/[[]client[]] repo/adrs/*"));
    }

    #[test]
    fn should_not_rewrite_a_file_that_changed_since_it_was_planned() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("0001-x.md");
        fs::write(&path, "edited elsewhere\n").unwrap();
        assert!(!rewrite_if_unchanged(&path, "planned\n", "fixed\n").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "edited elsewhere\n");

        fs::write(&path, "planned\n").unwrap();
        assert!(rewrite_if_unchanged(&path, "planned\n", "fixed\n").unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), "fixed\n");
    }
}
