use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Parser;

use adrlint::adr::{self, Document};
use adrlint::cli::{Cli, Command, InitArgs, LintArgs, LockArgs, NewArgs, UnlockArgs};
use adrlint::lock::{self, LockFile};
use adrlint::report::OutputFormat;
use adrlint::{config, diagnostics, discovery, report, rules, template};

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
        Command::Init(args) => init(config, args),
        Command::New(args) => new(config, args),
        Command::Lock(args) => lock_command(config, args),
        Command::Unlock(args) => unlock_command(config, args),
    }
}

fn lint(config_path: Option<&Path>, args: LintArgs, color: bool) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    let (findings, _) = collect(&loaded, &args.paths, true)?;
    report::print(&findings, args.format, color)?;

    if diagnostics::has_errors(&findings) {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

type LintResult = (Vec<diagnostics::Diagnostic>, Vec<(PathBuf, Document)>);

/// Lints every ADR reachable from `paths`, returning the findings and the parsed documents.
fn collect(loaded: &config::Loaded, paths: &[PathBuf], check_lock: bool) -> Result<LintResult> {
    let directories = resolve_directories(paths, loaded)?;
    let mut findings = Vec::new();
    let mut documents = Vec::new();

    for directory in directories {
        let scanned =
            discovery::scan(&directory).with_context(|| format!("reading ADR directory {}", directory.display()))?;
        findings.extend(rules::check_directory(&scanned, &loaded.config.adr));
        for file in &scanned.adrs {
            let source =
                fs::read_to_string(&file.path).with_context(|| format!("reading ADR {}", file.path.display()))?;
            let document = adr::parse(&source);
            findings.extend(rules::check_document(&document, &file.path, &loaded.root));
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
            eprintln!(
                "adrlint: {} uses canonical version {}; run `adrlint lock --all` to refresh it",
                lock_path(loaded).display(),
                manifest.canonical
            );
        }
        findings.extend(lock::missing_entries(&manifest, &loaded.root));
    }

    diagnostics::sort(&mut findings);
    Ok((findings, documents))
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

    fs::write(&path, config::render(&directory, args.start)).with_context(|| format!("writing {}", path.display()))?;
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
    let title = args.title.trim();
    if title.is_empty() {
        bail!("the ADR title must not be empty");
    }

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

    let name = format!("{next:04}-{}.md", template::slugify(title));
    let path = directory.join(name);
    write_new(&path, &template::render(title, "proposed"))?;
    println!("created {}", path.display());
    Ok(ExitCode::SUCCESS)
}

fn lock_command(config_path: Option<&Path>, args: LockArgs) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    if !loaded.config.lock.enabled {
        bail!("locking is disabled in the configuration; set lock.enabled = true to use it");
    }

    let (findings, documents) = collect(&loaded, &args.paths, true)?;
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
    if !existing.is_current() && (!args.all || !args.paths.is_empty()) {
        bail!(
            "{} uses canonical version {}; run `adrlint lock --all` without paths to re-lock every ADR",
            manifest_path.display(),
            existing.canonical
        );
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
