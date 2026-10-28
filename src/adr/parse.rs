//! The single Markdown parse of an ADR.
//!
//! `pulldown-cmark` runs with offset iterators enabled, so every event carries a byte range.
//! Front matter is removed before the parse and the body's byte offset is added back to each
//! range, which keeps offsets and lines pointing into the original file exactly once.

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::front_matter::extract;
use super::{Block, Document, FrontMatter, Heading, Link, LinkKind, ListItem};

/// Parses an ADR source into its document model.
pub fn parse(source: &str) -> Document {
    let (front_matter, body_start) = extract(source);
    let mut builder = Builder::new(source, body_start);
    builder.run();
    builder.finish(front_matter)
}

/// An open container. Text is routed to the innermost frame of each kind, so a nested list item
/// does not leak its text into its parent and a link's label does not leak into a sibling.
enum Frame {
    Paragraph {
        start: usize,
        line: usize,
        text: String,
    },
    Heading {
        level: u8,
        start: usize,
        line: usize,
        text: String,
    },
    Code {
        start: usize,
        line: usize,
        language: String,
        text: String,
    },
    Html {
        start: usize,
        line: usize,
        text: String,
    },
    List {
        /// Index of the block already pushed for this list, whose end is filled in at `End`.
        block: usize,
        ordered: bool,
    },
    Item {
        index: usize,
    },
    Table {
        start: usize,
        line: usize,
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        row: Vec<String>,
        in_header: bool,
    },
    Cell {
        text: String,
    },
    Link {
        kind: LinkKind,
        start: usize,
        end: usize,
        line: usize,
        url: String,
        title: String,
        text: String,
    },
    Other,
}

struct Builder<'a> {
    source: &'a str,
    base: usize,
    lines: LineIndex,
    frames: Vec<Frame>,
    document: Document,
}

impl<'a> Builder<'a> {
    fn new(source: &'a str, base: usize) -> Self {
        Self {
            source,
            base,
            lines: LineIndex::new(source),
            frames: Vec::new(),
            document: Document::default(),
        }
    }

    fn run(&mut self) {
        let source = self.source;
        let body = &source[self.base..];
        let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
        for (event, range) in Parser::new_ext(body, options).into_offset_iter() {
            self.event(event, range);
        }
    }

    fn finish(mut self, front_matter: Option<FrontMatter>) -> Document {
        let source = self.source;
        let source_len = source.len();

        let mut bodies = Vec::with_capacity(self.document.headings.len());
        for (index, heading) in self.document.headings.iter().enumerate() {
            let body_start = source[heading.end..]
                .find('\n')
                .map_or(source_len, |offset| heading.end + offset + 1);
            let body_end = self.document.headings[index + 1..]
                .iter()
                .find(|next| next.level <= heading.level)
                .map_or(source_len, |next| next.start);
            bodies.push((body_start, body_end));
        }
        for (heading, (body_start, body_end)) in self.document.headings.iter_mut().zip(bodies) {
            heading.body_start = body_start;
            heading.body_end = body_end;
        }

        self.document.body_start = self.base;
        self.document.front_matter = front_matter;
        self.document
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        let start = self.base + range.start;
        let end = self.base + range.end;
        match event {
            Event::Start(tag) => self.start(tag, start, end),
            Event::End(tag) => self.end(tag, end),
            Event::Text(text) => self.push_text(&text),
            Event::Code(code) => self.push_text(&code),
            Event::InlineHtml(html) => self.push_text(&html),
            Event::Html(html) => self.push_html(&html),
            Event::InlineMath(math) | Event::DisplayMath(math) => self.push_text(&math),
            Event::SoftBreak => self.push_text(" "),
            Event::HardBreak => self.push_text("\n"),
            Event::FootnoteReference(label) => self.push_text(&format!("[^{label}]")),
            Event::TaskListMarker(checked) => self.push_text(if checked { "[x] " } else { "[ ] " }),
            Event::Rule => {
                let end = self.trimmed_end(end);
                self.document.blocks.push(Block::Rule {
                    line: self.lines.line(start),
                    start,
                    end,
                });
            }
        }
    }

