pub mod adr;
pub mod cli;
pub mod config;
pub mod diagnostics;
pub mod discovery;
pub mod lock;
pub mod markdown;
pub mod report;
pub mod rules;
pub mod template;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
