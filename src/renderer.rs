//! RenderState behavior — raw-output primitives, word-wrap, style stack,
//! event dispatch, and the table / code-block / footnote / image flushers.
//!
//! Everything here is `impl RenderState<'a>` — the state types themselves
//! live in `state`. Splitting the declaration from the behavior keeps each
//! file under the ~600-line rule-of-thumb from `docs/FolderStructure.md`.

use std::io;
use std::path::PathBuf;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Tag, TagEnd};
use syntect::easy::HighlightLines;
use syntect::util::as_24_bit_terminal_escaped;
use tempfile::NamedTempFile;
use unicode_width::UnicodeWidthStr;

use crate::format::{border, heading_level_num, pad_cell};
use crate::image::{fetch_remote_image_to_temp, image_max_height_scale};
use crate::resolve::resolve_local_image_path;
use crate::sanitize::{sanitize_code_lang, sanitize_display_text, sanitize_osc_url};
use crate::state::{ListMarker, RenderState, TableState, TrailingNewlines};
use crate::style::{StyleFlag, style_flag_ansi};
use crate::terminal::ImageProtocol;
use crate::utils::{highlight_theme, link_text_equals_url, syntax_set};

impl<'a> RenderState<'a> {
    /// Single source of truth for "write blockquote bars followed by N
    /// two-space list-indent pads." Both `write_parent_indent` and
    /// `write_indent_raw` delegate here — the only axis that varies between
    /// them is how many list-pads to emit relative to the depth of
    /// `list_stack`.
    pub fn write_indent_prefix(&mut self, list_pads: usize) -> io::Result<()> {
        for _ in 0..self.blockquote_depth {
            if self.term.color_enabled {
                self.out.write_all(b"\x1b[2m\xE2\x94\x82\x1b[0m ")?;
            } else {
                self.out.write_all(b"| ")?;
            }
            self.col_pos += 2;
            self.trailing_nl = TrailingNewlines::Zero;
        }
        for _ in 0..list_pads {
            self.out.write_all(b"  ")?;
            self.col_pos += 2;
            self.trailing_nl = TrailingNewlines::Zero;
        }
        Ok(())
    }

    /// Write the indentation prefix for every nesting level **above** the
    /// current list item — i.e. active blockquote bars plus one two-space
    /// indent per *parent* list. The current list item's own indent and its
    /// bullet are emitted separately by `flush_pending_bullet`, so this
    /// helper deliberately stops one level short of `list_stack.len()`.
    /// Callers that need the *full* indent (continuation lines inside
    /// wrapped text, code blocks, etc.) use `write_indent_raw` instead.
    pub fn write_parent_indent(&mut self) -> io::Result<()> {
        self.write_indent_prefix(self.list_stack.len().saturating_sub(1))
    }

    pub fn flush_pending_bullet(&mut self) -> io::Result<()> {
        if let Some(bullet) = self.pending_bullet.take() {
            if self.col_pos == 0 {
                self.write_parent_indent()?;
            }
            // Bullet is always unstyled.
            if self.term.color_enabled && !self.style_stack.is_empty() {
                self.out.write_all(b"\x1b[0m")?;
            }
            let w = UnicodeWidthStr::width(bullet.as_str());
            self.out.write_all(bullet.as_bytes())?;
            self.col_pos += w;
            self.trailing_nl = TrailingNewlines::Zero;
            if self.term.color_enabled && !self.style_stack.is_empty() {
                self.reset_styles()?;
            }
        }
        Ok(())
    }

    // ---- raw output primitives -------------------------------------------

    pub fn write_raw(&mut self, s: &str) -> io::Result<()> {
        if !s.is_empty() {
            self.out.write_all(s.as_bytes())?;
            self.trailing_nl = TrailingNewlines::Zero;
        }
        Ok(())
    }

    /// Emit a single `\n`, closing any active ANSI styling first so the next
    /// line's indent prefix (blockquote bar, list padding) is not tinted by
    /// whatever style was active mid-text.
    ///
    /// **Invariant — caller obligation:** after `write_newline`, no style
    /// from the previous line is still "open" on the terminal. If the next
    /// visible output is styled text, the caller **must** call
    /// `ensure_bol_styled` (which re-plays `style_stack` via `reset_styles`)
    /// before writing. The renderer uses a full-reset + re-apply pattern
    /// rather than matched on/off pairs because an early return from any
    /// event handler would otherwise leak style escapes across events.
    pub fn write_newline(&mut self) -> io::Result<()> {
        if self.term.color_enabled && !self.style_stack.is_empty() {
            self.out.write_all(b"\x1b[0m")?;
        }
        self.out.write_all(b"\n")?;
        self.trailing_nl = self.trailing_nl.bump();
        self.col_pos = 0;
        Ok(())
    }

    pub fn ensure_line_start(&mut self) -> io::Result<()> {
        if self.col_pos != 0 {
            self.write_newline()?;
        }
        Ok(())
    }

    pub fn ensure_blank_line(&mut self) -> io::Result<()> {
        if self.col_pos != 0 {
            self.write_newline()?;
        }
        while !self.trailing_nl.has_blank_line() {
            self.out.write_all(b"\n")?;
            self.trailing_nl = self.trailing_nl.bump();
            self.col_pos = 0;
        }
        Ok(())
    }

    pub fn current_indent_width(&self) -> usize {
        self.blockquote_depth * 2 + self.list_stack.len() * 2
    }