    fn start(&mut self, tag: Tag<'_>, start: usize, end: usize) {
        let line = self.lines.line(start);
        match tag {
            Tag::Paragraph => self.start_paragraph(start, line),
            Tag::Heading { level, .. } => self.start_heading(level, start, line),
            Tag::CodeBlock(kind) => self.start_code(kind, start, line),
            Tag::HtmlBlock => self.start_html(start, line),
            Tag::List(number) => self.start_list(number, start, line),
            Tag::Item => self.start_item(start, line),
            Tag::Table(_) => self.start_table(start, line),
            Tag::TableHead => self.set_table_header(true),
            Tag::TableRow => {}
            Tag::TableCell => self.frames.push(Frame::Cell { text: String::new() }),
            Tag::Link { dest_url, title, .. } => self.start_link(
                LinkKind::Link,
                dest_url.to_string(),
                title.to_string(),
                start,
                end,
                line,
            ),
            Tag::Image { dest_url, title, .. } => self.start_link(
                LinkKind::Image,
                dest_url.to_string(),
                title.to_string(),
                start,
                end,
                line,
            ),
            Tag::BlockQuote(_)
            | Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Superscript
            | Tag::Subscript
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::MetadataBlock(_) => self.frames.push(Frame::Other),
        }
    }

    fn start_paragraph(&mut self, start: usize, line: usize) {
        self.frames.push(Frame::Paragraph {
            start,
            line,
            text: String::new(),
        });
    }

    fn start_heading(&mut self, level: HeadingLevel, start: usize, line: usize) {
        self.frames.push(Frame::Heading {
            level: level_of(level),
            start,
            line,
            text: String::new(),
        });
    }

    fn start_code(&mut self, kind: CodeBlockKind<'_>, start: usize, line: usize) {
        let language = match kind {
            CodeBlockKind::Fenced(info) => info.trim().to_owned(),
            CodeBlockKind::Indented => String::new(),
        };
        self.frames.push(Frame::Code {
            start,
            line,
            language,
            text: String::new(),
        });
    }

    fn start_html(&mut self, start: usize, line: usize) {
        self.frames.push(Frame::Html {
            start,
            line,
            text: String::new(),
        });
    }

    fn start_list(&mut self, start_number: Option<u64>, start: usize, line: usize) {
        let block = self.document.blocks.len();
        self.document.blocks.push(Block::List {
            ordered: start_number.is_some(),
            start_number,
            depth: self.list_depth(),
            line,
            start,
            end: start,
        });
        self.frames.push(Frame::List {
            block,
            ordered: start_number.is_some(),
        });
    }

    fn start_item(&mut self, start: usize, line: usize) {
        let index = self.document.list_items.len();
        self.document.list_items.push(ListItem {
            marker: self.marker(start),
            ordered: self.enclosing_list_ordered(),
            depth: self.list_depth().saturating_sub(1),
            text: String::new(),
            line,
            start,
            end: start,
        });
        self.frames.push(Frame::Item { index });
    }

    fn start_table(&mut self, start: usize, line: usize) {
        self.frames.push(Frame::Table {
            start,
            line,
            header: Vec::new(),
            rows: Vec::new(),
            row: Vec::new(),
            in_header: false,
        });
    }

    fn start_link(&mut self, kind: LinkKind, url: String, title: String, start: usize, end: usize, line: usize) {
        self.frames.push(Frame::Link {
            kind,
            start,
            end,
            line,
            url,
            title,
            text: String::new(),
        });
    }

