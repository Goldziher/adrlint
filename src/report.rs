use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use clap::ValueEnum;
use owo_colors::OwoColorize;

use crate::diagnostics::Diagnostic;

/// Files larger than this are not re-read to show a source line under a finding.
const MAX_SNIPPET_SOURCE_BYTES: u64 = 4 * 1024 * 1024;

/// Source lines longer than this (in characters) are not echoed under a finding.
const MAX_SNIPPET_CHARS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Github,
}

pub fn print(diagnostics: &[Diagnostic], format: OutputFormat, color: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    match format {
        OutputFormat::Text => print_text(&mut stdout, diagnostics, color),
        OutputFormat::Json => print_json(&mut stdout, diagnostics),
        OutputFormat::Github => print_github(&mut stdout, diagnostics),
    }
}

/// Counts for the closing line of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub errors: usize,
    pub warnings: usize,
    pub fixable: usize,
    pub files: usize,
}

pub fn summarize(diagnostics: &[Diagnostic], files: usize) -> Summary {
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity.is_error())
        .count();
    Summary {
        errors,
        warnings: diagnostics.len() - errors,
        fixable: diagnostics.iter().filter(|diagnostic| diagnostic.fixable).count(),
        files,
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let files = if self.files == 1 { "file" } else { "files" };
        if self.errors == 0 && self.warnings == 0 {
            return write!(
                formatter,
                "adrlint: no problems found in {} {files} checked",
                self.files
            );
        }
        let errors = if self.errors == 1 { "error" } else { "errors" };
        let warnings = if self.warnings == 1 { "warning" } else { "warnings" };
        write!(
            formatter,
            "adrlint: {} {errors}, {} {warnings} in {} {files} checked",
            self.errors, self.warnings, self.files
        )?;
        if self.fixable > 0 {
            write!(formatter, " ({} fixable with `adrlint lint --fix`)", self.fixable)?;
        }
        Ok(())
    }
}

/// The lines of the files findings point into, read at most once each.
#[derive(Default)]
struct Sources {
    files: HashMap<PathBuf, Option<Vec<String>>>,
}

impl Sources {
    fn line(&mut self, diagnostic: &Diagnostic) -> Option<&str> {
        let lines = self
            .files
            .entry(diagnostic.path.clone())
            .or_insert_with(|| read_lines(&diagnostic.path))
            .as_ref()?;
        let index = diagnostic.line.checked_sub(1)?;
        lines.get(index).map(String::as_str)
    }
}

fn read_lines(path: &std::path::Path) -> Option<Vec<String>> {
    if fs::metadata(path).ok()?.len() > MAX_SNIPPET_SOURCE_BYTES {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    Some(text.lines().map(str::to_owned).collect())
}

fn print_text(writer: &mut impl Write, diagnostics: &[Diagnostic], color: bool) -> io::Result<()> {
    let color = color && io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let mut sources = Sources::default();
    for diagnostic in diagnostics {
        let severity = match (color, diagnostic.severity.is_error()) {
            (true, true) => diagnostic.severity.as_str().red().bold().to_string(),
            (true, false) => diagnostic.severity.as_str().yellow().bold().to_string(),
            (false, _) => diagnostic.severity.as_str().to_owned(),
        };
        let code = if color {
            format!("[{}]", diagnostic.code).dimmed().to_string()
        } else {
            format!("[{}]", diagnostic.code)
        };
        writeln!(
            writer,
            "{}:{}:{}: {} {} {}",
            printable(&diagnostic.path.display().to_string()),
            diagnostic.line,
            diagnostic.column,
            severity,
            code,
            printable(&diagnostic.message)
        )?;

        let number = diagnostic.line.to_string();
        let pad = " ".repeat(number.len());
        let mut annotated = false;
        if diagnostic.end_line.is_some()
            && let Some(text) = sources.line(diagnostic)
            && text.chars().count() <= MAX_SNIPPET_CHARS
        {
            let shown = printable(&text.replace('\t', " ")).into_owned();
            let width = text.chars().count();
            let start = diagnostic.column.max(1);
            let end = match (diagnostic.end_line, diagnostic.end_column) {
                (Some(line), Some(end)) if line == diagnostic.line => end,
                _ => width + 1,
            };
            let carets = "^".repeat(end.saturating_sub(start).max(1));
            let carets = match (color, diagnostic.severity.is_error()) {
                (true, true) => carets.red().bold().to_string(),
                (true, false) => carets.yellow().bold().to_string(),
                (false, _) => carets,
            };
            writeln!(writer, "{pad} |")?;
            writeln!(writer, "{number} | {shown}")?;
            writeln!(writer, "{pad} | {}{carets}", " ".repeat(start - 1))?;
            annotated = true;
        }
        if let Some(help) = &diagnostic.help {
            let lead = if annotated { pad.as_str() } else { " " };
            writeln!(writer, "{lead} = help: {}", printable(help))?;
        }
    }
    Ok(())
}

/// `text` with every control character replaced by U+FFFD, one character for one so a caret
/// under the line still lines up.
///
/// ADR text is untrusted: an escape sequence or a carriage return echoed to a terminal or a CI log
/// could clear the screen or forge a line of output.
fn printable(text: &str) -> Cow<'_, str> {
    if text.chars().any(char::is_control) {
        Cow::Owned(
            text.chars()
                .map(|character| if character.is_control() { '\u{fffd}' } else { character })
                .collect(),
        )
    } else {
        Cow::Borrowed(text)
    }
}

