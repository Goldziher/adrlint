//! The parsed ADR document: front matter, headings, blocks, links and list items.
//!
//! [`parse`] owns the only Markdown parse of an ADR. Rules, content hashing and formatting read
//! this model rather than re-reading or re-parsing the source.
//!
//! Every offset is a **byte** offset into the source handed to [`parse`], and every line number is
//! 1-based in that source. Front matter is stripped before the Markdown parse and its length is
//! added back to each offset, so a heading on line 6 reports line 6, not line 2 of the body.

mod front_matter;
mod parse;

pub use front_matter::{FrontMatter, FrontMatterError};
pub use parse::parse;

/// A Markdown ADR, parsed once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub front_matter: Option<FrontMatter>,
    /// Byte offset where the Markdown body starts: after the front matter, or `0` when there is
    /// none.
    pub body_start: usize,
    /// Headings in document order, each with the span of the section it owns.
    pub headings: Vec<Heading>,
    /// Block-level content in document order.
    pub blocks: Vec<Block>,
    /// Links in document order, including reference links with their resolved destination.
    pub links: Vec<Link>,
    /// List items in document order, flattened across nesting levels.
    pub list_items: Vec<ListItem>,
}

/// A heading and the section body it introduces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    /// Rendered inline text, with soft wraps collapsed to a space.
    pub text: String,
    /// 1-based line of the heading.
    pub line: usize,
    /// Byte offset of the heading's first character (`#` for ATX, text for setext).
    pub start: usize,
    /// Byte offset just past the heading's last character, excluding the line terminator.
    pub end: usize,
    /// Byte offset of the first byte after the heading's line.
    pub body_start: usize,
    /// Byte offset where the next heading of the same or higher level starts, or the end of the
    /// source.
    pub body_end: usize,
}

/// A link or image as written, with its destination resolved for reference links.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Link {
    pub kind: LinkKind,
    pub url: String,
    pub title: String,
    /// Rendered inline text of the link or image label.
    pub text: String,
    /// 1-based line the link opens on.
    pub line: usize,
    /// Byte offset of the opening `[`.
    pub start: usize,
    /// Byte offset just past the closing `)` or `]`.
    pub end: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LinkKind {
    #[default]
    Link,
    Image,
}

/// A single list item, flattened out of its list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListItem {
    /// The marker as written: `*`, `-`, `+`, or an ordered marker such as `1.` or `1)`.
    pub marker: String,
    pub ordered: bool,
    /// Nesting depth, `0` for a top-level list.
    pub depth: usize,
    /// Rendered inline text of the item, excluding nested items. Paragraphs inside the item are
    /// separated by a newline.
    pub text: String,
    /// 1-based line of the marker.
    pub line: usize,
    pub start: usize,
    /// Byte offset just past the item's last character, excluding the trailing line terminator.
    pub end: usize,
}

/// Block-level content.
///
/// Text is rendered inline content: emphasis markers are dropped, soft line breaks become a
/// single space, and hard line breaks stay as `\n` so they remain distinguishable. Inline HTML is
/// kept verbatim inside [`Block::Paragraph::text`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph {
        text: String,
        line: usize,
        start: usize,
        end: usize,
    },
    List {
        ordered: bool,
        /// First number of an ordered list, as written.
        start_number: Option<u64>,
        depth: usize,
        line: usize,
        start: usize,
        end: usize,
    },
    CodeBlock {
        /// Fenced info string, trimmed; empty for an indented block.
        language: String,
        /// The code exactly as parsed, including inner blank lines and the trailing newline.
        text: String,
        line: usize,
        start: usize,
        end: usize,
    },
    Table {
        header: Vec<String>,
        rows: Vec<Vec<String>>,
        line: usize,
        start: usize,
        end: usize,
    },
    /// A raw HTML block, with its own line breaks preserved.
    Html {
        text: String,
        line: usize,
        start: usize,
        end: usize,
    },
    /// A thematic break.
    Rule { line: usize, start: usize, end: usize },
}
