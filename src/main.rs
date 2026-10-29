use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail};
use clap::Parser;

use adrlint::adr::{self, Document};
use adrlint::cli::{Cli, Command, FormatArgs, InitArgs, LintArgs, LockArgs, NewArgs, UnlockArgs};
use adrlint::lock::{self, LockFile};
use adrlint::report::OutputFormat;
use adrlint::{config, diagnostics, discovery, markdown, report, rules, template};

/// Titles produce one slug segment of at most this many bytes.
const MAX_SLUG_LEN: usize = 80;

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
        Command::Lock(args) => lock_command(config, args),
        Command::Unlock(args) => unlock_command(config, args),
    }
}

fn lint(config_path: Option<&Path>, args: LintArgs, color: bool) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    let (findings, _, stale) = collect(&loaded, &args.paths, true)?;
    if let Some(canonical) = stale {
        report_stale_lock(&lock_path(&loaded), canonical);
    }
    report::print(&findings, args.format, color)?;

    if diagnostics::has_errors(&findings) {
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
    let manifest = if loaded.config.lock.enabled {
        lock::load(&lock_path(&loaded))?
    } else {
        LockFile::default()
    };
    let directories = resolve_directories(&args.paths, &loaded)?;
    let mut changed = Vec::new();
    let mut refused = Vec::new();

    for directory in directories {
        let scanned =
            discovery::scan(&directory).with_context(|| format!("reading ADR directory {}", directory.display()))?;
        for file in &scanned.adrs {
            let source =
                fs::read_to_string(&file.path).with_context(|| format!("reading ADR {}", file.path.display()))?;
            let formatted = markdown::format(&source, &file.path, &loaded.config.markdown)?;
            if formatted == source {
                continue;
            }
            if changes_locked_content(&loaded, &manifest, &file.path, &formatted) && !args.force {
                eprintln!(
                    "refusing to change locked content of {} (use --force)",
                    file.path.display()
                );
                refused.push(file.path.clone());
                continue;
            }
            if !args.check {
                reject_symlink(&file.path)?;
                write_atomic(&file.path, formatted.as_bytes())
                    .with_context(|| format!("writing {}", file.path.display()))?;
            }
            changed.push(file.path.clone());
        }
    }

    for path in &changed {
        if args.check {
            println!("would reformat {}", path.display());
        } else {
            println!("reformatted {}", path.display());
        }
    }

    if !refused.is_empty() || (args.check && !changed.is_empty()) {
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

/// Whether writing `formatted` would change the canonical content of a locked ADR.
///
/// A stale manifest is ignored: its hashes belong to another serializer version and cannot be
/// trusted, so it must not block formatting.
fn changes_locked_content(loaded: &config::Loaded, manifest: &LockFile, path: &Path, formatted: &str) -> bool {
    if !manifest.is_current() {
        return false;
    }
    let Some(entry) = manifest.entry(&lock::key(&loaded.root, path)) else {
        return false;
    };
    lock::canonical_hash(&adr::parse(formatted)) != entry.blake3
}

type LintResult = (Vec<diagnostics::Diagnostic>, Vec<(PathBuf, Document)>, Option<u32>);

/// Lints every ADR reachable from `paths`, returning the findings, the parsed documents and the
/// stale canonical version when the manifest belongs to another serializer version.
fn collect(loaded: &config::Loaded, paths: &[PathBuf], check_lock: bool) -> Result<LintResult> {
    let directories = resolve_directories(paths, loaded)?;
    let mut findings = Vec::new();
    let mut documents = Vec::new();
    let mut stale = None;

    for directory in directories {
        let scanned =
            discovery::scan(&directory).with_context(|| format!("reading ADR directory {}", directory.display()))?;
        findings.extend(rules::check_directory(&scanned, &loaded.config.adr));
        for file in &scanned.adrs {
            let source =
                fs::read_to_string(&file.path).with_context(|| format!("reading ADR {}", file.path.display()))?;
            let document = adr::parse(&source);
            findings.extend(rules::check_document(&document, &file.path, &loaded.root));
            findings.extend(markdown::check(&source, &file.path, &loaded.config.markdown));
            documents.push((file.path.clone(), document));
        }
    }

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
        } else {
            stale = Some(manifest.canonical);
        }
        findings.extend(lock::missing_entries(&manifest, &loaded.root));
    }

    rules::apply_overrides(&mut findings, &loaded.config.rules);
    diagnostics::sort(&mut findings);
    Ok((findings, documents, stale))
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

fn resolve_directories(paths: &[PathBuf], loaded: &config::Loaded) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        let directory = loaded.root.join(&loaded.config.adr.dir);
        if !directory.is_dir() {
            bail!(
                "ADR directory {} does not exist; run `adrlint init` first",
                directory.display()
            );
        }
        return Ok(vec![directory]);
    }

    let mut directories = BTreeSet::new();
    for path in paths {
        let metadata = fs::metadata(path).with_context(|| format!("reading path {}", path.display()))?;
        let directory = if metadata.is_dir() {
            path.clone()
        } else {
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        };
        let directory = fs::canonicalize(&directory).with_context(|| format!("resolving {}", directory.display()))?;
        directories.insert(directory);
    }
    Ok(directories.into_iter().collect())
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

fn lock_command(config_path: Option<&Path>, args: LockArgs) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    if !loaded.config.lock.enabled {
        bail!("locking is disabled in the configuration; set lock.enabled = true to use it");
    }

    let (findings, documents, stale) = collect(&loaded, &args.paths, true)?;
    if diagnostics::has_errors(&findings) {
        report::print(&findings, OutputFormat::Text, true)?;
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
        fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
