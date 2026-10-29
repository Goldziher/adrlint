#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }

    pub fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }
}

/// One finding, located by path, line and column, optionally with an end position and a hint.
///
/// `line` and `column` are 1-based; the column counts characters. `end_line` and `end_column` are
/// `Some` when the rule (or [`locate`]) knows the extent of the offending text; the end column is
/// exclusive. `help` is a short suggestion, and `fixable` says `adrlint lint --fix` can resolve the
/// finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub path: std::path::PathBuf,
    pub line: usize,
    pub column: usize,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
    pub help: Option<String>,
    pub fixable: bool,
}

impl Diagnostic {
    pub fn error(
        code: &'static str,
        path: impl Into<std::path::PathBuf>,
        line: usize,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            path: path.into(),
            line,
            column: 1,
            end_line: None,
            end_column: None,
            help: None,
            fixable: false,
        }
    }

    /// Sets the full span, with an exclusive end column.
    #[must_use]
    pub fn with_span(mut self, column: usize, end_line: usize, end_column: usize) -> Self {
        self.column = column;
        self.end_line = Some(end_line);
        self.end_column = Some(end_column);
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    #[must_use]
    pub fn with_fixable(mut self, fixable: bool) -> Self {
        self.fixable = fixable;
        self
    }
}

/// Orders findings by path, position, code and message, so output is identical across runs.
pub fn sort(diagnostics: &mut [Diagnostic]) {
    diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.line.cmp(&right.line))
            .then(left.column.cmp(&right.column))
            .then(left.code.cmp(right.code))
            .then(left.message.cmp(&right.message))
    });
}

pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| diagnostic.severity.is_error())
}

/// Fills in the span of every finding that only knows its line.
///
/// The span covers the trimmed text of the line, so a heading finding underlines the heading and a
/// list-marker finding (`MADR31`) underlines the marker. A finding whose line is outside `source`
/// keeps its position and gains no span.
pub fn locate(diagnostics: &mut [Diagnostic], source: &str) {
    if diagnostics.iter().all(|diagnostic| diagnostic.end_line.is_some()) {
        return;
    }
    let lines = source.lines().collect::<Vec<_>>();
    for diagnostic in diagnostics
        .iter_mut()
        .filter(|diagnostic| diagnostic.end_line.is_none())
    {
        let Some(text) = diagnostic.line.checked_sub(1).and_then(|index| lines.get(index)) else {
            continue;
        };
        let leading = text.chars().take_while(|character| character.is_whitespace()).count();
        let width = text.trim().chars().count();
        if width == 0 {
            continue;
        }
        let width = if diagnostic.code == "MADR31" { 1 } else { width };
        if diagnostic.column == 1 {
            diagnostic.column = leading + 1;
        }
        diagnostic.end_line = Some(diagnostic.line);
        diagnostic.end_column = Some(diagnostic.column + width);
    }
}

/// A short suggestion for a finding code, used when the rule did not attach a more specific one.
pub fn default_help(code: &str) -> Option<&'static str> {
    Some(match code {
        "MADR01a" => "start the file with a single `# Title` heading",
        "MADR01b" => "add a `## Context and Problem Statement` section",
        "MADR01c" => "add a `## Considered Options` section",
        "MADR01d" => "add a `## Decision Outcome` section",
        "MADR05" => "demote the heading to `##` or lower; only the title uses `#`",
        "MADR06" => "use the heading level the MADR template gives this section",
        "MADR07a" | "MADR07b" => "make it a `###` heading inside `## Decision Outcome`",
        "MADR08a" => "move the title to the first line after the front matter",
        "MADR08b" => "reorder the sections to follow the MADR template",
        "MADR31" => "use `*` as the list marker",
        "MADR41a" => "number the first ADR of the log with the configured `adr.start`",
        "MADR41b" => "number ADRs consecutively, or set `adr.allow_gaps = true`",
        "MADR42" => "move the file out of the ADR directory, or give it a `NNNN-` prefix",
        "MADR43" => "give each ADR in the directory its own number",
        "MADR44" => "name the file `NNNN-lowercase-words-separated-by-hyphens.md`",
        "ADR50" => "revert the edit, or run `adrlint unlock <path> --reason <text>` first",
        "ADR51" => "restore the file, or run `adrlint unlock <path> --reason <text>`",
        "ADR52" => "make the file readable UTF-8 Markdown",
        _ => return None,
    })
}

/// Attaches [`default_help`] to every finding that has no hint of its own.
pub fn add_default_help(diagnostics: &mut [Diagnostic]) {
    for diagnostic in diagnostics.iter_mut().filter(|diagnostic| diagnostic.help.is_none()) {
        diagnostic.help = default_help(diagnostic.code).map(str::to_owned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(code: &'static str, line: usize) -> Diagnostic {
        Diagnostic::error(code, "docs/adr/0001-x.md", line, "message")
    }

    #[test]
    fn should_locate_the_trimmed_text_of_the_line() {
        let mut findings = vec![at("MADR06", 2)];
        locate(&mut findings, "# Title\n  ## Heading  \n");
        assert_eq!(findings[0].column, 3);
        assert_eq!(findings[0].end_line, Some(2));
        assert_eq!(findings[0].end_column, Some(3 + "## Heading".len()));
    }

    #[test]
    fn should_locate_only_the_marker_for_a_list_finding() {
        let mut findings = vec![at("MADR31", 1)];
        locate(&mut findings, "- item\n");
        assert_eq!((findings[0].column, findings[0].end_column), (1, Some(2)));
    }

    #[test]
    fn should_keep_a_span_a_rule_already_set() {
        let mut findings = vec![at("MADR06", 1).with_span(4, 1, 6)];
        locate(&mut findings, "# Title\n");
        assert_eq!((findings[0].column, findings[0].end_column), (4, Some(6)));
    }

    #[test]
    fn should_leave_a_finding_outside_the_source_without_a_span() {
        let mut findings = vec![at("MADR06", 9)];
        locate(&mut findings, "# Title\n");
        assert_eq!(findings[0].end_line, None);
    }

    #[test]
    fn should_count_columns_in_characters() {
        let mut findings = vec![at("MADR06", 1)];
        locate(&mut findings, "## é\n");
        assert_eq!(findings[0].end_column, Some(5));
    }

    #[test]
    fn should_sort_by_path_then_position() {
        let mut findings = vec![at("MADR06", 3), at("MADR05", 1)];
        sort(&mut findings);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn should_add_help_without_overwriting() {
        let mut findings = vec![at("MADR31", 1), at("MADR05", 1).with_help("custom")];
        add_default_help(&mut findings);
        assert_eq!(findings[0].help.as_deref(), Some("use `*` as the list marker"));
        assert_eq!(findings[1].help.as_deref(), Some("custom"));
    }
}
