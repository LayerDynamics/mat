//! Render-time state types.
//!
//! There is no layered architecture — all cross-event state lives on a
//! single `RenderState`. Methods (write primitives, word-wrap, tags,
//! flushes) live in `renderer` so this file stays focused on the shape
//! of the state and its constructors.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;

use pulldown_cmark::Alignment;

use crate::style::StyleFlag;
use crate::terminal::TermConfig;

pub enum ListMarker {
    Unordered,
    Ordered(u64),
}

/// Tri-state counter of consecutive trailing `\n`s emitted at the current
/// cursor position. `ensure_blank_line` only needs to distinguish "none",
/// "one", and "two or more" — the exact count beyond two is irrelevant, so
/// the saturating-`u8`-with-magic-thresholds encoding collapses to three
/// named variants.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TrailingNewlines {
    Zero,
    One,
    Two,
}

impl TrailingNewlines {
    /// Monotonic bump after emitting one `\n`. Saturates at `Two` — the
    /// renderer never cares how many blank lines past two have been written.
    pub fn bump(self) -> Self {
        match self {
            Self::Zero => Self::One,
            Self::One | Self::Two => Self::Two,
        }
    }

    /// True when at least one blank line separates us from the previous
    /// non-blank content (i.e. ≥ 2 trailing newlines).
    pub fn has_blank_line(self) -> bool {
        matches!(self, Self::Two)
    }
}

pub struct TableState {
    pub aligns: Vec<Alignment>,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub current_row: Vec<String>,
    pub current_cell: String,
    pub in_header: bool,
}

pub struct RenderState<'a> {
    pub out: &'a mut dyn Write,
    pub term: &'a TermConfig,

    // layout / caret
    pub col_pos: usize,
    pub trailing_nl: TrailingNewlines,
    pub blockquote_depth: usize,
    pub list_stack: Vec<ListMarker>,

    // style
    pub style_stack: Vec<StyleFlag>,

    // code block
    pub in_code_block: bool,
    pub code_lang: String,
    pub code_buf: String,

    // link
    pub pending_link_url: Option<String>,
    /// Captures the textual display content rendered between `Tag::Link` start
    /// and end. Used at link close to suppress the `(url)` fallback when the
    /// display text is the same as the URL (autolinks, bare URLs).
    pub pending_link_text: Option<String>,
    /// Raw OSC 8 opener bytes (`\x1b]8;;URL\x1b\\`) queued at `Tag::Link` and
    /// flushed inside `emit_word` immediately before the first visible word of
    /// the link's display text. Deferring the opener keeps it on the same
    /// terminal line as its first token after a word-wrap, so terminals that
    /// treat newline as an implicit OSC 8 terminator still render the link as
    /// a single clickable region.
    pub pending_osc8_open: Option<String>,

    // image
    pub capturing_image: Option<String>,
    pub image_alt_buf: String,

    // table
    pub table: Option<TableState>,

    // footnotes
    pub footnote_index: HashMap<String, usize>,
    pub footnote_counter: usize,
    pub footnote_bodies: Vec<(String, String)>,
    pub capturing_footnote: Option<String>,
    pub footnote_capture_buf: String,

    // list item: bullet is deferred so a task-list marker can replace it.
    pub pending_bullet: Option<String>,

    /// Directory against which relative local-image paths (`![alt](x.png)`)
    /// are resolved. `None` when the document came from stdin with no
    /// additional context and no trusted filesystem root — in that case we
    /// refuse local-image access entirely because we cannot tell which
    /// directory tree is safe to read from.
    pub source_base: Option<PathBuf>,
}

impl<'a> RenderState<'a> {
    pub fn new(out: &'a mut dyn Write, term: &'a TermConfig) -> Self {
        Self {
            out,
            term,
            col_pos: 0,
            trailing_nl: TrailingNewlines::Two, // suppress leading blank lines
            blockquote_depth: 0,
            list_stack: Vec::new(),
            style_stack: Vec::new(),
            in_code_block: false,
            code_lang: String::new(),
            code_buf: String::new(),
            pending_link_url: None,
            pending_link_text: None,
            pending_osc8_open: None,
            capturing_image: None,
            image_alt_buf: String::new(),
            table: None,
            footnote_index: HashMap::new(),
            footnote_counter: 0,
            footnote_bodies: Vec::new(),
            capturing_footnote: None,
            footnote_capture_buf: String::new(),
            pending_bullet: None,
            source_base: None,
        }
    }

    /// Sets the directory for resolving relative local-image paths.
    /// Callers at the `render()` level derive this from the source file's
    /// parent via `utils::source_base_dir`. Builder-style so callers can
    /// keep the concise `RenderState::new(...)` for the default
    /// (no local-image) case used by tests.
    pub fn with_source_base(mut self, base: Option<PathBuf>) -> Self {
        self.source_base = base;
        self
    }
}
