//! rumdl diagnostics and fixes adapted to adrlint's finding model.
//!
//! One rule set is built per run from `[markdown]`, so `format` and `lint` agree about which rules
//! are active: a rule set to `off` produces no finding and no fix.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow};
use rumdl_lib::config::{Config as RumdlConfig, RuleConfig};
use rumdl_lib::document_run::DocumentRun;
use rumdl_lib::rule::{Rule, Severity as RumdlSeverity};
use rumdl_lib::rules::{all_rules, filter_rules};

use crate::config::MarkdownConfig;
use crate::diagnostics::{Diagnostic, Severity};

/// Code carried by a finding rumdl reports without a rule name.
const FALLBACK_CODE: &str = "MD000";

/// The fix loop is bounded: a rule set that keeps rewriting the same buffer is a rumdl defect, and
/// formatting must terminate rather than spin.
const MAX_FIX_ITERATIONS: usize = 5;

/// A rumdl rule set configured from `[markdown]`, ready for one document run.
struct Engine {
    config: RumdlConfig,
    /// Every rumdl rule name, interned to the `&'static str` a [`Diagnostic`] carries.
    codes: BTreeMap<String, &'static str>,
    rules: Vec<Box<dyn Rule>>,
}

impl Engine {
    fn new(markdown: &MarkdownConfig) -> Self {
        let mut config = RumdlConfig::default();
        // MADR31 requires `*`, so the formatter must not pick `-` as its consistent style.
        config.rules.insert(
            "MD004".to_owned(),
            RuleConfig {
                severity: None,
                values: BTreeMap::from([("style".to_owned(), toml::Value::String("asterisk".to_owned()))]),
            },
        );
        config.global.disable = markdown
            .rules
            .iter()
            .filter(|(_, setting)| setting.trim().eq_ignore_ascii_case("off"))
            .map(|(code, _)| code.clone())
            .collect();

        let all = all_rules(&config);
        let codes = all.iter().map(|rule| (rule.name().to_owned(), rule.name())).collect();
        let rules = filter_rules(&all, &config.global);
        Self { config, codes, rules }
    }
}

/// Every code a Markdown finding can carry: rumdl's rule names, plus the fallback for a warning
/// that names no rule. Built once from rumdl's own rule list, so it follows a rumdl upgrade.
pub fn rule_codes() -> &'static BTreeSet<String> {
    static CODES: OnceLock<BTreeSet<String>> = OnceLock::new();
    CODES.get_or_init(|| {
        all_rules(&RumdlConfig::default())
            .iter()
            .map(|rule| rule.name().to_ascii_uppercase())
            .chain([FALLBACK_CODE.to_owned()])
            .collect()
    })
}

/// The engine built for one rule configuration, reused while the configuration stays the same.
type CachedEngine = ((bool, BTreeMap<String, String>), Rc<Engine>);

thread_local! {
    /// The engine last built on this thread, keyed by the `[markdown]` settings it was built from.
    ///
    /// Building the rule set costs more than linting a small ADR, so a worker thread builds it once
    /// and reuses it for every file it handles.
    static ENGINE: RefCell<Option<CachedEngine>> = const { RefCell::new(None) };
}

fn engine(markdown: &MarkdownConfig) -> Rc<Engine> {
    ENGINE.with(|cell| {
        let mut cached = cell.borrow_mut();
        let key = (markdown.enabled, markdown.rules.clone());
        if let Some((cached_key, engine)) = cached.as_ref()
            && *cached_key == key
        {
            return Rc::clone(engine);
        }
        let engine = Rc::new(Engine::new(markdown));
        *cached = Some((key, Rc::clone(&engine)));
        engine
    })
}

/// Lints one ADR with rumdl, mapping every warning to an adrlint finding.
///
/// Rules disabled in `markdown.rules` are absent from the rule set, and a finding is filtered again
/// by code so a rumdl default cannot resurrect a rule the configuration turned off.
pub fn check(source: &str, path: &Path, markdown: &MarkdownConfig) -> Vec<Diagnostic> {
    if !markdown.enabled {
        return Vec::new();
    }

    let engine = engine(markdown);
    let analysis = match DocumentRun::new(source, &engine.rules, &engine.config)
        .file_path(path)
        .analyze()
    {
        Ok(analysis) => analysis,
        Err(error) => {
            return vec![Diagnostic::error(
                FALLBACK_CODE,
                path,
                1,
                format!("rumdl could not analyse the document: {error}"),
            )];
        }
    };

    analysis
        .warnings
        .into_iter()
        .filter_map(|warning| {
            let code = warning
                .rule_name
                .as_deref()
                .and_then(|name| engine.codes.get(name).copied())
                .unwrap_or(FALLBACK_CODE);
            if markdown.is_off(code) {
                return None;
            }
            let fixable = warning.fix.is_some();
            let mut diagnostic = Diagnostic {
                code,
                severity: severity(markdown, code, warning.severity),
                message: warning.message,
                path: path.to_path_buf(),
                line: warning.line,
                column: warning.column,
                end_line: None,
                end_column: None,
                help: None,
                fixable,
            };
            if warning.end_line >= warning.line && warning.end_column > 0 {
                diagnostic.end_line = Some(warning.end_line);
                diagnostic.end_column = Some(warning.end_column);
            }
            Some(diagnostic)
        })
        .collect()
}

