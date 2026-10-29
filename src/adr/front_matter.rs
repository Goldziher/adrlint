//! YAML front matter extraction.
//!
//! Front matter is a `---` line at byte 0, the YAML document, and a closing `---` (or `...`)
//! line. A block without a closing fence is not front matter: it is a thematic break followed by
//! ordinary Markdown, which keeps a stray `---` from swallowing the document.

use saphyr::{LoadableYamlNode, YamlOwned};

/// The largest front matter block parsed, in bytes. A huge block is rejected before it reaches the
/// YAML parser so a single ADR cannot exhaust memory.
const MAX_BLOCK_BYTES: usize = 64 * 1024;

/// The deepest YAML indentation accepted, in levels. Nested block mappings recurse in the parser,
/// so a hostile document must be rejected before it overflows the stack.
const MAX_INDENT_DEPTH: usize = 64;

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
    // A UTF-8 byte-order mark shifts every byte offset by three. Strip it once for detection and
    // keep working in original coordinates, so returned offsets and line numbers stay valid.
    let bom = source
        .strip_prefix('\u{feff}')
        .map_or(0, |rest| source.len() - rest.len());
    let content = &source[bom..];

    let Some(first_end) = content.find('\n') else {
        return (None, bom);
    };
    if content[..first_end].trim_end() != "---" {
        return (None, bom);
    }

    let raw_start = bom + first_end + 1;
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

    (None, bom)
}

/// Parses the block, rejecting input that would exhaust the parser: an oversized block or one
/// nested deeper than [`MAX_INDENT_DEPTH`].
///
/// The opening fence is always the first line, so the block's YAML starts on line 2. Both limits
/// are reported as ordinary [`FrontMatterError`]s so `MADR21` reports them instead of the process
/// aborting.
fn parse_yaml(raw: &str) -> (Option<YamlOwned>, Option<FrontMatterError>) {
    if raw.trim().is_empty() {
        return (None, None);
    }
    if raw.len() > MAX_BLOCK_BYTES {
        return (
            None,
            Some(FrontMatterError {
                message: format!(
                    "front matter is {} bytes; the limit is {MAX_BLOCK_BYTES} bytes",
                    raw.len()
                ),
                line: 2,
            }),
        );
    }
    if let Some((depth, line)) = excessive_indentation(raw) {
        return (
            None,
            Some(FrontMatterError {
                message: format!(
                    "front matter nests {depth} levels deep on line {line}; the limit is {MAX_INDENT_DEPTH} levels"
                ),
                line: 1 + line,
            }),
        );
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

/// The first line whose indentation introduces more than [`MAX_INDENT_DEPTH`] nesting levels, and
/// that depth. Indentation is tracked as a stack of distinct widths, so sibling keys at the same
/// depth do not accumulate.
fn excessive_indentation(raw: &str) -> Option<(usize, usize)> {
    let mut levels: Vec<usize> = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let trimmed = line.trim_start_matches(' ');
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - trimmed.len();
        while levels.last().is_some_and(|level| *level >= indent) {
            levels.pop();
        }
        levels.push(indent);
        let depth = levels.len() - 1;
        if depth > MAX_INDENT_DEPTH {
            return Some((depth, index + 1));
        }
    }
    None
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

    #[test]
    fn should_strip_a_leading_byte_order_mark() {
        let source = "\u{feff}---\nstatus: accepted\n---\n# Title\n";
        let (extracted, body_start) = extract(source);
        let front_matter = extracted.expect("expected front matter");

        assert_eq!(front_matter.line, 1);
        assert_eq!(front_matter.end_line, 3);
        assert!(front_matter.error.is_none());
        assert_eq!(body_start, source.find("# Title").unwrap());
        assert_eq!(&source[body_start..], "# Title\n");
    }

    #[test]
    fn should_return_the_body_after_a_bom_without_front_matter() {
        let source = "\u{feff}# Title\n";
        let (extracted, body_start) = extract(source);
        assert!(extracted.is_none());
        assert_eq!(body_start, source.find('#').unwrap());
    }

    #[test]
    fn should_reject_an_oversized_front_matter_block() {
        let block = "key: value\n".repeat(7000);
        let source = format!("---\n{block}---\n# Title\n");
        let (extracted, body_start) = extract(&source);
        let front_matter = extracted.expect("expected front matter");

        assert!(front_matter.value.is_none());
        let error = front_matter.error.expect("expected a parse error");
        assert_eq!(error.line, 2);
        assert!(error.message.contains("limit"), "{}", error.message);
        assert_eq!(&source[body_start..], "# Title\n");
    }

    #[test]
    fn should_reject_deeply_nested_front_matter_without_aborting() {
        let mut block = String::new();
        for level in 0..200 {
            block.push_str(&"  ".repeat(level));
            block.push_str(&format!("key{level}:\n"));
        }
        let source = format!("---\n{block}---\n# Title\n");
        let (extracted, _) = extract(&source);
        let front_matter = extracted.expect("expected front matter");

        assert!(front_matter.value.is_none());
        let error = front_matter.error.expect("expected a parse error");
        assert!(error.message.contains("levels"), "{}", error.message);
        assert!(error.line > 1, "expected a line inside the block");
    }
}
