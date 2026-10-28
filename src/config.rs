use std::collections::BTreeMap;
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
    #[serde(default)]
    pub markdown: MarkdownConfig,
    #[serde(default)]
    pub rules: BTreeMap<String, String>,
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

/// rumdl's Markdown layer.
///
/// `rules` holds per-rule settings (`off`, `warn`, `error`) keyed by rumdl's own `MD###` code. The
/// defaults are applied when the table is parsed and merged under whatever the user wrote, so
/// naming one rule never silently re-enables the opinionated defaults.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkdownConfig {
    #[serde(default = "default_markdown_enabled")]
    pub enabled: bool,
    #[serde(default = "default_markdown_rules")]
    pub rules: BTreeMap<String, String>,
}

impl Default for MarkdownConfig {
    fn default() -> Self {
        Self {
            enabled: default_markdown_enabled(),
            rules: default_markdown_rules(),
        }
    }
}

impl MarkdownConfig {
    /// Fills in the opinionated defaults without overriding anything the user set.
    ///
    /// Rule codes are matched case-insensitively, so a lower-case `md009` in the configuration turns
    /// the same rule off instead of silently doing nothing.
    fn with_defaults(mut self) -> Self {
        let mut rules = default_markdown_rules();
        for (code, setting) in std::mem::take(&mut self.rules) {
            rules.insert(code.trim().to_ascii_uppercase(), setting);
        }
        self.rules = rules;
        self
    }

    /// The configured setting for a rule, if any.
    pub fn setting(&self, code: &str) -> Option<&str> {
        self.rules.get(code).map(|setting| setting.trim())
    }

    /// Whether the rule is turned off. `off` removes the finding and its fix.
    pub fn is_off(&self, code: &str) -> bool {
        self.setting(code)
            .is_some_and(|setting| setting.eq_ignore_ascii_case("off"))
    }
}

fn default_markdown_enabled() -> bool {
    true
}

fn default_markdown_rules() -> BTreeMap<String, String> {
    BTreeMap::from([
        // ADR prose is reflowed by its authors, so a fixed line length fights the template.
        ("MD013".to_owned(), "off".to_owned()),
        // Decision records legitimately embed HTML tables and other inline markup.
        ("MD033".to_owned(), "off".to_owned()),
    ])
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
    let mut config: Config =
        toml::from_str(&contents).with_context(|| format!("parsing configuration from {}", path.display()))?;
    config.markdown = config.markdown.with_defaults();
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
    for (code, setting) in &config.markdown.rules {
        if code.trim().is_empty() {
            bail!("markdown.rules keys must not be empty");
        }
        let setting = setting.trim();
        if !matches_ignore_ascii_case(setting, &["off", "warn", "error"]) {
            bail!("markdown.rules.{code} must be one of off, warn or error, found {setting:?}");
        }
    }
    for (code, setting) in &config.rules {
        if code.trim().is_empty() {
            bail!("rules keys must not be empty");
        }
        let setting = setting.trim();
        if !matches_ignore_ascii_case(setting, &["off", "warn", "error"]) {
            bail!("rules.{code} must be one of off, warn or error, found {setting:?}");
        }
    }
    Ok(())
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.eq_ignore_ascii_case(candidate))
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

# rumdl's Markdown rules run over the same source as the MADR rules.
[markdown]
enabled = true

[markdown.rules]
# ADR prose is reflowed by its authors, so a fixed line length is not enforced.
MD013 = "off"
# Decision records legitimately embed HTML tables and other inline markup.
MD033 = "off"

# Per-rule severity for MADR and ADR codes. Keys match by longest prefix.
[rules]
# MADR03 = "warn"
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

    #[test]
    fn should_default_to_disabling_line_length_and_inline_html() {
        let markdown = MarkdownConfig::default();
        assert!(markdown.enabled);
        assert!(markdown.is_off("MD013"));
        assert!(markdown.is_off("MD033"));
        assert!(!markdown.is_off("MD009"));
    }

    #[test]
    fn should_keep_the_defaults_when_only_one_rule_is_configured() {
        let parsed: Config = toml::from_str("[markdown.rules]\nMD009 = \"off\"\n").unwrap();
        let markdown = parsed.markdown.with_defaults();
        assert_eq!(markdown.setting("MD009"), Some("off"));
        assert_eq!(markdown.setting("MD013"), Some("off"));
        assert_eq!(markdown.setting("MD033"), Some("off"));
    }

    #[test]
    fn should_let_an_explicit_setting_override_a_default() {
        let parsed: Config = toml::from_str("[markdown.rules]\nMD013 = \"error\"\n").unwrap();
        let markdown = parsed.markdown.with_defaults();
        assert_eq!(markdown.setting("MD013"), Some("error"));
        assert!(!markdown.is_off("MD013"));
    }

    #[test]
    fn should_match_rule_codes_case_insensitively() {
        let parsed: Config = toml::from_str("[markdown.rules]\nmd009 = \"off\"\n").unwrap();
        let markdown = parsed.markdown.with_defaults();
        assert!(markdown.is_off("MD009"));
    }

    #[test]
    fn should_reject_an_unknown_markdown_severity() {
        let mut config = Config::default();
        config.markdown.rules.insert("MD009".to_owned(), "maybe".to_owned());
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("markdown.rules.MD009 must be one of off, warn or error"));
    }

    #[test]
    fn should_parse_rule_overrides() {
        let config: Config = toml::from_str("[rules]\nMADR03 = \"warn\"\n").unwrap();
        assert_eq!(config.rules.get("MADR03").map(String::as_str), Some("warn"));
    }

    #[test]
    fn should_reject_an_unknown_rule_severity() {
        let mut config = Config::default();
        config.rules.insert("MADR03".to_owned(), "maybe".to_owned());
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("rules.MADR03 must be one of off, warn or error"));
    }

    #[test]
    fn should_render_the_markdown_table() {
        let rendered = render(Path::new("docs/adr"), 1);
        assert!(rendered.contains("[markdown]"));
        assert!(rendered.contains("MD013 = \"off\""));
        assert!(rendered.contains("MD033 = \"off\""));
        let parsed: Config = toml::from_str(&rendered).unwrap();
        assert!(parsed.markdown.enabled);
        assert!(parsed.markdown.is_off("MD013"));
    }
}
