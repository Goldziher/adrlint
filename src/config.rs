use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::lock::RootEscape;

pub const CONFIG_FILE_NAMES: [&str; 2] = [".adrlint.toml", "adrlint.toml"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub adr: AdrConfig,
    #[serde(default)]
    pub lock: LockConfig,
    #[serde(default)]
    pub lint: LintConfig,
    #[serde(default)]
    pub markdown: MarkdownConfig,
    #[serde(default)]
    pub rules: BTreeMap<String, String>,
}

/// Opt-in lint checks that need more than the ADR files themselves.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintConfig {
    /// Whether `MADR11` checks that external links resolve. Not implemented yet: enabling it is a
    /// configuration error rather than a silent no-op.
    #[serde(default)]
    pub check_external_links: bool,
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
    /// Additional roots, each a directory or a glob pattern relative to the project root.
    ///
    /// When non-empty this replaces `dir` as the set of roots. It is a whole list: a layer that
    /// sets it replaces the list below rather than folding into it.
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    #[serde(default = "default_start")]
    pub start: u32,
    #[serde(default)]
    pub allow_gaps: bool,
    /// Whether the roots are searched for nested ADR logs, as `--recursive` does.
    #[serde(default)]
    pub recursive: bool,
    /// Whether a configuration layer named `dir`. Without one, a missing `docs/adr` is not an error:
    /// the project is searched for ADR logs instead.
    #[serde(skip)]
    pub dir_configured: bool,
}