    /// Write the full indentation prefix — every blockquote bar plus one
    /// two-space pad for *every* active list level, including the current
    /// one. Used for continuation lines inside wrapped paragraphs, code
    /// blocks, table rows, and horizontal rules. Use `write_parent_indent`
    /// when emitting a new list item whose own bullet supplies the last
    /// level of indent.
    pub fn write_indent_raw(&mut self) -> io::Result<()> {
        self.write_indent_prefix(self.list_stack.len())
    }

    pub fn reset_styles(&mut self) -> io::Result<()> {
        if !self.term.color_enabled {
            return Ok(());
        }
        let mut buf = String::from("\x1b[0m");
        for &flag in &self.style_stack {
            buf.push_str(style_flag_ansi(flag));
        }
        self.out.write_all(buf.as_bytes())?;
        Ok(())
    }

    pub fn ensure_bol_styled(&mut self) -> io::Result<()> {
        if self.col_pos == 0 {
            if self.pending_bullet.is_none() {
                self.write_indent_raw()?;
            }
            // When a bullet is pending, flush_pending_bullet will handle both
            // the parent indent and any style reapplication around the bullet.
            if !self.style_stack.is_empty() && self.pending_bullet.is_none() {
                self.reset_styles()?;
            }
        }
        Ok(())
    }

    // ---- word-wrapped text emission --------------------------------------

    pub fn emit_word(&mut self, word: &str) -> io::Result<()> {
        if word.is_empty() {
            return Ok(());
        }
        let w = UnicodeWidthStr::width(word);
        let limit = self.term.width;
        if self.col_pos > 0 && self.col_pos + w > limit {
            self.write_newline()?;
        }
        self.ensure_bol_styled()?;
        self.flush_pending_bullet()?;
        // Flush any deferred OSC 8 opener now, so the clickable region starts
        // on the same visual line as the word it wraps. The opener has zero
        // display width — emitting it here does not affect col_pos or wrap
        // calculations, but it does keep `\x1b]8;;URL\x1b\\<word>` contiguous.
        if let Some(seq) = self.pending_osc8_open.take() {
            self.out.write_all(seq.as_bytes())?;
            self.trailing_nl = TrailingNewlines::Zero;
        }
        self.write_raw(word)?;
        self.col_pos += w;
        Ok(())
    }

    pub fn emit_space(&mut self) -> io::Result<()> {
        if self.col_pos == 0 {
            return Ok(());
        }
        if self.col_pos + 1 > self.term.width {
            self.write_newline()?;
            return Ok(());
        }
        self.write_raw(" ")?;
        self.col_pos += 1;
        Ok(())
    }

    /// Word-wrapped text emission. Splits `text` on whitespace and hands
    /// each word to `emit_word` as a `&str` slice borrowed from the input —
    /// no per-word allocation. Walks by `char_indices` so UTF-8 boundaries
    /// are respected without building an intermediate `String`.
    pub fn write_wrapped(&mut self, text: &str) -> io::Result<()> {
        let mut word_start: Option<usize> = None;
        for (i, c) in text.char_indices() {
            if c.is_whitespace() {
                if let Some(start) = word_start.take() {
                    // `start` and `i` are both char boundaries from
                    // char_indices, so the slice is always valid UTF-8.
                    self.emit_word(&text[start..i])?;
                }
                self.emit_space()?;
            } else if word_start.is_none() {
                word_start = Some(i);
            }
        }
        if let Some(start) = word_start {
            self.emit_word(&text[start..])?;
        }
        Ok(())
    }

    // ---- style stack -----------------------------------------------------

    pub fn push_style(&mut self, flag: StyleFlag) -> io::Result<()> {
        self.style_stack.push(flag);
        if self.term.color_enabled && self.col_pos > 0 {
            self.out.write_all(style_flag_ansi(flag).as_bytes())?;
        }
        Ok(())
    }

    pub fn pop_style(&mut self, flag: StyleFlag) -> io::Result<()> {
        if let Some(pos) = self.style_stack.iter().rposition(|&f| f == flag) {
            self.style_stack.remove(pos);
        }
        if self.term.color_enabled {
            self.reset_styles()?;
        }
        Ok(())
    }

    // ---- event dispatch --------------------------------------------------