/// Applies rumdl's fixes to one ADR, returning the document in its own line-ending convention.
///
/// A document nothing changed in comes back byte-identical, which keeps `format` idempotent.
pub fn format(source: &str, path: &Path, markdown: &MarkdownConfig) -> Result<String> {
    if !markdown.enabled {
        return Ok(source.to_owned());
    }

    let engine = engine(markdown);
    let (fixed, _) = DocumentRun::new(source, &engine.rules, &engine.config)
        .file_path(path)
        .fix(MAX_FIX_ITERATIONS)
        .map_err(|error| anyhow!("{error}"))
        .with_context(|| format!("formatting {}", path.display()))?;
    Ok(fixed)
}

/// Maps rumdl's severity.
///
/// An enabled Markdown rule fails `lint` by default, because the Markdown layer is part of the gate.
/// `[markdown.rules] <CODE> = "warn"` downgrades it; rumdl's `Info` findings stay warnings.
fn severity(markdown: &MarkdownConfig, code: &str, severity: RumdlSeverity) -> Severity {
    match markdown.setting(code).map(str::to_ascii_lowercase).as_deref() {
        Some("warn") => Severity::Warning,
        Some("error") => Severity::Error,
        _ => match severity {
            RumdlSeverity::Info => Severity::Warning,
            RumdlSeverity::Error | RumdlSeverity::Warning => Severity::Error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MarkdownConfig {
        MarkdownConfig::default()
    }

    #[test]
    fn should_report_trailing_spaces_as_md009() {
        let findings = check("# Title   \n\nBody.\n", Path::new("0001-test.md"), &config());
        assert!(
            findings.iter().any(|finding| finding.code == "MD009"),
            "expected MD009, got {:?}",
            findings.iter().map(|finding| finding.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn should_report_a_missing_final_newline_as_md047() {
        let findings = check("# Title", Path::new("0001-test.md"), &config());
        assert!(findings.iter().any(|finding| finding.code == "MD047"));
    }

    #[test]
    fn should_ignore_long_lines_by_default() {
        let source = format!("# Title\n\n{}\n", "word ".repeat(40));
        let findings = check(&source, Path::new("0001-test.md"), &config());
        assert!(!findings.iter().any(|finding| finding.code == "MD013"));
    }

    #[test]
    fn should_report_nothing_when_markdown_is_disabled() {
        let markdown = MarkdownConfig {
            enabled: false,
            ..MarkdownConfig::default()
        };
        assert!(check("# Title  \n", Path::new("0001-test.md"), &markdown).is_empty());
    }

    #[test]
    fn should_leave_the_source_untouched_when_markdown_is_disabled() {
        let markdown = MarkdownConfig {
            enabled: false,
            ..MarkdownConfig::default()
        };
        let source = "# Title  \n";
        let fixed = format(source, Path::new("0001-test.md"), &markdown).unwrap();
        assert_eq!(fixed, source);
    }

    #[test]
    fn should_fix_and_be_idempotent() {
        let source = "# Title   \n\nBody.   ";
        let path = Path::new("0001-test.md");
        let fixed = format(source, path, &config()).unwrap();
        assert_eq!(fixed, "# Title\n\nBody.\n");
        assert_eq!(format(&fixed, path, &config()).unwrap(), fixed);
    }

    #[test]
    fn should_preserve_crlf_line_endings() {
        let source = "# Title   \r\n\r\nBody.\r\n";
        let fixed = format(source, Path::new("0001-test.md"), &config()).unwrap();
        assert_eq!(fixed, "# Title\r\n\r\nBody.\r\n");
    }

    #[test]
    fn should_honour_a_rule_turned_off_in_configuration() {
        let mut markdown = MarkdownConfig::default();
        markdown.rules.insert("MD009".to_owned(), "off".to_owned());
        let findings = check("# Title   \n\nBody.\n", Path::new("0001-test.md"), &markdown);
        assert!(!findings.iter().any(|finding| finding.code == "MD009"));

        let fixed = format("# Title   \n\nBody.\n", Path::new("0001-test.md"), &markdown).unwrap();
        assert!(fixed.starts_with("# Title   \n"));
    }

    #[test]
    fn should_default_enabled_markdown_rules_to_errors() {
        let findings = check("# Title   \n\nBody.\n", Path::new("0001-test.md"), &config());
        let finding = findings
            .iter()
            .find(|finding| finding.code == "MD009")
            .expect("MD009 finding");
        assert_eq!(finding.severity, Severity::Error);
    }

    #[test]
    fn should_let_configuration_downgrade_a_markdown_finding_to_a_warning() {
        let mut markdown = MarkdownConfig::default();
        markdown.rules.insert("MD009".to_owned(), "warn".to_owned());
        let findings = check("# Title   \n\nBody.\n", Path::new("0001-test.md"), &markdown);
        let finding = findings
            .iter()
            .find(|finding| finding.code == "MD009")
            .expect("MD009 finding");
        assert_eq!(finding.severity, Severity::Warning);
    }

    #[test]
    fn should_normalise_list_markers_to_asterisks() {
        let source = "# Title\n\n- one\n- two\n";
        let fixed = format(source, Path::new("0001-test.md"), &config()).unwrap();
        assert!(fixed.contains("* one"), "{fixed}");
        assert!(fixed.contains("* two"), "{fixed}");
    }
}