impl Default for AdrConfig {
    fn default() -> Self {
        Self {
            dir: default_dir(),
            paths: Vec::new(),
            start: default_start(),
            allow_gaps: false,
            recursive: false,
            dir_configured: false,
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
/// defaults are applied when the configuration is built and merged under whatever the user wrote, so
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

/// One configuration file before layering.
///
/// Every field is optional so an absent key differs from one explicitly set to its default.
/// [`Layer::apply_to`] replaces the keys a layer contains and leaves the rest alone, which is what
/// makes a nested file override only what it names.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Layer {
    #[serde(default)]
    adr: AdrLayer,
    #[serde(default)]
    lock: LockLayer,
    #[serde(default)]
    lint: LintLayer,
    #[serde(default)]
    markdown: MarkdownLayer,
    #[serde(default)]
    rules: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdrLayer {
    #[serde(default)]
    dir: Option<PathBuf>,
    #[serde(default)]
    paths: Option<Vec<PathBuf>>,
    #[serde(default)]
    start: Option<u32>,
    #[serde(default)]
    allow_gaps: Option<bool>,
    #[serde(default)]
    recursive: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LockLayer {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    statuses: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LintLayer {
    #[serde(default)]
    check_external_links: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkdownLayer {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    rules: BTreeMap<String, String>,
}

impl Layer {
    fn apply_to(&self, config: &mut Config) {
        if let Some(dir) = &self.adr.dir {
            config.adr.dir = dir.clone();
            config.adr.dir_configured = true;
        }
        // `paths` is a whole list: a layer that sets it replaces the list above it.
        if let Some(paths) = &self.adr.paths {
            config.adr.paths = paths.clone();
        }
        if let Some(start) = self.adr.start {
            config.adr.start = start;
        }
        if let Some(allow_gaps) = self.adr.allow_gaps {
            config.adr.allow_gaps = allow_gaps;
        }
        if let Some(recursive) = self.adr.recursive {
            config.adr.recursive = recursive;
        }

        if let Some(enabled) = self.lock.enabled {
            config.lock.enabled = enabled;
        }
        if let Some(path) = &self.lock.path {
            config.lock.path = path.clone();
        }
        // `statuses` is a whole list, not a fold: a layer that sets it replaces the list above it.
        if let Some(statuses) = &self.lock.statuses {
            config.lock.statuses = statuses.clone();
        }

        if let Some(check) = self.lint.check_external_links {
            config.lint.check_external_links = check;
        }

        if let Some(enabled) = self.markdown.enabled {
            config.markdown.enabled = enabled;
        }
        for (code, setting) in &self.markdown.rules {
            config
                .markdown
                .rules
                .insert(code.trim().to_ascii_uppercase(), setting.clone());
        }
        for (code, setting) in &self.rules {
            config.rules.insert(code.clone(), setting.clone());
        }
    }
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub root: PathBuf,
    pub path: Option<PathBuf>,
}

/// Loads the configuration with the documented precedence, reading the global file from
/// `dirs::config_dir()`.
pub fn load(explicit: Option<&Path>) -> Result<Loaded> {
    let global = dirs::config_dir().map(|directory| directory.join("adrlint").join("config.toml"));
    load_with_global(explicit, global.as_deref())
}

/// Loads the configuration with an explicit global path.
///
/// This is the testable form of [`load`]: `global` is the user configuration file, or `None` to skip
/// it. Tests point it at an isolated file so they never read the developer's real global config.
///
/// Precedence, highest first: the explicit `--config` file, the nearest project file and its
/// ancestors outward, `global`, then built-in defaults.
pub fn load_with_global(explicit: Option<&Path>, global: Option<&Path>) -> Result<Loaded> {
    if let Some(path) = explicit {
        let root = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let mut config = Config::default();
        if let Some(global) = global {
            apply_file(&mut config, global, false)?;
        }
        apply_file(&mut config, path, true)?;
        validate(&config).with_context(|| format!("invalid configuration in {}", path.display()))?;
        return Ok(Loaded {
            config,
            root: absolute(root),
            path: Some(path.to_path_buf()),
        });
    }

    let cwd = std::env::current_dir().context("reading the current directory")?;
    let mut config = Config::default();
    if let Some(global) = global {
        apply_file(&mut config, global, false)?;
    }

    // Nearest first. One file per directory: the preferred name wins outright and the deprecated
    // fallback is not merged in.
    let mut project = Vec::new();
    for ancestor in cwd.ancestors() {
        for name in CONFIG_FILE_NAMES {
            let candidate = ancestor.join(name);
            if candidate.is_file() {
                project.push(candidate);
                break;
            }
        }
    }
    // Apply from the outermost ancestor inward so the nearest file wins.
    for path in project.iter().rev() {
        apply_file(&mut config, path, true)?;
    }

    let path = project.first().cloned();
    let root = path
        .as_ref()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .unwrap_or(cwd);
    validate(&config).with_context(|| match &path {
        Some(path) => format!("invalid configuration in {}", path.display()),
        None => "invalid configuration".to_owned(),
    })?;
    Ok(Loaded {
        config,
        root: absolute(root),
        path,
    })
}

/// Reads one file and folds it into `config`. A missing optional file is skipped; a missing required
/// file is an error.
fn apply_file(config: &mut Config, path: &Path, required: bool) -> Result<()> {
    if !required && !path.is_file() {
        return Ok(());
    }
    let contents =
        fs::read_to_string(path).with_context(|| format!("reading configuration from {}", path.display()))?;
    let layer: Layer =
        toml::from_str(&contents).with_context(|| format!("parsing configuration from {}", path.display()))?;
    layer.apply_to(config);
    Ok(())
}

/// Canonicalises a root when possible; a root that does not exist is kept as given.
fn absolute(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

fn validate(config: &Config) -> Result<()> {
    if !matches!(config.adr.start, 0 | 1) {
        bail!("adr.start must be 0 or 1, found {}", config.adr.start);
    }
    validate_dir(&config.adr.dir)?;
    for path in &config.adr.paths {
        validate_relative(path, "adr.paths")?;
    }
    validate_relative(&config.lock.path, "lock.path")?;
    if config.lint.check_external_links {
        bail!(
            "lint.check_external_links is not supported yet: external link checking (MADR11) is not implemented; remove the key or set it to false"
        );
    }
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
    if path.as_os_str().is_empty() {
        bail!("{field} must not be empty");
    }
    match crate::lock::escape_from_root(&path.to_string_lossy()) {
        Some(RootEscape::Absolute | RootEscape::Drive) => {
            bail!("{field} must be relative to the configuration file")
        }
        Some(RootEscape::Parent) => bail!("{field} must not escape the project root"),
        None => Ok(()),
    }
}

pub fn render(dir: &Path, start: u32) -> String {
    let dir = toml::Value::String(render_dir(dir)).to_string();
    format!(
        r#"# adrlint configuration
#
# The ADR directory, relative to this file. ADR files are `NNNN-brief-description.md`.
[adr]
dir = {dir}
# Additional roots, relative to this file: directories or glob patterns such as "adrs/*".
# When non-empty they replace `dir`. Pair them with `recursive` (or `--recursive`) to include
# nested ADR directories, and address one namespace with e.g. `adrlint lint adrs/team-a`.
# With neither `dir` nor `paths` configured and no `docs/adr`, the project is searched instead.
paths = []
# Search the roots for nested ADR logs, as `--recursive` does.
recursive = false
# 0 or 1. The base is fixed for the life of the directory; do not change it later.
start = {start}
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
    )
}

/// The ADR directory as it goes into the configuration file.
///
/// On Windows the separator is `\` and the file uses `/`; elsewhere a backslash is a literal
/// character in a directory name and must survive the round-trip.
fn render_dir(dir: &Path) -> String {
    let text = dir.to_string_lossy();
    if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Folds one TOML document over the built-in defaults, exactly as a single config file would be.
    fn merged(source: &str) -> Config {
        let mut config = Config::default();
        let layer: Layer = toml::from_str(source).unwrap();
        layer.apply_to(&mut config);
        config
    }

    #[test]
    fn should_accept_a_lint_table_with_external_links_disabled() {
        let config = merged("[lint]\ncheck_external_links = false\n");
        assert!(!config.lint.check_external_links);
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn should_reject_external_link_checking_until_it_is_implemented() {
        let config = merged("[lint]\ncheck_external_links = true\n");
        assert!(config.lint.check_external_links);
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("not supported yet"), "{error}");
    }

    #[test]
    fn should_reject_unknown_lint_keys() {
        assert!(toml::from_str::<Layer>("[lint]\nbogus = true\n").is_err());
    }

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
    fn should_reject_windows_absolute_paths_on_every_platform() {
        for (path, expected) in [
            (r"C:\adr", "must be relative"),
            ("C:/adr", "must be relative"),
            (r"\\host\share\adr", "must be relative"),
            (r"\adr", "must be relative"),
            (r"docs\..\..\adr", "must not escape"),
        ] {
            let error = validate_dir(Path::new(path)).unwrap_err().to_string();
            assert!(error.contains(expected), "{path}: {error}");
            let config = Config {
                lock: LockConfig {
                    path: PathBuf::from(path),
                    ..LockConfig::default()
                },
                ..Config::default()
            };
            let error = validate(&config).unwrap_err().to_string();
            assert!(error.contains(expected), "{path}: {error}");
        }
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
        let config = merged("[markdown.rules]\nMD009 = \"off\"\n");
        assert_eq!(config.markdown.setting("MD009"), Some("off"));
        assert_eq!(config.markdown.setting("MD013"), Some("off"));
        assert_eq!(config.markdown.setting("MD033"), Some("off"));
    }

    #[test]
    fn should_let_an_explicit_setting_override_a_default() {
        let config = merged("[markdown.rules]\nMD013 = \"error\"\n");
        assert_eq!(config.markdown.setting("MD013"), Some("error"));
        assert!(!config.markdown.is_off("MD013"));
    }

    #[test]
    fn should_match_rule_codes_case_insensitively() {
        let config = merged("[markdown.rules]\nmd009 = \"off\"\n");
        assert!(config.markdown.is_off("MD009"));
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
        let config = merged("[rules]\nMADR03 = \"warn\"\n");
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

    #[test]
    fn should_escape_the_rendered_directory() {
        let dir = Path::new("weird \"quoted\" \\ plans");
        let rendered = render(dir, 1);
        let parsed: Config = toml::from_str(&rendered).expect("the rendered configuration must parse as TOML");
        assert_eq!(parsed.adr.dir, PathBuf::from(render_dir(dir)));
        validate(&parsed).expect("the rendered configuration must validate");

        let both = Path::new("quote \"and\" 'apostrophe'");
        let rendered = render(both, 1);
        let parsed: Config = toml::from_str(&rendered).expect("the rendered configuration must parse");
        assert_eq!(parsed.adr.dir, PathBuf::from(render_dir(both)));
        validate(&parsed).expect("the rendered configuration must validate");
    }

    #[test]
    fn should_replace_a_list_rather_than_fold_it() {
        let mut config = Config::default();
        let layer: Layer = toml::from_str("[lock]\nstatuses = [\"proposed\"]\n").unwrap();
        layer.apply_to(&mut config);
        assert_eq!(config.lock.statuses, vec!["proposed".to_owned()]);
    }

    #[test]
    fn should_default_to_no_additional_adr_paths() {
        assert!(AdrConfig::default().paths.is_empty());
        let config = merged("[adr]\ndir = \"adrs\"\n");
        assert!(config.adr.paths.is_empty(), "paths stays empty until a layer sets it");
    }

    #[test]
    fn should_replace_additional_adr_paths_rather_than_fold_them() {
        let mut config = Config::default();
        let outer: Layer = toml::from_str("[adr]\npaths = [\"adrs/*\", \"product/adrs/*\"]\n").unwrap();
        outer.apply_to(&mut config);
        assert_eq!(
            config.adr.paths,
            vec![PathBuf::from("adrs/*"), PathBuf::from("product/adrs/*")]
        );

        let inner: Layer = toml::from_str("[adr]\npaths = [\"docs/adr\"]\n").unwrap();
        inner.apply_to(&mut config);
        assert_eq!(
            config.adr.paths,
            vec![PathBuf::from("docs/adr")],
            "a layer that sets paths replaces the whole list"
        );

        // An empty list is a value too: it restores the `dir` default rather than inheriting.
        let cleared: Layer = toml::from_str("[adr]\npaths = []\n").unwrap();
        cleared.apply_to(&mut config);
        assert!(config.adr.paths.is_empty());
    }

    #[test]
    fn should_reject_an_absolute_additional_adr_path() {
        let config = Config {
            adr: AdrConfig {
                paths: vec![std::env::temp_dir().join("adr")],
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("adr.paths must be relative"), "{error}");
    }

    #[test]
    fn should_reject_an_additional_adr_path_escaping_the_project_root() {
        let config = Config {
            adr: AdrConfig {
                paths: vec![PathBuf::from("adrs/../outside")],
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("adr.paths must not escape"), "{error}");
    }

    #[test]
    fn should_reject_an_empty_additional_adr_path() {
        let config = Config {
            adr: AdrConfig {
                paths: vec![PathBuf::new()],
                ..AdrConfig::default()
            },
            ..Config::default()
        };
        let error = validate(&config).unwrap_err().to_string();
        assert!(error.contains("adr.paths must not be empty"), "{error}");
    }

    #[test]
    fn should_render_the_additional_paths_key() {
        let rendered = render(Path::new("docs/adr"), 1);
        assert!(rendered.contains("paths = []"));
        assert!(rendered.contains("--recursive"));
        let parsed: Config = toml::from_str(&rendered).expect("the rendered configuration must parse");
        assert!(parsed.adr.paths.is_empty());
    }
}
