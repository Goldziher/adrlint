use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const CONFIG_FILE_NAMES: [&str; 2] = [".adrlint.toml", "adrlint.toml"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub adr: AdrConfig,
    #[serde(default)]
    pub lock: LockConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockConfig {
    #[serde(default = "default_lock_enabled")]
    pub enabled: bool,
    #[serde(default = "default_lock_path")]
    pub path: PathBuf,
    #[serde(default = "default_lock_statuses")]
    pub statuses: Vec<String>,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: default_lock_path(),
            statuses: default_lock_statuses(),
        }
    }
}

fn default_lock_enabled() -> bool {
    true
}

fn default_lock_path() -> PathBuf {
    PathBuf::from("adrlint.lock")
}

fn default_lock_statuses() -> Vec<String> {
    vec!["accepted".to_owned(), "rejected".to_owned()]
}

/// Whether a status is covered by the lock list, case-insensitively.
pub fn is_locked_status(config: &LockConfig, status: &str) -> bool {
    config.statuses.iter().any(|locked| locked.eq_ignore_ascii_case(status))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdrConfig {
    #[serde(default = "default_dir")]
    pub dir: PathBuf,
    #[serde(default = "default_start")]
    pub start: u32,
    #[serde(default)]
    pub allow_gaps: bool,
}

impl Default for AdrConfig {
    fn default() -> Self {
        Self {
            dir: default_dir(),
            start: default_start(),
            allow_gaps: false,
        }
    }
}

fn default_dir() -> PathBuf {
    PathBuf::from("docs/adr")
}

fn default_start() -> u32 {
    1
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub root: PathBuf,
    pub path: Option<PathBuf>,
}

pub fn load(explicit: Option<&Path>) -> Result<Loaded> {
    if let Some(path) = explicit {
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let config = read(path)?;
        return Ok(Loaded {
            config,
            root: absolute(root),
            path: Some(path.to_path_buf()),
        });
    }

    let cwd = std::env::current_dir().context("reading the current directory")?;
    for ancestor in cwd.ancestors() {
        for name in CONFIG_FILE_NAMES {
            let candidate = ancestor.join(name);
            if candidate.is_file() {
                let config = read(&candidate)?;
                return Ok(Loaded {
                    config,
                    root: absolute(ancestor.to_path_buf()),
                    path: Some(candidate),
                });
            }
        }
    }

    Ok(Loaded {
        config: Config::default(),
        root: absolute(cwd),
        path: None,
    })
}

/// Canonicalises a root when possible; a root that does not exist is kept as given.
fn absolute(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn read(path: &Path) -> Result<Config> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading configuration from {}", path.display()))?;
    let config: Config =
        toml::from_str(&contents).with_context(|| format!("parsing configuration from {}", path.display()))?;
    validate(&config).with_context(|| format!("invalid configuration in {}", path.display()))?;
    Ok(config)
}

fn validate(config: &Config) -> Result<()> {
    if !matches!(config.adr.start, 0 | 1) {
        bail!("adr.start must be 0 or 1, found {}", config.adr.start);
    }
    validate_dir(&config.adr.dir)?;
    validate_relative(&config.lock.path, "lock.path")?;
    if config.lock.statuses.iter().any(|status| status.trim().is_empty()) {
        bail!("lock.statuses must not contain empty values");
    }
    Ok(())
}

pub fn validate_dir(dir: &Path) -> Result<()> {
    validate_relative(dir, "adr.dir")
}

fn validate_relative(path: &Path, field: &str) -> Result<()> {
    if path.is_absolute() {
        bail!("{field} must be relative to the configuration file");
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        bail!("{field} must not escape the project root");
    }
    if path.as_os_str().is_empty() {
        bail!("{field} must not be empty");
    }
    Ok(())
}

pub fn render(dir: &Path, start: u32) -> String {
    let dir = dir.to_string_lossy().replace('\\', "/");
    format!(
        r#"# adrlint configuration
#
# The ADR directory, relative to this file. ADR files are `NNNN-brief-description.md`.
[adr]
dir = "{}"
# 0 or 1. The base is fixed for the life of the directory; do not change it later.
start = {}
# Only for imported histories that legitimately start with holes.
allow_gaps = false

# Accepted decisions are immutable once locked. Presence of the manifest is what opts in.
[lock]
enabled = true
path = "adrlint.lock"
statuses = ["accepted", "rejected"]
"#,
        dir, start
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_start_outside_zero_or_one() {
        let config = Config {
            adr: AdrConfig {
                start: 2,
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("adr.start must be 0 or 1"));
    }

    #[test]
    fn should_reject_directory_escaping_the_project_root() {
        let config = Config {
            adr: AdrConfig {
                dir: PathBuf::from("../adr"),
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("must not escape"));
    }

    #[test]
    fn should_reject_absolute_directory() {
        let config = Config {
            adr: AdrConfig {
                dir: std::env::temp_dir().join("adr"),
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("must be relative"));
    }
}