    pub fn dispatch(&mut self, ev: Event<'_>) -> io::Result<()> {
        match ev {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag) => self.handle_end(tag),
            Event::Text(s) => self.handle_text(&s),
            Event::Code(s) => self.handle_inline_code(&s),
            Event::Html(_) | Event::InlineHtml(_) => Ok(()),
            Event::SoftBreak => {
                if self.in_code_block {
                    self.code_buf.push('\n');
                    Ok(())
                } else if let Some(table) = &mut self.table {
                    table.current_cell.push(' ');
                    Ok(())
                } else if self.capturing_image.is_some() {
                    self.image_alt_buf.push(' ');
                    Ok(())
                } else if self.capturing_footnote.is_some() {
                    self.footnote_capture_buf.push(' ');
                    Ok(())
                } else {
                    self.emit_space()
                }
            }
            Event::HardBreak => {
                if self.in_code_block {
                    self.code_buf.push('\n');
                    Ok(())
                } else if self.table.is_some() || self.capturing_image.is_some() {
                    Ok(())
                } else if self.capturing_footnote.is_some() {
                    self.footnote_capture_buf.push(' ');
                    Ok(())
                } else {
                    self.write_newline()?;
                    self.ensure_bol_styled()
                }
            }
            Event::Rule => self.write_hr(),
            Event::TaskListMarker(checked) => {
                self.ensure_bol_styled()?;
                // Task markers replace the list bullet; drop the pending one.
                self.pending_bullet = None;
                let marker = if checked { "☑ " } else { "☐ " };
                if checked && self.term.color_enabled {
                    self.out.write_all(b"\x1b[2m")?;
                }
                self.write_raw(marker)?;
                if checked && self.term.color_enabled {
                    self.reset_styles()?;
                }
                self.col_pos += UnicodeWidthStr::width(marker);
                Ok(())
            }
            Event::FootnoteReference(label) => self.handle_footnote_ref(&label),
            Event::InlineMath(s) | Event::DisplayMath(s) => self.handle_inline_code(&s),
        }
    }

    // ---- text routing ----------------------------------------------------

    pub fn handle_text(&mut self, text: &str) -> io::Result<()> {
        // Sanitize once at intake so every downstream buffer (table cells,
        // footnote bodies, code blocks, image alt) sees only safe bytes.
        // Anywhere we'd later emit this text — inline, inside a reverse-video
        // inline-code region, inside an OSC 8 hyperlink label, or through the
        // syntax highlighter — a stray ESC / BEL / NUL / ST would corrupt the
        // terminal or break out of the surrounding escape.
        let clean = sanitize_display_text(text);
        let text = clean.as_ref();
        // Always record link display text so TagEnd::Link's "display == url"
        // check works in every context (table cells, footnote bodies, plain
        // paragraphs). Without this, a link inside a table cell would always
        // trip the "(url)" suffix even when the display text equals the URL.
        if let Some(buf) = self.pending_link_text.as_mut() {
            buf.push_str(text);
        }
        if self.capturing_image.is_some() {
            self.image_alt_buf.push_str(text);
            return Ok(());
        }
        if self.capturing_footnote.is_some() {
            self.footnote_capture_buf.push_str(text);
            return Ok(());
        }
        if self.in_code_block {
            self.code_buf.push_str(text);
            return Ok(());
        }
        if let Some(table) = &mut self.table {
            table.current_cell.push_str(text);
            return Ok(());
        }
        self.write_wrapped(text)
    }

    pub fn handle_inline_code(&mut self, code: &str) -> io::Result<()> {
        // Inline code renders in reverse-video and is especially dangerous as
        // an injection vector — a stray ESC inside the code span would emit a
        // real CSI/OSC to the terminal. Sanitize before buffering or writing.
        let clean = sanitize_display_text(code);
        let code = clean.as_ref();
        // Mirror handle_text: always record link display text so the
        // TagEnd::Link equality check works in every buffering context.
        if let Some(buf) = self.pending_link_text.as_mut() {
            buf.push_str(code);
        }
        if self.capturing_image.is_some() {
            self.image_alt_buf.push_str(code);
            return Ok(());
        }
        if self.capturing_footnote.is_some() {
            self.footnote_capture_buf.push_str(code);
            return Ok(());
        }
        if self.in_code_block {
            self.code_buf.push_str(code);
            return Ok(());
        }
        if let Some(table) = &mut self.table {
            table.current_cell.push_str(code);
            return Ok(());
        }
        let w = UnicodeWidthStr::width(code);
        if self.col_pos > 0 && self.col_pos + w + 2 > self.term.width {
            self.write_newline()?;
        }
        // push_style BEFORE ensure_bol_styled so that the inline-code reverse-video
        // escape is emitted at the start of a line (reset_styles picks it up from
        // the stack). Otherwise push_style is a no-op at col 0 and the opening
        // \x1b[7m is lost — visible as the very first code span of a document
        // rendering with no opening escape.
        self.push_style(StyleFlag::InlineCode)?;
        self.ensure_bol_styled()?;
        self.flush_pending_bullet()?;
        // If this inline-code span is the first visible token of an OSC 8
        // link (e.g. `[`foo`](url)`), flush the deferred opener now so the
        // clickable region wraps the code span contiguously.
        if let Some(seq) = self.pending_osc8_open.take() {
            self.out.write_all(seq.as_bytes())?;
            self.trailing_nl = TrailingNewlines::Zero;
        }
        self.write_raw(" ")?;
        self.write_raw(code)?;
        self.write_raw(" ")?;
        self.col_pos += w + 2;
        self.pop_style(StyleFlag::InlineCode)?;
        Ok(())
    }

    // ---- tags ------------------------------------------------------------

    pub fn handle_start(&mut self, tag: Tag<'_>) -> io::Result<()> {
        match tag {
            Tag::Paragraph => self.ensure_blank_line(),
            Tag::Heading { level, .. } => {
                self.ensure_blank_line()?;
                self.ensure_bol_styled()?;
                let lvl = heading_level_num(level);
                self.push_style(StyleFlag::Heading(lvl))?;
                Ok(())
            }
            Tag::BlockQuote(_) => {
                self.ensure_line_start()?;
                self.blockquote_depth += 1;
                self.style_stack.push(StyleFlag::BlockQuote);
                Ok(())
            }
            Tag::CodeBlock(kind) => {
                self.ensure_blank_line()?;
                self.in_code_block = true;
                self.code_buf.clear();
                self.code_lang = match kind {
                    // The fence info-string is untrusted input (from the
                    // markdown source), printed verbatim as a dim label
                    // above the block and used to look up a syntect syntax.
                    // Sanitize to a conservative character set — letters,
                    // digits, `+`, `-`, `.`, `_`, `#` (c#, f#), `/` — so a
                    // malicious fence like ```\x1b]0;title\x07 cannot
                    // inject an OSC/CSI into the terminal or smuggle a
                    // directory traversal into the syntect lookup.
                    CodeBlockKind::Fenced(l) => sanitize_code_lang(l.trim()),
                    CodeBlockKind::Indented => String::new(),
                };
                Ok(())
            }
            Tag::HtmlBlock => Ok(()),
            Tag::List(start) => {
                // If we're opening a nested list immediately after a parent
                // Item (no text yet), we still need the parent's bullet
                // drawn on its own line so the nested items indent under it.
                if self.pending_bullet.is_some() {
                    self.flush_pending_bullet()?;
                    self.write_newline()?;
                }
                self.ensure_line_start()?;
                match start {
                    Some(n) => self.list_stack.push(ListMarker::Ordered(n)),
                    None => self.list_stack.push(ListMarker::Unordered),
                }
                Ok(())
            }
            Tag::Item => {
                self.ensure_line_start()?;
                let depth = self.list_stack.len();
                let marker_str = match self.list_stack.last_mut() {
                    Some(ListMarker::Unordered) => match depth {
                        1 => "• ".to_string(),
                        2 => "◦ ".to_string(),
                        _ => "▸ ".to_string(),
                    },
                    Some(ListMarker::Ordered(n)) => {
                        let s = format!("{n}. ");
                        *n += 1;
                        s
                    }
                    None => "• ".to_string(),
                };
                self.pending_bullet = Some(marker_str);
                Ok(())
            }
            Tag::FootnoteDefinition(label) => {
                self.capturing_footnote = Some(label.to_string());
                self.footnote_capture_buf.clear();
                Ok(())
            }
            Tag::DefinitionList => {
                self.ensure_blank_line()?;
                Ok(())
            }
            Tag::DefinitionListTitle => {
                self.ensure_line_start()?;
                self.push_style(StyleFlag::Bold)?;
                Ok(())
            }
            Tag::DefinitionListDefinition => {
                self.ensure_line_start()?;
                self.write_indent_raw()?;
                // Definitions render under their term, indented by two spaces
                // and prefixed with a dim ":" gutter for visual hierarchy.
                if self.term.color_enabled {
                    self.write_raw("\x1b[2m: \x1b[0m")?;
                } else {
                    self.write_raw(": ")?;
                }
                self.col_pos += 2;
                self.push_style(StyleFlag::Italic)?;
                Ok(())
            }
            Tag::Table(aligns) => {
                self.ensure_blank_line()?;
                self.table = Some(TableState {
                    aligns,
                    header: Vec::new(),
                    rows: Vec::new(),
                    current_row: Vec::new(),
                    current_cell: String::new(),
                    in_header: false,
                });
                Ok(())
            }
            Tag::TableHead => {
                if let Some(t) = &mut self.table {
                    t.in_header = true;
                    t.current_row.clear();
                }
                Ok(())
            }
            Tag::TableRow => {
                if let Some(t) = &mut self.table {
                    t.current_row.clear();
                }
                Ok(())
            }
            Tag::TableCell => {
                if let Some(t) = &mut self.table {
                    t.current_cell.clear();
                }
                Ok(())
            }
            Tag::Emphasis => self.push_style(StyleFlag::Italic),
            Tag::Strong => self.push_style(StyleFlag::Bold),
            Tag::Strikethrough => self.push_style(StyleFlag::Strike),
            Tag::Superscript | Tag::Subscript => self.push_style(StyleFlag::Dim),
            Tag::Link { dest_url, .. } => {
                // Sanitize the URL for display (stripped of control bytes so
                // it cannot inject into the plain-text fallback) and emit the
                // OSC 8 escape only when the raw URL passes the stricter
                // `sanitize_osc_url` check. A URL containing raw ESC / BEL /
                // NUL / ST can close the hyperlink escape early and retarget
                // everything that follows.
                let raw = dest_url.to_string();
                let display_clean = sanitize_display_text(&raw).into_owned();
                self.pending_link_url = Some(display_clean);
                self.pending_link_text = Some(String::new());
                // When we are buffering into a table cell, footnote body, or
                // image alt-text, NO output may go directly to `self.out` —
                // an OSC 8 open would land above the table/footnote and the
                // underline escape would leak outside the cell's padded
                // boundaries. Skip direct emission; TagEnd::Link appends a
                // textual `(url)` into the buffer so the target survives.
                let buffering = self.table.is_some()
                    || self.capturing_footnote.is_some()
                    || self.capturing_image.is_some();
                if !buffering
                    && self.term.osc8_supported
                    && let Some(safe) = sanitize_osc_url(&raw)
                {
                    // Defer the OSC 8 open — don't flush it until `emit_word`
                    // is about to write the link's first visible token. That
                    // way, if the first word would overflow the current line,
                    // `emit_word` wraps first and emits the opener on the new
                    // line, keeping opener + first word atomic. Emitting it
                    // here instead can strand the opener at the end of the
                    // previous line, and terminals that treat `\n` as an
                    // implicit OSC 8 terminator (some tmux configs, older
                    // VTE) lose the clickable region entirely.
                    self.pending_osc8_open = Some(format!("\x1b]8;;{safe}\x1b\\"));
                }
                if buffering {
                    Ok(())
                } else {
                    self.push_style(StyleFlag::Underline)
                }
            }
            Tag::Image { dest_url, .. } => {
                let raw = dest_url.to_string();
                let clean = sanitize_display_text(&raw).into_owned();
                self.capturing_image = Some(clean);
                self.image_alt_buf.clear();
                Ok(())
            }
            Tag::MetadataBlock(_) => Ok(()),
        }
    }

    pub fn handle_end(&mut self, tag: TagEnd) -> io::Result<()> {
        match tag {
            TagEnd::Paragraph => self.ensure_blank_line(),
            TagEnd::Heading(level) => {
                let lvl = heading_level_num(level);
                self.pop_style(StyleFlag::Heading(lvl))?;
                self.ensure_blank_line()
            }
            TagEnd::BlockQuote(_) => {
                if let Some(pos) = self
                    .style_stack
                    .iter()
                    .rposition(|&f| f == StyleFlag::BlockQuote)
                {
                    self.style_stack.remove(pos);
                }
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.ensure_line_start()
            }
            TagEnd::CodeBlock => self.flush_code_block(),
            TagEnd::HtmlBlock => Ok(()),
            TagEnd::List(_) => {
                self.list_stack.pop();
                self.ensure_line_start()?;
                if self.list_stack.is_empty() {
                    self.ensure_blank_line()?;
                }
                Ok(())
            }
            TagEnd::Item => {
                if self.pending_bullet.is_some() {
                    self.flush_pending_bullet()?;
                }
                self.ensure_line_start()
            }
            TagEnd::FootnoteDefinition => {
                if let Some(label) = self.capturing_footnote.take() {
                    // Ensure the reference has a numeric slot
                    if !self.footnote_index.contains_key(&label) {
                        self.footnote_counter += 1;
                        self.footnote_index
                            .insert(label.clone(), self.footnote_counter);
                    }
                    let body = std::mem::take(&mut self.footnote_capture_buf);
                    self.footnote_bodies.push((label, body.trim().to_string()));
                }
                Ok(())
            }
            TagEnd::DefinitionList => self.ensure_blank_line(),
            TagEnd::DefinitionListTitle => {
                self.pop_style(StyleFlag::Bold)?;
                self.ensure_line_start()
            }
            TagEnd::DefinitionListDefinition => {
                self.pop_style(StyleFlag::Italic)?;
                self.ensure_line_start()
            }
            TagEnd::Table => self.flush_table(),
            TagEnd::TableHead => {
                // Idempotent header capture. pulldown-cmark 0.13 emits
                // header cells directly inside TableHead (no TableRow
                // wrapping), so current_row holds the unassigned header
                // cells when we get here. If a future revision starts
                // wrapping header cells in an inner TableRow, End(TableRow)
                // will have fired first with `in_header=true` and drained
                // current_row into `t.header`; guarding on `header.is_empty()`
                // prevents us from clobbering a good header with an
                // accidentally-empty current_row.
                if let Some(t) = &mut self.table {
                    if t.header.is_empty() && !t.current_row.is_empty() {
                        t.header = std::mem::take(&mut t.current_row);
                    } else {
                        // Header already captured by inner End(TableRow).
                        // Drop any stragglers so they don't leak into row 0.
                        t.current_row.clear();
                    }
                    t.in_header = false;
                }
                Ok(())
            }
            TagEnd::TableRow => {
                if let Some(t) = &mut self.table {
                    if t.in_header {
                        t.header = std::mem::take(&mut t.current_row);
                    } else {
                        let row = std::mem::take(&mut t.current_row);
                        t.rows.push(row);
                    }
                }
                Ok(())
            }
            TagEnd::TableCell => {
                if let Some(t) = &mut self.table {
                    let cell = std::mem::take(&mut t.current_cell);
                    t.current_row.push(cell);
                }
                Ok(())
            }
            TagEnd::Emphasis => self.pop_style(StyleFlag::Italic),
            TagEnd::Strong => self.pop_style(StyleFlag::Bold),
            TagEnd::Strikethrough => self.pop_style(StyleFlag::Strike),
            TagEnd::Superscript | TagEnd::Subscript => self.pop_style(StyleFlag::Dim),
            TagEnd::Link => {
                let url = self.pending_link_url.take().unwrap_or_default();
                let display = self.pending_link_text.take().unwrap_or_default();
                // Inside a table cell / footnote body / image alt we never
                // pushed the Underline style at Tag::Link start (to avoid
                // leaking escapes outside the buffer), so we must NOT pop
                // it here either. Route the URL suffix into the active
                // buffer as plain text.
                if let Some(t) = self.table.as_mut() {
                    if !url.is_empty() && !link_text_equals_url(&display, &url) {
                        t.current_cell.push_str(&format!(" ({url})"));
                    }
                    return Ok(());
                }
                if self.capturing_footnote.is_some() {
                    if !url.is_empty() && !link_text_equals_url(&display, &url) {
                        self.footnote_capture_buf.push_str(&format!(" ({url})"));
                    }
                    return Ok(());
                }
                if self.capturing_image.is_some() {
                    // Alt-text contexts almost never contain real links
                    // (parsers treat [x] inside alt text as literal), but
                    // if one slips through, append URL as plain text.
                    if !url.is_empty() && !link_text_equals_url(&display, &url) {
                        self.image_alt_buf.push_str(&format!(" ({url})"));
                    }
                    return Ok(());
                }

                self.pop_style(StyleFlag::Underline)?;
                // If the OSC 8 opener is still pending, the link had no
                // rendered display text — `emit_word` never fired, so no
                // opener was ever flushed. Drop it silently; emitting only a
                // closer would be a malformed escape that retargets every
                // later hyperlink on the line.
                let opener_was_flushed = self.pending_osc8_open.take().is_none();
                if self.term.osc8_supported && !url.is_empty() {
                    if opener_was_flushed {
                        self.out.write_all(b"\x1b]8;;\x1b\\")?;
                        self.trailing_nl = TrailingNewlines::Zero;
                    }
                } else if !url.is_empty() && !link_text_equals_url(&display, &url) {
                    // Append " (url)" in dim+underline, but only when the URL
                    // is meaningfully different from the displayed text. This
                    // prevents bare URLs / autolinks from rendering as
                    // `https://example.com (https://example.com)`.
                    self.push_style(StyleFlag::Dim)?;
                    let label = format!(" ({url})");
                    self.write_wrapped(&label)?;
                    self.pop_style(StyleFlag::Dim)?;
                }
                Ok(())
            }
            TagEnd::Image => {
                let url = self.capturing_image.take().unwrap_or_default();
                let alt = std::mem::take(&mut self.image_alt_buf);
                // Inline contexts that buffer text — table cells, footnote
                // bodies — must not have viuer write image bytes mid-buffer
                // (it bypasses the buffer and clobbers layout). Substitute a
                // textual placeholder so the cell/footnote stays well-formed.
                if let Some(t) = self.table.as_mut() {
                    let label = if alt.is_empty() {
                        format!("[image: {url}]")
                    } else {
                        format!("[image: {alt}]")
                    };
                    t.current_cell.push_str(&label);
                    return Ok(());
                }
                if self.capturing_footnote.is_some() {
                    let label = if alt.is_empty() {
                        format!("[image: {url}]")
                    } else {
                        format!("[image: {alt}]")
                    };
                    self.footnote_capture_buf.push_str(&label);
                    return Ok(());
                }
                self.render_image(&url, &alt)
            }
            TagEnd::MetadataBlock(_) => Ok(()),
        }
    }

    // ---- footnote / rule helpers -----------------------------------------

    pub fn handle_footnote_ref(&mut self, label: &str) -> io::Result<()> {
        let n = match self.footnote_index.get(label).copied() {
            Some(n) => n,
            None => {
                self.footnote_counter += 1;
                self.footnote_index
                    .insert(label.to_string(), self.footnote_counter);
                self.footnote_counter
            }
        };
        let marker = format!("[{n}]");
        self.push_style(StyleFlag::Dim)?;
        self.emit_word(&marker)?;
        self.pop_style(StyleFlag::Dim)
    }

    pub fn write_hr(&mut self) -> io::Result<()> {
        if self.pending_bullet.is_some() {
            self.flush_pending_bullet()?;
        }
        self.ensure_blank_line()?;
        self.write_indent_raw()?;
        let w = self
            .term
            .width
            .saturating_sub(self.current_indent_width())
            .max(3);
        if self.term.color_enabled {
            self.write_raw("\x1b[2m")?;
        }
        let bar: String = "─".repeat(w);
        self.write_raw(&bar)?;
        if self.term.color_enabled {
            self.write_raw("\x1b[0m")?;
        }
        self.col_pos += w;
        self.write_newline()?;
        self.ensure_blank_line()
    }

    // ---- code block flush ------------------------------------------------

    pub fn flush_code_block(&mut self) -> io::Result<()> {
        self.in_code_block = false;
        let lang = std::mem::take(&mut self.code_lang);
        let code = std::mem::take(&mut self.code_buf);

        self.ensure_line_start()?;

        // Dim italic language label above the block (only if we have one)
        if !lang.is_empty() {
            self.write_indent_raw()?;
            if self.term.color_enabled {
                self.write_raw("\x1b[2;3m")?;
                self.write_raw(&lang)?;
                self.write_raw("\x1b[0m")?;
            } else {
                self.write_raw(&lang)?;
            }
            self.col_pos += UnicodeWidthStr::width(lang.as_str());
            self.write_newline()?;
        }

        // First-touch lazy init: `SYNTAX_SET` / `HIGHLIGHT_THEME` materialize
        // exactly once per process and only on the first code block we
        // actually render. A doc with no fenced code pays zero syntect cost.
        let syntaxes = syntax_set();
        let theme = highlight_theme();
        let syntax_ref = if !lang.is_empty() {
            syntaxes
                .find_syntax_by_token(&lang)
                .or_else(|| syntaxes.find_syntax_by_extension(&lang))
                .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
        } else {
            syntaxes.find_syntax_plain_text()
        };

        let mut hl = HighlightLines::new(syntax_ref, theme);
        for line in code.lines() {
            self.write_indent_raw()?;
            if self.term.color_enabled {
                match hl.highlight_line(line, syntaxes) {
                    Ok(ranges) => {
                        let escaped = as_24_bit_terminal_escaped(&ranges, false);
                        self.write_raw(&escaped)?;
                        self.write_raw("\x1b[0m")?;
                    }
                    Err(_) => {
                        self.write_raw(line)?;
                    }
                }
            } else {
                self.write_raw(line)?;
            }
            // No col_pos update — write_newline() resets it to 0 immediately,
            // and nothing between here and the newline reads the column.
            self.write_newline()?;
        }

        self.ensure_blank_line()?;
        Ok(())
    }

    // ---- table flush -----------------------------------------------------

    pub fn flush_table(&mut self) -> io::Result<()> {
        let table = match self.table.take() {
            Some(t) => t,
            None => return Ok(()),
        };

        self.ensure_line_start()?;

        let n_cols = table
            .header
            .len()
            .max(table.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if n_cols == 0 {
            return Ok(());
        }

        let mut widths = vec![0usize; n_cols];
        for (i, c) in table.header.iter().enumerate() {
            if i < n_cols {
                widths[i] = widths[i].max(UnicodeWidthStr::width(c.as_str()));
            }
        }
        for row in &table.rows {
            for (i, c) in row.iter().enumerate() {
                if i < n_cols {
                    widths[i] = widths[i].max(UnicodeWidthStr::width(c.as_str()));
                }
            }
        }
        // every cell is padded with " " on each side + "│" borders
        // total = Σ(width+2) + (n_cols+1)
        let total =
            |ws: &[usize]| -> usize { ws.iter().map(|w| w + 2).sum::<usize>() + ws.len() + 1 };

        let avail = self.term.width.saturating_sub(self.current_indent_width());
        if total(&widths) > avail {
            let overhead = widths.len() * 3 + 1; // borders + padding
            let content_budget = avail.saturating_sub(overhead).max(n_cols);
            let sum: usize = widths.iter().sum();
            if sum > content_budget {
                // scale down proportionally; ensure each col >= 1
                let mut remaining = content_budget;
                for (i, w) in widths.iter_mut().enumerate() {
                    if i + 1 == n_cols {
                        *w = remaining.max(1);
                    } else {
                        let share =
                            ((*w as f64) / (sum as f64) * content_budget as f64).floor() as usize;
                        let share = share.max(1);
                        *w = share;
                        remaining = remaining.saturating_sub(share);
                    }
                }
            }
        }

        let top = border(&widths, '┌', '┬', '┐', '─');
        let mid = border(&widths, '╞', '╪', '╡', '═');
        let sep = border(&widths, '├', '┼', '┤', '─');
        let bot = border(&widths, '└', '┴', '┘', '─');

        self.write_table_border(&top)?;
        self.write_table_row(&table.header, &widths, &table.aligns, true)?;
        self.write_table_border(&mid)?;
        for (idx, row) in table.rows.iter().enumerate() {
            self.write_table_row(row, &widths, &table.aligns, false)?;
            if idx + 1 != table.rows.len() {
                self.write_table_border(&sep)?;
            }
        }
        self.write_table_border(&bot)?;
        self.ensure_blank_line()?;
        Ok(())
    }

    pub fn write_table_border(&mut self, border: &str) -> io::Result<()> {
        self.write_indent_raw()?;
        if self.term.color_enabled {
            self.write_raw("\x1b[2m")?;
        }
        self.write_raw(border)?;
        if self.term.color_enabled {
            self.write_raw("\x1b[0m")?;
        }
        self.col_pos += UnicodeWidthStr::width(border);
        self.write_newline()
    }

    pub fn write_table_row(
        &mut self,
        cells: &[String],
        widths: &[usize],
        aligns: &[Alignment],
        header: bool,
    ) -> io::Result<()> {
        self.write_indent_raw()?;
        let mut line_w = 0usize;
        if self.term.color_enabled {
            self.write_raw("\x1b[2m│\x1b[0m")?;
        } else {
            self.write_raw("|")?;
        }
        line_w += 1;
        let empty = String::new();
        for (i, &col_w) in widths.iter().enumerate() {
            let cell = cells.get(i).unwrap_or(&empty);
            let align = aligns.get(i).copied().unwrap_or(Alignment::None);
            let padded = pad_cell(cell, col_w, align);
            self.write_raw(" ")?;
            line_w += 1;
            if header && self.term.color_enabled {
                self.write_raw("\x1b[1m")?;
            }
            self.write_raw(&padded)?;
            if header && self.term.color_enabled {
                self.write_raw("\x1b[22m")?;
            }
            line_w += UnicodeWidthStr::width(padded.as_str());
            self.write_raw(" ")?;
            line_w += 1;
            if self.term.color_enabled {
                self.write_raw("\x1b[2m│\x1b[0m")?;
            } else {
                self.write_raw("|")?;
            }
            line_w += 1;
        }
        self.col_pos += line_w;
        self.write_newline()
    }

    // ---- image rendering -------------------------------------------------

    pub fn render_image(&mut self, url: &str, alt: &str) -> io::Result<()> {
        let fallback = |this: &mut Self, why: &str| -> io::Result<()> {
            let label = if alt.is_empty() {
                format!("[image: {url}{why}]")
            } else {
                format!("[image: {alt}{why}]")
            };
            this.push_style(StyleFlag::Dim)?;
            this.write_wrapped(&label)?;
            this.pop_style(StyleFlag::Dim)?;
            Ok(())
        };

        if url.is_empty() {
            return fallback(self, "");
        }
        if self.term.image_protocol == ImageProtocol::None {
            return fallback(self, "");
        }
        // Image escape sequences are meaningless when stdout isn't a real TTY
        // (e.g., user piped through `less` with --force-color). Fall back to
        // textual placeholder instead of polluting the pipe.
        if !self.term.is_tty {
            return fallback(self, "");
        }

        // Resolve the URL into a local path. Remote URLs are downloaded into a
        // NamedTempFile that lives until the end of this function (RAII cleanup).
        // Local references are constrained to the source document's directory
        // so a hostile markdown file cannot `![](../../../../etc/passwd)` the
        // renderer into reading arbitrary files off disk.
        let _temp_holder: Option<NamedTempFile>;
        let path_buf: PathBuf = if url.starts_with("http://") || url.starts_with("https://") {
            match fetch_remote_image_to_temp(url) {
                Ok(tf) => {
                    let p = tf.path().to_path_buf();
                    _temp_holder = Some(tf);
                    p
                }
                Err(reason) => {
                    return fallback(self, &format!(" ({reason})"));
                }
            }
        } else {
            _temp_holder = None;
            match &self.source_base {
                Some(base) => {
                    match resolve_local_image_path(
                        url,
                        base,
                        self.term.allow_absolute_image_paths,
                    ) {
                        Ok(p) => p,
                        Err(reason) => return fallback(self, &format!(" ({reason})")),
                    }
                }
                // stdin or an unknown filesystem root: refuse local file
                // access. This is the secure default; the alternative
                // (reading `./foo.png` from CWD) gives an attacker an oracle
                // for any file the renderer's process can open.
                None => return fallback(self, " (no source dir)"),
            }
        };

        if !path_buf.exists() {
            return fallback(self, " (not found)");
        }

        // Flush our buffered writer before viuer takes over stdout directly.
        self.ensure_line_start()?;
        self.out.flush()?;

        let avail_cols = self
            .term
            .width
            .saturating_sub(self.current_indent_width())
            .max(4) as u32;

        // Aspect-correct row count: load the image once to read its true pixel
        // dimensions, derive how many cell-rows it would occupy at `avail_cols`
        // columns given the cell aspect ratio, then clamp so a giant image
        // never blows past IMAGE_MAX_VIEWPORT_FRACTION of the visible viewport.
        let (img_w_px, img_h_px) = match image::image_dimensions(&path_buf) {
            Ok((w, h)) => (w, h),
            Err(_) => return fallback(self, " (decode failed)"),
        };
        let cell_w = self.term.cell_pixel_width.max(1) as f64;
        let cell_h = self.term.cell_pixel_height.max(1) as f64;
        let target_cols = avail_cols as f64;
        let target_rows =
            (img_h_px as f64 * cell_w * target_cols) / (img_w_px as f64 * cell_h).max(1.0);
        let viewport_rows = terminal_size::terminal_size()
            .map(|(_, h)| h.0 as f64)
            .unwrap_or(40.0);
        let max_rows = (viewport_rows * image_max_height_scale()).max(4.0);
        let rows = target_rows.round().clamp(1.0, max_rows) as u32;

        let vcfg = viuer::Config {
            absolute_offset: false,
            x: self.current_indent_width() as u16,
            y: 0,
            width: Some(avail_cols),
            height: Some(rows),
            use_kitty: matches!(self.term.image_protocol, ImageProtocol::Kitty),
            use_iterm: matches!(self.term.image_protocol, ImageProtocol::Iterm2),
            #[cfg(feature = "sixel")]
            use_sixel: matches!(self.term.image_protocol, ImageProtocol::Sixel),
            ..Default::default()
        };

        // When sixel was detected but the binary was built without the `sixel`
        // cargo feature, viuer would try Kitty/iTerm/halfblock — none of which
        // is sixel. Force the halfblock path to render *something* instead of
        // silently emitting a kitty escape sequence into a sixel-only terminal.
        #[cfg(not(feature = "sixel"))]
        let vcfg = {
            let mut v = vcfg;
            if matches!(self.term.image_protocol, ImageProtocol::Sixel) {
                v.use_kitty = false;
                v.use_iterm = false;
            }
            v
        };

        match viuer::print_from_file(&path_buf, &vcfg) {
            Ok(_) => {
                self.col_pos = 0;
                self.trailing_nl = TrailingNewlines::One;
                Ok(())
            }
            Err(_) => fallback(self, " (render failed)"),
        }
    }

    // ---- flush footnotes (end-of-document) ------------------------------

    pub fn flush_footnotes(&mut self) -> io::Result<()> {
        if self.footnote_bodies.is_empty() {
            return Ok(());
        }
        self.ensure_blank_line()?;
        let w = self.term.width.max(3);
        if self.term.color_enabled {
            self.write_raw("\x1b[2m")?;
        }
        let bar: String = "─".repeat(w);
        self.write_raw(&bar)?;
        if self.term.color_enabled {
            self.write_raw("\x1b[0m")?;
        }
        self.col_pos += w;
        self.write_newline()?;
        self.ensure_blank_line()?;

        let bodies = std::mem::take(&mut self.footnote_bodies);
        // Order by footnote number (index). Unindexed labels fall back to
        // insertion order.
        let mut numbered: Vec<(usize, String, String)> = bodies
            .into_iter()
            .map(|(label, body)| {
                let n = self
                    .footnote_index
                    .get(&label)
                    .copied()
                    .unwrap_or(usize::MAX);
                (n, label, body)
            })
            .collect();
        numbered.sort_by_key(|(n, _, _)| *n);

        for (n, _label, body) in numbered {
            let prefix = format!("[{n}] ");
            self.push_style(StyleFlag::Dim)?;
            self.ensure_bol_styled()?;
            self.write_raw(&prefix)?;
            self.col_pos += UnicodeWidthStr::width(prefix.as_str());
            self.write_wrapped(&body)?;
            self.pop_style(StyleFlag::Dim)?;
            self.write_newline()?;
        }
        Ok(())
    }
}
