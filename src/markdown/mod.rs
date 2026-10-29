//! rumdl's Markdown layer, linked in-process.
//!
//! The MADR rules read the document model from [`crate::adr::parse`]; rumdl runs its own parse over
//! the same bytes, so an ADR is judged as an ADR and as Markdown in one run without either layer
//! re-reading the file.

pub mod rumdl;

pub use rumdl::{check, format, rule_codes};
