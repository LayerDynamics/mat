//! Markdown ingest — source preprocessor + `render()` entry point that
//! drives the `pulldown_cmark::Parser` event loop against `RenderState`.

use std::borrow::Cow;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use pulldown_cmark::{Options, Parser};

use crate::state::RenderState;
use crate::terminal::TermConfig;

/// Normalize common non-strict markdown patterns that CommonMark refuses but
/// users clearly *mean* as lists:
///
/// 1. A line whose only non-whitespace content starts with `- `, `* `, `+ `,
///    or `N. ` but is indented ≥4 spaces, **and** the previous non-blank line
///    is not itself indented as a list item — strip the indent. CommonMark
///    would treat this as paragraph continuation or an indented code block;
///    the user almost always means a top-level list.
/// 2. Insert a blank line between a paragraph and the first such list item so
///    pulldown-cmark starts a fresh list instead of continuing the paragraph.
///
/// Anything inside fenced code blocks (```...```) is left untouched.
pub fn preprocess_markdown(src: &str) -> Cow<'_, str> {
    fn is_list_marker(trimmed: &str) -> bool {
        trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("+ ")
            || trimmed
                .split_once(". ")
                .map(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
                .unwrap_or(false)
    }

    // Fast path: if no line has `    - ` / `    * ` / `    + ` / `    N. `,
    // nothing to normalize — return the source borrowed.
    let suspicious = src.lines().any(|l| {
        let t = l.trim_start();
        let indent = l.len() - t.len();
        indent >= 4 && is_list_marker(t)
    });
    if !suspicious {
        return Cow::Borrowed(src);
    }

    let mut out = String::with_capacity(src.len() + 16);
    let mut in_fence = false;
    // None = not inside a promoted list run; Some(n) = inside, strip n leading
    // spaces off each line in the run so relative nesting is preserved.
    let mut run_base_indent: Option<usize> = None;
    let mut prev_was_blank = true;
    // Last non-blank line was itself a list marker (possibly nested). When
    // true, a `    - X` line is legitimate CommonMark nesting — leave it
    // alone. When false, `    - X` is almost certainly a user's
    // over-indented list after a paragraph — promote it.
    let mut last_nonblank_was_listish = false;

    for line in src.split_inclusive('\n') {
        let body = line.trim_end_matches('\n').trim_end_matches('\r');
        let trimmed = body.trim_start();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            run_base_indent = None;
            out.push_str(line);
            prev_was_blank = false;
            last_nonblank_was_listish = false;
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }

        let indent = body.len() - trimmed.len();
        let listish = is_list_marker(trimmed);

        match run_base_indent {
            None => {
                if listish && indent >= 4 && !last_nonblank_was_listish {
                    if !prev_was_blank {
                        out.push('\n');
                    }
                    out.push_str(trimmed);
                    out.push('\n');
                    run_base_indent = Some(indent);
                    prev_was_blank = false;
                    last_nonblank_was_listish = true;
                    continue;
                }
                out.push_str(line);
                if trimmed.is_empty() {
                    prev_was_blank = true;
                } else {
                    prev_was_blank = false;
                    last_nonblank_was_listish = listish;
                }
            }
            Some(base) => {
                if trimmed.is_empty() {
                    out.push_str(line);
                    prev_was_blank = true;
                    continue;
                }
                if indent >= base && (listish || indent > base) {
                    let stripped = indent - base;
                    for _ in 0..stripped {
                        out.push(' ');
                    }
                    out.push_str(trimmed);
                    out.push('\n');
                    prev_was_blank = false;
                    last_nonblank_was_listish = true;
                    continue;
                }
                run_base_indent = None;
                out.push_str(line);
                prev_was_blank = trimmed.is_empty();
                if !trimmed.is_empty() {
                    last_nonblank_was_listish = listish;
                }
            }
        }
    }

    Cow::Owned(out)
}

pub fn render(markdown: &str, term: &TermConfig, source_base: Option<PathBuf>) -> io::Result<()> {
    let stdout = io::stdout();
    let handle = stdout.lock();
    let mut out = BufWriter::new(handle);

    // Syntect defaults load lazily inside `flush_code_block` via
    // `syntax_set()` / `highlight_theme()` — not eagerly here — so
    // documents with no fenced code pay zero syntect cost, honoring
    // the "feel like a syscall wrapper" contract in CLAUDE.md.

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);
    opts.insert(Options::ENABLE_DEFINITION_LIST);
    // ENABLE_MATH wires `$inline$` / `$$display$$` into `Event::InlineMath`
    // and `Event::DisplayMath`. docs/Logic.md specifies v1 behavior as
    // "passthrough as code" and `dispatch()` routes both events to
    // `handle_inline_code` so they render in the same reverse-video span as
    // a backtick code run — no TeX layout, no special styling. Any `$`
    // outside a math span (e.g. shell var references in prose) stays
    // literal because pulldown-cmark only parses `$…$` / `$$…$$` as math
    // when the delimiters are balanced and adjacent to non-space text.
    opts.insert(Options::ENABLE_MATH);

    let normalized = preprocess_markdown(markdown);
    let parser = Parser::new_ext(&normalized, opts);

    {
        let writer: &mut dyn Write = &mut out;
        let mut state = RenderState::new(writer, term).with_source_base(source_base);
        for event in parser {
            state.dispatch(event)?;
        }
        state.flush_footnotes()?;
        // Trailing newline so the next shell prompt starts on its own line.
        if state.col_pos != 0 {
            state.write_newline()?;
        }
    }

    out.flush()?;
    Ok(())
}