    fn end(&mut self, tag: TagEnd, end: usize) {
        match tag {
            TagEnd::Paragraph => self.end_paragraph(end),
            TagEnd::Heading(_) => self.end_heading(end),
            TagEnd::CodeBlock => self.end_code(end),
            TagEnd::HtmlBlock => self.end_html(end),
            TagEnd::Item => self.end_item(end),
            TagEnd::List(_) => self.end_list(end),
            TagEnd::TableHead => self.set_table_header(false),
            TagEnd::TableCell => self.end_cell(),
            TagEnd::TableRow => self.end_row(),
            TagEnd::Table => self.end_table(end),
            TagEnd::Link => self.end_link(end),
            TagEnd::Image => self.end_link(end),
            _ => {
                self.pop();
            }
        }
    }

    fn end_paragraph(&mut self, end: usize) {
        let Frame::Paragraph { start, line, text } = self.pop() else {
            return;
        };
        let end = self.trimmed_end(end);
        self.separate_item_paragraphs();
        self.document.blocks.push(Block::Paragraph {
            text: text.trim_end().to_owned(),
            line,
            start,
            end,
        });
    }

    fn end_heading(&mut self, end: usize) {
        let Frame::Heading {
            level,
            start,
            line,
            text,
        } = self.pop()
        else {
            return;
        };
        let end = self.trimmed_end(end);
        self.document.headings.push(Heading {
            level,
            text: text.trim().to_owned(),
            line,
            start,
            end,
            body_start: 0,
            body_end: 0,
        });
    }

    fn end_code(&mut self, end: usize) {
        let Frame::Code {
            start,
            line,
            language,
            text,
        } = self.pop()
        else {
            return;
        };
        let end = self.trimmed_end(end);
        self.document.blocks.push(Block::CodeBlock {
            language,
            text,
            line,
            start,
            end,
        });
    }

    fn end_html(&mut self, end: usize) {
        let Frame::Html { start, line, text } = self.pop() else {
            return;
        };
        let end = self.trimmed_end(end);
        self.document.blocks.push(Block::Html { text, line, start, end });
    }

    fn end_item(&mut self, end: usize) {
        let Frame::Item { index } = self.pop() else {
            return;
        };
        let end = self.trimmed_end(end);
        if let Some(item) = self.document.list_items.get_mut(index) {
            item.text = item.text.trim_end().to_owned();
            item.end = end;
        }
    }

    fn end_list(&mut self, end: usize) {
        let Frame::List { block, .. } = self.pop() else {
            return;
        };
        let end = self.trimmed_end(end);
        if let Some(Block::List { end: block_end, .. }) = self.document.blocks.get_mut(block) {
            *block_end = end;
        }
    }

    fn set_table_header(&mut self, header: bool) {
        if let Some(Frame::Table { in_header, .. }) = self.last_table() {
            *in_header = header;
        }
    }

    fn end_cell(&mut self) {
        let Frame::Cell { text } = self.pop() else {
            return;
        };
        let Some(Frame::Table {
            header, row, in_header, ..
        }) = self.last_table()
        else {
            return;
        };
        if *in_header {
            header.push(text);
        } else {
            row.push(text);
        }
    }

    fn end_row(&mut self) {
        let Some(Frame::Table { rows, row, .. }) = self.last_table() else {
            return;
        };
        rows.push(std::mem::take(row));
    }

    fn end_table(&mut self, end: usize) {
        let Frame::Table {
            start,
            line,
            header,
            rows,
            ..
        } = self.pop()
        else {
            return;
        };
        let end = self.trimmed_end(end);
        self.document.blocks.push(Block::Table {
            header,
            rows,
            line,
            start,
            end,
        });
    }

    fn end_link(&mut self, _end: usize) {
        let Frame::Link {
            kind,
            start,
            end,
            line,
            url,
            title,
            text,
        } = self.pop()
        else {
            return;
        };
        self.document.links.push(Link {
            kind,
            url,
            title,
            text: text.trim().to_owned(),
            line,
            start,
            end: self.trimmed_end(end),
        });
    }

    fn pop(&mut self) -> Frame {
        self.frames.pop().unwrap_or(Frame::Other)
    }

