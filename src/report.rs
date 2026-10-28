use std::io::{self, IsTerminal, Write};

use clap::ValueEnum;
use owo_colors::OwoColorize;

use crate::diagnostics::Diagnostic;

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

fn print_text(writer: &mut impl Write, diagnostics: &[Diagnostic], color: bool) -> io::Result<()> {
    let color = color && io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
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
            diagnostic.path.display(),
            diagnostic.line,
            diagnostic.column,
            severity,
            code,
            diagnostic.message
        )?;
    }
    Ok(())
}

fn print_json(writer: &mut impl Write, diagnostics: &[Diagnostic]) -> io::Result<()> {
    let values = diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "code": diagnostic.code,
                "severity": diagnostic.severity.as_str(),
                "message": diagnostic.message,
                "path": diagnostic.path.display().to_string(),
                "line": diagnostic.line,
                "column": diagnostic.column,
            })
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
        writeln!(
            writer,
            "::{level} file={},line={},col={}::[{}] {}",
            escape(&diagnostic.path.display().to_string()),
            diagnostic.line,
            diagnostic.column,
            diagnostic.code,
            escape(&diagnostic.message)
        )?;
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value.replace('%', "%25").replace('\r', "%0D").replace('\n', "%0A")
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
    fn should_escape_github_commands() {
        let mut diagnostic = diagnostic();
        diagnostic.message = "50% done\nnow".to_owned();
        let mut output = Vec::new();
        print_github(&mut output, &[diagnostic]).unwrap();
        assert!(String::from_utf8(output).unwrap().contains("50%25 done%0Anow"));
    }
}
