use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::report::OutputFormat;

#[derive(Debug, Parser)]
#[command(
    name = "adrlint",
    version,
    about = "Lint and format MADR architecture decision records",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to the configuration file
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Disable coloured output
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Lint ADR files and the directory that holds them
    Lint(LintArgs),
    /// Format ADR files with rumdl
    Format(FormatArgs),
    /// Write a starter configuration and ADR directory
    Init(InitArgs),
    /// Create the next sequential ADR
    New(NewArgs),
    /// Record canonical content hashes for accepted ADRs
    Lock(LockArgs),
    /// Remove one ADR from the immutability manifest
    Unlock(UnlockArgs),
}

#[derive(Debug, Args)]
pub struct LintArgs {
    /// Files or directories to lint; defaults to the configured ADR directory
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Debug, Args)]
pub struct FormatArgs {
    /// Files or directories to format; defaults to the configured ADR directory
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Report files that would change without writing them
    #[arg(long)]
    pub check: bool,

    /// Write changes even when they alter the canonical content of a locked ADR
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// ADR directory, relative to the configuration file
    #[arg(long, value_name = "DIR")]
    pub dir: Option<PathBuf>,

    /// First ADR number, 0 or 1
    #[arg(long, value_parser = clap::value_parser!(u32).range(0..=1), default_value_t = 1)]
    pub start: u32,

    /// Overwrite an existing configuration file
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// Title of the decision record
    #[arg(value_name = "TITLE")]
    pub title: String,
}

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Files or directories to lock; defaults to the configured ADR directory
    #[arg(value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Verify the manifest without writing it
    #[arg(long)]
    pub check: bool,

    /// Lock every ADR, including proposed and withdrawn ones
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct UnlockArgs {
    /// ADR path, as stored in the manifest
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Why the record is being unlocked; goes into the commit message
    #[arg(long, value_name = "TEXT")]
    pub reason: String,
}