    /// Sends a rendered inline chunk to the innermost paragraph-like sink, the innermost list
    /// item and the innermost link, so nested containers do not duplicate it outwards.
    fn push_text(&mut self, chunk: &str) {
        let mut item_seen = false;
        let mut block_seen = false;
        let mut link_seen = false;
        for frame in self.frames.iter_mut().rev() {
            match frame {
                Frame::Item { index } if !item_seen => {
                    item_seen = true;
                    if let Some(item) = self.document.list_items.get_mut(*index) {
                        item.text.push_str(chunk);
                    }
                }
                Frame::Paragraph { text, .. }
                | Frame::Heading { text, .. }
                | Frame::Cell { text }
                | Frame::Code { text, .. }
                    if !block_seen =>
                {
                    block_seen = true;
                    text.push_str(chunk);
                }
                Frame::Link { text, .. } if !link_seen => {
                    link_seen = true;
                    text.push_str(chunk);
                }
                _ => {}
            }
        }
    }

    fn push_html(&mut self, html: &str) {
        let Some(Frame::Html { text, .. }) = self
            .frames
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame, Frame::Html { .. }))
        else {
            self.push_text(html);
            return;
        };
        text.push_str(html);
    }

    /// A loose list item holds one paragraph frame per paragraph; join them with a newline so the
    /// item text stays readable and hash-stable.
    fn separate_item_paragraphs(&mut self) {
        let index = self.frames.iter().rev().find_map(|frame| match frame {
            Frame::Item { index } => Some(*index),
            _ => None,
        });
        let Some(item) = index.and_then(|index| self.document.list_items.get_mut(index)) else {
            return;
        };
        if !item.text.is_empty() && !item.text.ends_with('\n') {
            item.text.push('\n');
        }
    }

    fn last_table(&mut self) -> Option<&mut Frame> {
        self.frames
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame, Frame::Table { .. }))
    }

    fn list_depth(&self) -> usize {
        self.frames
            .iter()
            .filter(|frame| matches!(frame, Frame::List { .. }))
            .count()
    }

    fn enclosing_list_ordered(&self) -> bool {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::List { ordered, .. } => Some(*ordered),
                _ => None,
            })
            .unwrap_or(false)
    }

    /// The marker as written, read from the item's own byte range rather than from the rendered
    /// text, because pulldown-cmark reports only whether a list is ordered.
    fn marker(&self, start: usize) -> String {
        let line_end = self.source[start..]
            .find('\n')
            .map_or(self.source.len(), |offset| start + offset);
        let line = self.source[start..line_end].trim_start_matches([' ', '\t']);
        let bytes = line.as_bytes();

        if matches!(bytes.first().copied(), Some(b'*' | b'-' | b'+')) {
            return line[..1].to_owned();
        }
        let digits = bytes.iter().take_while(|byte| byte.is_ascii_digit()).count();
        if digits > 0 && matches!(bytes.get(digits).copied(), Some(b'.' | b')')) {
            return line[..digits + 1].to_owned();
        }
        String::new()
    }

    /// Drops the `\r\n`, `\n` or `\r` that ends a block's source range.
    fn trimmed_end(&self, mut end: usize) -> usize {
        while end > 0 && matches!(self.source.as_bytes()[end - 1], b'\n' | b'\r') {
            end -= 1;
        }
        end
    }
}

fn level_of(level: HeadingLevel) -> u8 {
    level as u8
}

