pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod discovery;
pub mod report;
pub mod rules;
pub mod template;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
