//! YAML front matter extraction.
//!
//! Front matter is a `---` line at byte 0, the YAML document, and a closing `---` (or `...`)
//! line. A block without a closing fence is not front matter: it is a thematic break followed by
//! ordinary Markdown, which keeps a stray `---` from swallowing the document.

use saphyr::{LoadableYamlNode, YamlOwned};

/// The front matter block and the result of parsing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    /// Text between the fences, including the newline that precedes the closing fence.
    pub raw: String,
    /// The first YAML document, or `None` when the block is empty or invalid.
    pub value: Option<YamlOwned>,
    /// Why the YAML failed to parse, if it did.
    pub error: Option<FrontMatterError>,
    /// 1-based line of the opening fence.
    pub line: usize,
    /// 1-based line of the closing fence.
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatterError {
    pub message: String,
    /// 1-based line in the whole file, not in the block.
    pub line: usize,
}

/// Returns the front matter, if any, and the byte offset where the Markdown body starts.
pub(crate) fn extract(source: &str) -> (Option<FrontMatter>, usize) {
    let Some(first_end) = source.find('\n') else {
        return (None, 0);
    };
    if source[..first_end].trim_end() != "---" {
        return (None, 0);
    }

    let raw_start = first_end + 1;
    let mut line = 1;
    let mut cursor = raw_start;
    while cursor <= source.len() {
        let line_end = source[cursor..].find('\n').map_or(source.len(), |end| cursor + end);
        line += 1;

        // The fence must start at column 0: an indented `---` can be YAML block-scalar content.
        let fence = source[cursor..line_end].trim_end();
        if fence == "---" || fence == "..." {
            let raw = source[raw_start..cursor].to_owned();
            let (value, error) = parse_yaml(&raw);
            let body_start = if line_end < source.len() {
                line_end + 1
            } else {
                source.len()
            };
            return (
                Some(FrontMatter {
                    raw,
                    value,
                    error,
                    line: 1,
                    end_line: line,
                }),
                body_start,
            );
        }

        cursor = line_end + 1;
    }

    (None, 0)
}

/// The opening fence is always the first line, so the block's YAML starts on line 2.
fn parse_yaml(raw: &str) -> (Option<YamlOwned>, Option<FrontMatterError>) {
    if raw.trim().is_empty() {
        return (None, None);
    }

    match YamlOwned::load_from_str(raw) {
        Ok(documents) => (documents.into_iter().next(), None),
        Err(error) => (
            None,
            Some(FrontMatterError {
                message: error.info().to_owned(),
                line: 1 + error.marker().line(),
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::SafelyIndex;

    fn front_matter(source: &str) -> FrontMatter {
        extract(source).0.expect("expected front matter")
    }

    #[test]
    fn should_extract_and_parse_front_matter() {
        let source = "---\nstatus: accepted\ndate: 2024-01-02\n---\n# Title\n";
        let (extracted, body_start) = extract(source);
        let front_matter = extracted.expect("expected front matter");

        assert_eq!(front_matter.line, 1);
        assert_eq!(front_matter.end_line, 4);
        assert_eq!(front_matter.raw, "status: accepted\ndate: 2024-01-02\n");
        assert!(front_matter.error.is_none());
        assert_eq!(&source[body_start..], "# Title\n");

        let value = front_matter.value.as_ref().expect("expected a parsed value");
        assert_eq!(value.get("status").and_then(YamlOwned::as_str), Some("accepted"));
    }

    #[test]
    fn should_accept_a_dot_terminator() {
        let front_matter = front_matter("---\nstatus: accepted\n...\n# Title\n");
        assert_eq!(front_matter.end_line, 3);
        assert!(front_matter.error.is_none());
    }

    #[test]
    fn should_report_a_yaml_error_on_its_file_line() {
        let source = "---\ntitle: ok\n  bad: indent\n---\n";
        let (extracted, _) = extract(source);
        let front_matter = extracted.expect("expected front matter");

        assert!(front_matter.value.is_none());
        let error = front_matter.error.expect("expected a parse error");
        assert_eq!(error.line, 3);
        assert!(!error.message.is_empty());
    }

    #[test]
    fn should_accept_an_empty_block() {
        let front_matter = front_matter("---\n---\n# Title\n");
        assert_eq!(front_matter.raw, "");
        assert!(front_matter.value.is_none());
        assert!(front_matter.error.is_none());
    }

    #[test]
    fn should_not_close_the_block_on_an_indented_fence() {
        let source = "---\ndescription: |\n  ---\n  body\n---\n# Title\n";
        let (extracted, body_start) = extract(source);
        let front_matter = extracted.expect("expected front matter");

        assert_eq!(front_matter.end_line, 5);
        assert!(front_matter.error.is_none());
        assert_eq!(&source[body_start..], "# Title\n");
    }

    #[test]
    fn should_ignore_an_unterminated_fence() {
        assert_eq!(extract("---\ntitle: x\n"), (None, 0));
        assert_eq!(extract("Intro\n---\n"), (None, 0));
        assert_eq!(extract(""), (None, 0));
    }

    #[test]
    fn should_shift_lines_for_a_crlf_file() {
        let source = "---\r\nstatus: accepted\r\n---\r\n# Title\r\n";
        let (extracted, body_start) = extract(source);
        let front_matter = extracted.expect("expected front matter");

        assert_eq!(front_matter.end_line, 3);
        assert_eq!(&source[body_start..], "# Title\r\n");
    }
}