/// Byte offsets of every line start, for O(log n) line lookup. Lines are 1-based.
struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(
            source
                .bytes()
                .enumerate()
                .filter(|(_, byte)| *byte == b'\n')
                .map(|(index, _)| index + 1),
        );
        Self { starts }
    }

    fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|start| *start <= offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::Block;

    #[test]
    fn should_parse_atx_and_setext_headings() {
        let source = "# Title\n\nIntro\n\nSub\n===\n\n## Section ##\n\nbody\n";
        let document = parse(source);

        let headings = document
            .headings
            .iter()
            .map(|heading| (heading.level, heading.text.as_str(), heading.line))
            .collect::<Vec<_>>();
        assert_eq!(headings, [(1, "Title", 1), (1, "Sub", 5), (2, "Section", 8)]);

        for heading in &document.headings {
            assert!(heading.start < heading.end);
            assert!(source.is_char_boundary(heading.start) && source.is_char_boundary(heading.end));
        }
        assert_eq!(&source[document.headings[0].start..document.headings[0].end], "# Title");
        assert_eq!(
            &source[document.headings[2].start..document.headings[2].end],
            "## Section ##"
        );
        assert_eq!(
            &source[document.headings[1].start..document.headings[1].end],
            "Sub\n==="
        );
    }

    #[test]
    fn should_span_a_heading_body_to_the_next_same_or_higher_heading() {
        let source = "# Title\n\n## One\n\ntext\n\n### Deep\n\ndeep text\n\n## Two\n\nend\n";
        let document = parse(source);
        let one = &document.headings[1];
        let deep = &document.headings[2];
        let two = &document.headings[3];

        assert_eq!(document.headings[0].body_start, 8);
        assert_eq!(
            document.headings[0].body_end,
            source.len(),
            "no later level 1 heading closes the title's body"
        );
        assert_eq!(one.body_end, two.start);
        assert_eq!(
            deep.body_end, two.start,
            "a level 3 section closes at the next level 2 heading"
        );
        assert_eq!(two.body_end, source.len());

        let one_body = &source[one.body_start..one.body_end];
        assert!(one_body.contains("text"));
        assert!(one_body.contains("### Deep"));
        assert!(!one_body.contains("## Two"));
    }

    #[test]
    fn should_normalise_soft_breaks_and_keep_hard_breaks() {
        let source = "hard  \nbreak\nsoft\nbreak\n";
        let document = parse(source);

        let Block::Paragraph { text, line, .. } = &document.blocks[0] else {
            panic!("expected a paragraph, found {:?}", document.blocks[0]);
        };
        assert_eq!(text, "hard\nbreak soft break");
        assert_eq!(*line, 1);
    }

    #[test]
    fn should_record_nested_list_markers() {
        let source = "* one\n  - two\n    1. three\n+ four\n";
        let document = parse(source);

        let items = document
            .list_items
            .iter()
            .map(|item| {
                (
                    item.marker.as_str(),
                    item.ordered,
                    item.depth,
                    item.text.as_str(),
                    item.line,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            items,
            [
                ("*", false, 0, "one", 1),
                ("-", false, 1, "two", 2),
                ("1.", true, 2, "three", 3),
                ("+", false, 0, "four", 4),
            ]
        );
        assert_eq!(&source[document.list_items[2].start..], "1. three\n+ four\n");
    }

    #[test]
    fn should_record_ordered_list_numbers() {
        let source = "3. three\n4. four\n";
        let document = parse(source);

        let Block::List {
            ordered,
            start_number,
            depth,
            ..
        } = &document.blocks[0]
        else {
            panic!("expected a list, found {:?}", document.blocks[0]);
        };
        assert!(*ordered);
        assert_eq!(*start_number, Some(3));
        assert_eq!(*depth, 0);
        assert_eq!(document.list_items[1].marker, "4.");
    }

    #[test]
    fn should_preserve_fenced_code_including_blank_lines() {
        let source = "```rust\nfn main() {\n\n    println!(\"hi\");\n}\n```\n";
        let document = parse(source);

        let Block::CodeBlock {
            language, text, line, ..
        } = &document.blocks[0]
        else {
            panic!("expected a code block, found {:?}", document.blocks[0]);
        };
        assert_eq!(language, "rust");
        assert_eq!(text, "fn main() {\n\n    println!(\"hi\");\n}\n");
        assert_eq!(*line, 1);
    }

    #[test]
    fn should_record_indented_code_blocks() {
        let source = "text\n\n    indented();\n";
        let document = parse(source);

        let Block::CodeBlock { language, text, .. } = &document.blocks[1] else {
            panic!("expected a code block, found {:?}", document.blocks[1]);
        };
        assert_eq!(language, "");
        assert_eq!(text, "indented();\n");
    }

    #[test]
    fn should_record_inline_and_reference_links() {
        let source =
            "See [inline](https://example.com \"Title\") and [reference][id].\n\n[id]: https://example.org/docs\n";
        let document = parse(source);

        let links = document
            .links
            .iter()
            .map(|link| (link.url.as_str(), link.title.as_str(), link.text.as_str(), link.line))
            .collect::<Vec<_>>();
        assert_eq!(
            links,
            [
                ("https://example.com", "Title", "inline", 1),
                ("https://example.org/docs", "", "reference", 1),
            ]
        );
        assert_eq!(
            &source[document.links[0].start..document.links[0].end],
            "[inline](https://example.com \"Title\")"
        );
        assert_eq!(
            &source[document.links[1].start..document.links[1].end],
            "[reference][id]"
        );
    }

    #[test]
    fn should_record_image_destinations() {
        let source = "![diagram](docs/diagram.png \"Architecture\")\n";
        let document = parse(source);

        assert_eq!(document.links.len(), 1);
        let image = &document.links[0];
        assert_eq!(image.kind, LinkKind::Image);
        assert_eq!(image.url, "docs/diagram.png");
        assert_eq!(image.title, "Architecture");
        assert_eq!(image.text, "diagram");
        assert_eq!(image.line, 1);
    }

    #[test]
    fn should_keep_raw_html() {
        let source = "Use <kbd>Ctrl</kbd> here.\n\n<div>\nraw\n</div>\n";
        let document = parse(source);

        let Block::Paragraph { text, .. } = &document.blocks[0] else {
            panic!("expected a paragraph, found {:?}", document.blocks[0]);
        };
        assert_eq!(text, "Use <kbd>Ctrl</kbd> here.");

        let Block::Html { text, line, .. } = &document.blocks[1] else {
            panic!("expected an HTML block, found {:?}", document.blocks[1]);
        };
        assert_eq!(text, "<div>\nraw\n</div>\n");
        assert_eq!(*line, 3);
    }

    #[test]
    fn should_parse_tables() {
        let source = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let document = parse(source);

        let Block::Table { header, rows, .. } = &document.blocks[0] else {
            panic!("expected a table, found {:?}", document.blocks[0]);
        };
        assert_eq!(header, &["a", "b"]);
        assert_eq!(rows, &vec![vec!["1".to_owned(), "2".to_owned()]]);
    }

    #[test]
    fn should_record_thematic_breaks() {
        let source = "text\n\n---\n\nmore\n";
        let document = parse(source);

        let Block::Rule { line, start, .. } = &document.blocks[1] else {
            panic!("expected a thematic break, found {:?}", document.blocks[1]);
        };
        assert_eq!(*line, 3);
        assert_eq!(&source[*start..*start + 3], "---");
    }

    #[test]
    fn should_shift_front_matter_offsets_once() {
        let source = "---\nstatus: accepted\n---\n# Title\n\nBody\n";
        let document = parse(source);
        let heading = &document.headings[0];

        assert!(document.front_matter.is_some());
        assert_eq!(document.body_start, source.find("# Title").unwrap());
        assert_eq!(heading.line, 4);
        assert_eq!(heading.start, source.find("# Title").unwrap());
        assert_eq!(&source[heading.start..heading.end], "# Title");
        assert_eq!(&source[heading.body_start..heading.body_end], "\nBody\n");
    }

    #[test]
    fn should_report_byte_offsets_after_multibyte_text() {
        let source = "émoji 🚀\n\n# Heading\n\n- ünïcode\n";
        let document = parse(source);
        let heading = &document.headings[0];
        let item = &document.list_items[0];

        assert_eq!(heading.start, source.find('#').unwrap());
        assert_eq!(&source[heading.start..heading.end], "# Heading");
        assert_eq!(heading.line, 3);
        assert_eq!(item.start, source.find('-').unwrap());
        assert_eq!(&source[item.start..item.end], "- ünïcode");
        assert_eq!(item.text, "ünïcode");
    }
}
