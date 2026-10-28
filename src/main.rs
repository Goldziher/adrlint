use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Parser;

use adrlint::cli::{Cli, Command, InitArgs, LintArgs, NewArgs};
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
    }
}

fn lint(config_path: Option<&Path>, args: LintArgs, color: bool) -> Result<ExitCode> {
    let loaded = config::load(config_path)?;
    let directories = resolve_directories(&args.paths, &loaded)?;

    let mut findings = Vec::new();
    for directory in directories {
        let scanned =
            discovery::scan(&directory).with_context(|| format!("reading ADR directory {}", directory.display()))?;
        findings.extend(rules::check_directory(&scanned, &loaded.config.adr));
    }
    diagnostics::sort(&mut findings);
    report::print(&findings, args.format, color)?;

    if diagnostics::has_errors(&findings) {
        Ok(ExitCode::from(1))
    } else {
        Ok(ExitCode::SUCCESS)
    }
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
