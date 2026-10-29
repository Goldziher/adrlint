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
//! Nothing in the library shells out or performs network I/O.
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

pub mod adr;
pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod discovery;
pub mod fix;
pub mod lock;
pub mod markdown;
pub mod parallel;
pub mod report;
pub mod rules;
pub mod template;

pub use adr::{Document, parse as parse_adr};
pub use config::{Config, Loaded, load};
pub use diagnostics::{Diagnostic, Severity, has_errors};
pub use lock::canonical_hash;
pub use markdown::{check as check_markdown, format as format_markdown};
pub use rules::check_document;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