fn print_json(writer: &mut impl Write, diagnostics: &[Diagnostic]) -> io::Result<()> {
    let values = diagnostics
        .iter()
        .map(|diagnostic| {
            let mut value = serde_json::json!({
                "code": diagnostic.code,
                "severity": diagnostic.severity.as_str(),
                "message": diagnostic.message,
                "path": diagnostic.path.display().to_string(),
                "line": diagnostic.line,
                "column": diagnostic.column,
                "fixable": diagnostic.fixable,
            });
            if let Some(object) = value.as_object_mut() {
                if let Some(end_line) = diagnostic.end_line {
                    object.insert("end_line".to_owned(), end_line.into());
                }
                if let Some(end_column) = diagnostic.end_column {
                    object.insert("end_column".to_owned(), end_column.into());
                }
                if let Some(help) = &diagnostic.help {
                    object.insert("help".to_owned(), help.as_str().into());
                }
            }
            value
        })
        .collect::<Vec<_>>();
    serde_json::to_writer_pretty(&mut *writer, &values)?;
    writeln!(writer)
}

fn print_github(writer: &mut impl Write, diagnostics: &[Diagnostic]) -> io::Result<()> {
    for diagnostic in diagnostics {
        let level = if diagnostic.severity.is_error() {
            "error"
        } else {
            "warning"
        };
        let mut location = format!(
            "file={},line={},col={}",
            escape_property(&diagnostic.path.display().to_string()),
            diagnostic.line,
            diagnostic.column
        );
        if let Some(end_line) = diagnostic.end_line {
            location.push_str(&format!(",endLine={end_line}"));
        }
        if let Some(end_column) = diagnostic.end_column {
            location.push_str(&format!(",endColumn={end_column}"));
        }
        let mut message = format!("[{}] {}", diagnostic.code, diagnostic.message);
        if let Some(help) = &diagnostic.help {
            message.push_str(&format!(" (help: {help})"));
        }
        writeln!(writer, "::{level} {location}::{}", escape(&message))?;
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
}

/// Escapes a workflow-command property value, which also may not hold a raw `:` or `,`.
fn escape_property(value: &str) -> String {
    escape(value).replace(':', "%3A").replace(',', "%2C")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Severity;

    fn diagnostic() -> Diagnostic {
        Diagnostic {
            code: "MADR42",
            severity: Severity::Error,
            message: "non-ADR file".to_owned(),
            path: "docs/adr/notes.txt".into(),
            line: 1,
            column: 1,
            end_line: None,
            end_column: None,
            help: None,
            fixable: false,
        }
    }

    #[test]
    fn should_render_text_diagnostics() {
        let mut output = Vec::new();
        print_text(&mut output, &[diagnostic()], false).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "docs/adr/notes.txt:1:1: error [MADR42] non-ADR file\n"
        );
    }

    #[test]
    fn should_render_github_annotations() {
        let mut output = Vec::new();
        print_github(&mut output, &[diagnostic()]).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "::error file=docs/adr/notes.txt,line=1,col=1::[MADR42] non-ADR file\n"
        );
    }

    #[test]
    fn should_escape_a_github_property_value() {
        let mut diagnostic = diagnostic();
        diagnostic.path = "docs/adr/a,b:c.md".into();
        let mut output = Vec::new();
        print_github(&mut output, &[diagnostic]).unwrap();
        assert!(
            String::from_utf8(output)
                .unwrap()
                .contains("file=docs/adr/a%2Cb%3Ac.md,")
        );
    }

    #[test]
    fn should_escape_github_commands() {
        let mut diagnostic = diagnostic();
        diagnostic.message = "50% done\nnow".to_owned();
        let mut output = Vec::new();
        print_github(&mut output, &[diagnostic]).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("50%25 done%0Anow"));
    }

    #[test]
    fn should_render_a_source_line_with_a_caret_and_help() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("0001-x.md");
        std::fs::write(&path, "# Title\n\n- item\n").unwrap();
        let finding = Diagnostic::error("MADR31", &path, 3, "bad marker")
            .with_span(1, 3, 2)
            .with_help("use `*`");
        let mut output = Vec::new();
        print_text(&mut output, &[finding], false).unwrap();
        let text = String::from_utf8(output).unwrap();
        let tail = text.split_once('\n').unwrap().1;
        assert_eq!(tail, "  |\n3 | - item\n  | ^\n  = help: use `*`\n");
    }

    #[test]
    fn should_not_echo_terminal_control_characters() {
        let directory = tempfile::TempDir::new().unwrap();
        let path = directory.path().join("0001-x.md");
        std::fs::write(&path, "- item \u{1b}[2J\rforged\u{7}\n").unwrap();
        let finding = Diagnostic::error("MADR31", &path, 1, "heading '\u{1b}]0;x\u{7}'").with_span(1, 1, 2);
        let mut output = Vec::new();
        print_text(&mut output, &[finding], false).unwrap();
        let text = String::from_utf8(output).unwrap();
        assert!(
            !text
                .chars()
                .any(|character| character.is_control() && character != '\n'),
            "{text:?}"
        );
        assert!(
            text.contains("1 | - item \u{fffd}[2J\u{fffd}forged\u{fffd}\n"),
            "{text:?}"
        );
        assert!(text.contains("heading '\u{fffd}]0;x\u{fffd}'"), "{text:?}");
    }

    #[test]
    fn should_render_help_without_a_readable_source() {
        let finding = Diagnostic::error("MADR42", "missing/dir/notes.txt", 1, "foreign").with_help("move it");
        let mut output = Vec::new();
        print_text(&mut output, &[finding], false).unwrap();
        assert!(String::from_utf8(output).unwrap().ends_with("\n  = help: move it\n"));
    }

    #[test]
    fn should_add_the_span_and_help_to_github_annotations() {
        let finding = diagnostic().with_span(2, 3, 9).with_help("fix it");
        let mut output = Vec::new();
        print_github(&mut output, &[finding]).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            "::error file=docs/adr/notes.txt,line=1,col=2,endLine=3,endColumn=9::[MADR42] non-ADR file (help: fix it)\n"
        );
    }

    #[test]
    fn should_add_fields_to_json_without_removing_any() {
        let mut output = Vec::new();
        print_json(
            &mut output,
            &[diagnostic().with_span(1, 1, 4).with_help("h").with_fixable(true)],
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
        let first = &value[0];
        assert_eq!(first["code"], "MADR42");
        assert_eq!(first["line"], 1);
        assert_eq!(first["end_line"], 1);
        assert_eq!(first["end_column"], 4);
        assert_eq!(first["help"], "h");
        assert_eq!(first["fixable"], true);
    }

    #[test]
    fn should_summarize_counts_by_severity() {
        let mut warning = diagnostic();
        warning.severity = Severity::Warning;
        let findings = [diagnostic().with_fixable(true), warning];
        let summary = summarize(&findings, 3);
        assert_eq!(
            summary.to_string(),
            "adrlint: 1 error, 1 warning in 3 files checked (1 fixable with `adrlint lint --fix`)"
        );
        assert_eq!(
            summarize(&[], 1).to_string(),
            "adrlint: no problems found in 1 file checked"
        );
    }
}
