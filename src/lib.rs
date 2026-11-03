//! A linter and formatter for [MADR](https://adr.github.io/madr/) architecture decision records.
//!
//! The CLI is a thin layer over these modules, and every module is usable on its own:
//!
//! - [`adr::parse`] owns the single Markdown parse and returns a [`Document`] with front matter,
//!   headings and section spans, blocks, links and list items.
//! - [`rules::check_document`] runs the MADR and adrlint rules over a parsed document.
//! - [`markdown::check`] and [`markdown::format`] run rumdl in-process over the same source.
//! - [`lock::canonical_hash`] produces the formatting-insensitive content hash used by
//!   `adrlint.lock`.
//! - [`config::load`] resolves the layered configuration.
//!
//! Nothing in the library shells out. The only network I/O is [`rules::check_external_links`]
//! (`MADR11`), which runs only when called; `adrlint lint` calls it only when it is opted into.
//!
//! ```no_run
//! use adrlint::{adr, rules};
//! use std::path::Path;
//!
//! let source = "# Use Rust\n\n## Context and Problem Statement\n\nText.\n";
//! let document = adr::parse(source);
//! let findings = rules::check_document(&document, Path::new("0001-use-rust.md"), Path::new("."));
//! ```
//!
//! The most common entry points are re-exported at the crate root.
//!
//! # Features
//!
//! `cli` (default) enables the command-line layer: `cli`, `report`, `template` and the `adrlint`
//! binary, with their dependencies (clap, ignore, jiff, owo-colors, serde_json). Library consumers
//! depend on adrlint with `default-features = false`; every other module stays available.

pub mod adr;
#[cfg(feature = "cli")]
pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod discovery;
pub mod fix;
pub mod lock;
pub mod markdown;
pub mod parallel;
#[cfg(feature = "cli")]
pub mod report;
pub mod rules;
#[cfg(feature = "cli")]
pub mod template;

pub use adr::{Document, parse as parse_adr};
pub use config::{Config, Loaded, load};
pub use diagnostics::{Diagnostic, Severity, has_errors};
pub use lock::canonical_hash;
pub use markdown::{check as check_markdown, format as format_markdown};
pub use rules::check_document;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
