//! Renderer tests — the broad "given markdown, assert what's in the rendered
//! output" suite. Covers paragraphs, headings, lists, blockquotes, code
//! blocks, tables, links, footnotes, OSC 8, word wrap, definition lists, and
//! the various style regressions.

mod common;

use common::{render_to_string, render_to_string_full};
use mat::terminal::ImageProtocol;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// =====================================================================
// Baseline paragraphs, headings, lists, code blocks, tables, rules, wrap
// =====================================================================

#[test]
fn plain_paragraph() {
    let out = render_to_string("hello world\n", false);
    assert!(out.contains("hello world"), "got: {out:?}");
}

#[test]
fn heading_renders_text() {
    let out = render_to_string("# Title\n\nbody\n", false);
    assert!(out.contains("Title"));
    assert!(out.contains("body"));
}

#[test]
fn unordered_list_bullet() {
    let out = render_to_string("- one\n- two\n", false);
    assert!(out.contains("• one"), "got: {out:?}");
    assert!(out.contains("• two"));
}

#[test]
fn ordered_list_number() {
    let out = render_to_string("1. first\n2. second\n", false);
    assert!(out.contains("1. first"));
    assert!(out.contains("2. second"));
}

#[test]
fn code_block_passes_through() {
    let out = render_to_string("```\nlet x = 1;\n```\n", false);
    assert!(out.contains("let x = 1;"));
}

#[test]
fn table_draws_borders() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let out = render_to_string(md, false);
    assert!(out.contains("┌"));
    assert!(out.contains("└"));
    assert!(out.contains("1"));
    assert!(out.contains("2"));
}

#[test]
fn link_without_osc8_shows_url() {
    let out = render_to_string("[docs](https://example.com)\n", false);
    assert!(out.contains("docs"));
    assert!(out.contains("https://example.com"));
}

#[test]
fn horizontal_rule_uses_box_drawing() {
    let out = render_to_string("before\n\n---\n\nafter\n", false);
    assert!(out.contains('─'));
}

#[test]
fn word_wrap_honors_width() {
    // 80-col state; make a 200-char paragraph and ensure newline inserted
    let long: String = "word ".repeat(50);
    let out = render_to_string(&long, false);
    assert!(out.lines().all(|l| l.chars().count() <= 82));
}

// =====================================================================
// Task list markers
// =====================================================================

#[test]
fn task_list_marks_checkbox() {
    let out = render_to_string("- [ ] open\n- [x] done\n", false);
    assert!(out.contains("☐"));
    assert!(out.contains("☑"));
}

#[test]
fn task_list_does_not_double_bullet() {
    // Before the fix, task items rendered as "• ☐ open" — ugly and wrong.
    // Checkbox must replace the bullet entirely.
    let out = render_to_string("- [ ] open\n", false);
    assert!(
        !out.contains("• ☐") && !out.contains("•  ☐"),
        "bullet must not precede the task marker; got: {out:?}"
    );
    assert!(out.contains("☐ open"), "got: {out:?}");
}

// =====================================================================
// Inline code / style escapes
// =====================================================================

#[test]
fn inline_code_at_document_start_emits_opening_escape() {
    // Regression: the very first inline-code span of a document was losing
    // its opening \x1b[7m because push_style was called AFTER
    // ensure_bol_styled and skipped emit at col 0.
    let out = render_to_string("`cat` is a thing.\n", true);
    assert!(
        out.contains("\x1b[7m cat "),
        "opening reverse-video must be present; got: {out:?}"
    );
}

// =====================================================================
// Indentation
// =====================================================================

#[test]
fn top_level_unordered_list_has_no_leading_indent() {
    let md = "- one\n- two\n";
    let out = render_to_string(md, false);
    for line in out.lines() {
        if line.contains("one") || line.contains("two") {
            assert!(
                line.starts_with("• "),
                "depth-1 list line must start with bullet, got: {line:?}"
            );
        }
    }
}

// =====================================================================
// Footnotes
// =====================================================================

#[test]
fn footnote_reference_and_body() {
    let out = render_to_string("claim[^a].\n\n[^a]: source\n", false);
    assert!(out.contains("[1]"));
    assert!(out.contains("source"));
}

#[test]
fn footnote_numbers_match_reference_order() {
    let md = "\
first[^a] then[^b] and[^c].\n\
\n\
[^c]: third body\n\
[^a]: first body\n\
[^b]: second body\n";
    let out = render_to_string(md, false);
    let first_idx = out.find("first").unwrap();
    let then_idx = out.find("then").unwrap();
    let and_idx = out.find("and").unwrap();
    let m1 = out[first_idx..].find("[1]").unwrap() + first_idx;
    let m2 = out[then_idx..].find("[2]").unwrap() + then_idx;
    let m3 = out[and_idx..].find("[3]").unwrap() + and_idx;
    assert!(m1 < m2 && m2 < m3, "markers in reference order: {out:?}");
    let b1 = out.find("first body").unwrap();
    let b2 = out.find("second body").unwrap();
    let b3 = out.find("third body").unwrap();
    assert!(
        b1 < b2 && b2 < b3,
        "bodies emitted in numeric order: {out:?}"
    );
}

#[test]
fn footnote_forward_reference_defined_later() {
    let md = "[^z]: body defined first\n\nthen use[^z].\n";
    let out = render_to_string(md, false);
    assert!(out.contains("[1]"));
    assert!(out.contains("body defined first"));
}

#[test]
fn footnote_reference_without_definition_still_renders_marker() {
    let md = "see[^orphan] here\n";
    let out = render_to_string(md, false);
    assert!(out.contains("see"));
    assert!(out.contains("here"));
    assert!(
        out.contains("orphan") || out.contains("[^orphan]"),
        "orphan marker preserved: {out:?}"
    );
}

// =====================================================================
// Images in tables — placeholder in cell
// =====================================================================

#[test]
fn image_in_table_does_not_emit_viuer_bytes_above_table() {
    let md = "| col |\n|---|\n| ![logo](missing.png) |\n";
    let out = render_to_string_full(md, false, ImageProtocol::Halfblock, false, 40);
    let table_start = out.find('┌').expect("table top border");
    let pre = &out[..table_start];
    assert!(
        !pre.contains("[image:"),
        "image placeholder must not leak above table; got pre: {pre:?}"
    );
}

#[test]
fn image_in_table_renders_placeholder_in_cell() {
    let md = "| col |\n|---|\n| ![logo](missing.png) |\n";
    let out = render_to_string_full(md, false, ImageProtocol::Halfblock, false, 40);
    assert!(
        out.contains("[image: logo]"),
        "alt text must appear inside the table cell; got: {out:?}"
    );
}

// =====================================================================
// Links: URL de-duplication + distinct text
// =====================================================================

#[test]
fn autolink_does_not_double_print_url() {
    let md = "<https://example.com>\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 80);
    let count = out.matches("example.com").count();
    assert_eq!(
        count, 1,
        "URL must appear once, not twice; got: {out:?} (count={count})"
    );
}

#[test]
fn distinct_display_text_still_appends_url() {
    let md = "[click here](https://example.com)\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 80);
    assert!(out.contains("click here"), "display text present: {out:?}");
    assert!(out.contains("https://example.com"), "URL present: {out:?}");
}

// =====================================================================
// Width overrides at upper and lower bounds
// =====================================================================

#[test]
fn width_200_actually_uses_200_cols() {
    let long = "word ".repeat(50); // 250 chars total
    let out = render_to_string_full(&long, false, ImageProtocol::None, false, 200);
    let max_line = out.lines().map(|l| l.chars().count()).max().unwrap_or(0);
    assert!(
        max_line > 120,
        "expected wrapping at >120 cols, got max line len {max_line}; out: {out:?}"
    );
}

#[test]
fn width_extremes_are_clamped_safely() {
    let long = "word ".repeat(50);
    let out = render_to_string_full(&long, false, ImageProtocol::None, false, 30);
    assert!(
        out.lines().all(|l| l.chars().count() <= 32),
        "30-col render must wrap inside 30 cols (got: {out:?})"
    );
}

// =====================================================================
// Definition lists
// =====================================================================

#[test]
fn definition_list_emits_term_and_definition() {
    let md = "Term\n: Definition body\n";
    let out = render_to_string(md, false);
    assert!(out.contains("Term"), "term must render: {out:?}");
    assert!(
        out.contains("Definition body"),
        "definition must render: {out:?}"
    );
}

#[test]
fn definition_list_styles_term_bold_and_definition_italic() {
    let md = "Term\n: Body\n";
    let out = render_to_string(md, true);
    assert!(out.contains("\x1b[1m"), "bold escape for term: {out:?}");
    assert!(out.contains(": "), "definition gutter present: {out:?}");
    assert!(out.contains("\x1b[3m"), "italic for body: {out:?}");
}

// =====================================================================
// Heading palette — rendered output confirms style wiring
// =====================================================================

#[test]
fn rendered_h2_emits_bright_white_escape() {
    let out = render_to_string("## Title\n", true);
    assert!(
        out.contains("\x1b[1;97m"),
        "H2 must render bright white; got: {out:?}"
    );
}

// =====================================================================
// Wrapped-list-item continuation indent
// =====================================================================

#[test]
fn wrapped_list_item_continuation_keeps_indent() {
    let long_item: String = "alpha ".repeat(20);
    let md = format!("- {long_item}\n- short\n");
    let out = render_to_string_full(&md, false, ImageProtocol::None, false, 30);
    let mut lines = out.lines().filter(|l| !l.is_empty());
    let first = lines.next().expect("first line").to_string();
    assert!(
        first.starts_with("• "),
        "first line is the bullet: {first:?}"
    );
    let continuation = out
        .lines()
        .find(|l| l.contains("alpha") && !l.starts_with("• "));
    if let Some(c) = continuation {
        assert!(
            c.starts_with("  "),
            "wrapped continuation must keep 2-space list indent; got: {c:?}"
        );
    }
}

#[test]
fn softbreak_in_list_keeps_indent() {
    let md = "- first line\n  second line\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 80);
    let out_narrow = render_to_string_full(md, false, ImageProtocol::None, false, 18);
    let continuation = out_narrow
        .lines()
        .find(|l| l.contains("second") && !l.contains("first"));
    if let Some(c) = continuation {
        assert!(
            c.starts_with("  "),
            "continuation line must be indented under the bullet; got: {c:?}"
        );
    }
    assert!(out.contains("first line second line"));
}

// =====================================================================
// Unicode width
// =====================================================================

#[test]
fn word_wrap_cjk_doublewidth() {
    let md = "漢字 漢字 漢字 漢字 漢字 漢字\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 10);
    for line in out.lines() {
        let w = UnicodeWidthStr::width(line);
        assert!(
            w <= 10,
            "CJK wrap must respect width=10; got w={w}, line={line:?}"
        );
    }
    assert_eq!(UnicodeWidthStr::width("漢"), 2);
    assert_eq!(UnicodeWidthChar::width('字').unwrap_or(0), 2);
}

#[test]
fn word_wrap_emoji_zwj() {
    let md = "hi 👨‍👩‍👧 friend\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 15);
    assert!(out.contains("hi"));
    assert!(out.contains("friend"));
    for line in out.lines() {
        assert!(
            UnicodeWidthStr::width(line) <= 20,
            "wrap overflow on {line:?}"
        );
    }
}

#[test]
fn word_wrap_combining_marks() {
    let md = "cafe\u{0301} is open\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 12);
    assert!(
        out.contains("cafe\u{0301}"),
        "combining mark preserved: {out:?}"
    );
}

// =====================================================================
// Nested styles / blockquotes
// =====================================================================

#[test]
fn nested_blockquote_within_list() {
    let md = "- item\n\n  > quoted text\n";
    let out = render_to_string(md, false);
    assert!(out.contains("• item"), "bullet rendered: {out:?}");
    assert!(out.contains("quoted text"), "quote body rendered: {out:?}");
    let quoted = out.lines().find(|l| l.contains("quoted text")).unwrap();
    assert!(quoted.contains('|'), "quote gutter present: {quoted:?}");
}

// =====================================================================
// Code-block highlighting
// =====================================================================

#[test]
fn code_block_rust_keyword_highlighted() {
    let md = "```rust\nfn main() { let x = 1; }\n```\n";
    let out = render_to_string(md, true);
    assert!(out.contains("fn"), "fn keyword present: {out:?}");
    assert!(out.contains("let"), "let keyword present: {out:?}");
    assert!(out.contains('x'), "identifier x present: {out:?}");
    assert!(
        out.contains("\x1b[38;2;"),
        "24-bit fg escape expected: {out:?}"
    );
}

#[test]
fn code_block_unknown_lang_falls_back_plaintext() {
    let md = "```klingon\nQoH nuv\n```\n";
    let out = render_to_string(md, true);
    assert!(out.contains("QoH nuv"), "plain-text fallback: {out:?}");
    assert!(out.contains("klingon"), "language label present: {out:?}");
}

// =====================================================================
// Table edge cases
// =====================================================================

#[test]
fn table_wider_than_term_truncates_with_ellipsis() {
    let md = "\
| alpha | beta | gamma |\n\
|-------|------|-------|\n\
| one-very-long-cell | two | three-also-quite-long |\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 20);
    assert!(
        out.contains('…'),
        "ellipsis must appear on truncation: {out:?}"
    );
}

#[test]
fn table_empty_has_no_output() {
    let md = "|   |\n|---|\n";
    let out = render_to_string(md, false);
    assert!(
        out.contains('┌') || out.trim().is_empty(),
        "no-panic is enough; got: {out:?}"
    );
}

#[test]
fn table_with_ragged_rows() {
    let md = "\
| a | b | c |\n\
|---|---|---|\n\
| 1 | 2 | 3 |\n\
| 4 | 5 |\n";
    let out = render_to_string(md, false);
    assert!(out.contains('4'));
    assert!(out.contains('5'));
    assert!(out.contains("b"));
}

// =====================================================================
// OSC 8 hyperlinks
// =====================================================================

#[test]
fn osc8_emits_open_and_close_sequences() {
    let md = "[docs](https://example.com/api)\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, true, 80);
    assert!(
        out.contains("\x1b]8;;https://example.com/api\x1b\\"),
        "OSC 8 open present: {out:?}"
    );
    assert!(
        out.contains("\x1b]8;;\x1b\\"),
        "OSC 8 close present: {out:?}"
    );
}

#[test]
fn osc8_empty_url_does_not_emit_open_sequence() {
    let md = "[anchor]()\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, true, 80);
    assert!(
        !out.contains("\x1b]8;;\x1b\\"),
        "must emit neither open nor close for empty URL: {out:?}"
    );
}

#[test]
fn osc8_opener_stays_with_first_word_across_wrap() {
    // Regression: before the deferred-emission fix, a link near the end of a
    // line would emit `\x1b]8;;URL\x1b\\` on the prior line, then wrap, then
    // write the link text on the next line. Terminals that treat `\n` as an
    // implicit OSC 8 terminator lose the clickable region. The opener must
    // now be flushed *after* the wrap, on the same line as its first word.
    let prefix = "a".repeat(32);
    let md = format!("{prefix} [linktext](https://example.com/u)\n");
    let out = render_to_string_full(&md, true, ImageProtocol::None, true, 40);
    let opener = "\x1b]8;;https://example.com/u\x1b\\";
    let idx = out
        .find(opener)
        .unwrap_or_else(|| panic!("opener present: {out:?}"));
    let after = &out[idx + opener.len()..];
    let line_end = after.find('\n').unwrap_or(after.len());
    let same_line = &after[..line_end];
    assert!(
        same_line.contains("linktext"),
        "OSC 8 opener must share a visual line with its first token — got \
         opener-to-newline slice {same_line:?} in full output {out:?}"
    );
}

#[test]
fn osc8_link_with_empty_display_emits_no_escape() {
    // A link with empty display text must not emit an orphan opener or
    // closer — `TagEnd::Link` discards the deferred opener silently.
    let md = "[](https://example.com/u)\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, true, 80);
    assert!(
        !out.contains("\x1b]8;;"),
        "no OSC 8 escape when link had no display text: {out:?}"
    );
}

// =====================================================================
// Pathological inputs
// =====================================================================

#[test]
fn empty_input_renders_nothing() {
    let out = render_to_string("", false);
    assert!(out.trim().is_empty(), "empty input → empty output: {out:?}");
}

#[test]
fn huge_paragraph_does_not_overflow_col_pos() {
    let long = "alpha ".repeat(10_000);
    let out = render_to_string_full(&long, false, ImageProtocol::None, false, 80);
    for line in out.lines() {
        assert!(line.chars().count() <= 90, "wrap respected: {line:?}");
    }
}

#[test]
fn deeply_nested_lists_10_levels() {
    let mut md = String::new();
    for i in 0..10 {
        for _ in 0..i {
            md.push_str("  ");
        }
        md.push_str("- level\n");
    }
    let out = render_to_string_full(&md, false, ImageProtocol::None, false, 120);
    assert!(out.matches("level").count() >= 10);
}

#[test]
fn inline_code_longer_than_term_width() {
    let md = "prefix `xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx` suffix\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 40);
    assert!(out.contains("prefix"));
    assert!(out.contains("suffix"));
    assert!(out.contains('x'));
}

#[test]
fn link_with_empty_url() {
    let md = "see [anchor]() now\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, true, 80);
    assert!(out.contains("anchor"));
    assert!(
        !out.contains("\x1b]8;;"),
        "no OSC 8 sequence for empty URL: {out:?}"
    );
}

#[test]
fn link_with_unicode_url() {
    let md = "[docs](https://例え.jp/path)\n";
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 80);
    assert!(out.contains("例え.jp"), "unicode URL preserved: {out:?}");
}

// =====================================================================
// Word wrap boundaries
// =====================================================================

#[test]
fn word_wrap_exact_boundary_no_extra_newline() {
    let md = "1234567890\n"; // 10 chars
    let out = render_to_string_full(md, false, ImageProtocol::None, false, 10);
    let n_nonblank = out.lines().filter(|l| !l.is_empty()).count();
    assert_eq!(n_nonblank, 1, "exactly one content line; got: {out:?}");
}

#[test]
fn word_wrap_single_word_longer_than_width() {
    let long_word = "x".repeat(200);
    let md = format!("before {long_word} after\n");
    let out = render_to_string_full(&md, false, ImageProtocol::None, false, 30);
    assert!(out.contains(&long_word), "long word preserved: {out:?}");
    assert!(out.contains("before"));
    assert!(out.contains("after"));
}

// =====================================================================
// docs/ANSI.md — heading palette end-to-end
// Every level from H1 through H6 must render with its level-specific
// escape sequence (docs/ANSI.md Headings table). Covers the full palette
// in one pass so a style-table regression fails loudly.
// =====================================================================

#[test]
fn heading_all_six_levels_emit_level_specific_escapes() {
    let cases: [(&str, &str); 6] = [
        ("# H1\n", "\x1b[1;4;96m"),   // bold + underline + bright cyan
        ("## H2\n", "\x1b[1;97m"),    // bold + bright white
        ("### H3\n", "\x1b[1;37m"),   // bold + regular white
        ("#### H4\n", "\x1b[1;2;37m"), // bold + dim white
        ("##### H5\n", "\x1b[1;2m"),  // bold + dim
        ("###### H6\n", "\x1b[3;2m"), // italic + dim
    ];
    for (md, expected) in cases {
        let out = render_to_string(md, true);
        assert!(
            out.contains(expected),
            "{md:?} must emit {expected:?}; got: {out:?}"
        );
    }
}

// =====================================================================
// docs/ANSI.md — inline formatting escapes
// Standalone `**bold**`, `*italic*`, and `~~strike~~` must emit their
// exact SGR codes (1, 3, 9) even outside of compound combinations.
// =====================================================================

#[test]
fn bold_italic_strike_emit_specific_escapes() {
    let bold = render_to_string("**B**\n", true);
    assert!(bold.contains("\x1b[1m"), "bold SGR 1 present: {bold:?}");
    assert!(bold.contains('B'));

    let ital = render_to_string("*I*\n", true);
    assert!(ital.contains("\x1b[3m"), "italic SGR 3 present: {ital:?}");
    assert!(ital.contains('I'));

    let strike = render_to_string("~~S~~\n", true);
    assert!(
        strike.contains("\x1b[9m"),
        "strikethrough SGR 9 present: {strike:?}"
    );
    assert!(strike.contains('S'));
}

// =====================================================================
// docs/ANSI.md — paragraph boundary
// Two paragraphs separated by a blank line must render with a blank line
// between them in the output (i.e. "\n\n" boundary preserved).
// =====================================================================

#[test]
fn two_paragraphs_separated_by_blank_line() {
    let out = render_to_string("first para\n\nsecond para\n", false);
    let first = out.find("first para").expect("first present");
    let second = out.find("second para").expect("second present");
    let between = &out[first + "first para".len()..second];
    assert!(
        between.contains("\n\n"),
        "paragraphs must be separated by a blank line; got between: {between:?}"
    );
}

// =====================================================================
// docs/ANSI.md — HardBreak emits `\n`, not a joining space
// Markdown: two trailing spaces at end of a line produces a HardBreak
// event. Renderer must emit a literal newline inside the paragraph so
// the second line starts on its own row.
// =====================================================================

#[test]
fn hard_break_emits_literal_newline_mid_paragraph() {
    let md = "line one  \nline two\n";
    let out = render_to_string(md, false);
    let one = out.find("line one").expect("line one present");
    let two = out.find("line two").expect("line two present");
    let between = &out[one + "line one".len()..two];
    assert!(
        between.contains('\n'),
        "HardBreak must emit a newline between the two lines; got: {between:?}"
    );
    assert!(
        !between.starts_with(' ') || between.contains('\n'),
        "HardBreak must not be collapsed into a SoftBreak space: {between:?}"
    );
}

// =====================================================================
// docs/ANSI.md — horizontal rule
// `---` renders as `─` repeated to fill (width - indent) in dim. The
// existing `horizontal_rule_uses_box_drawing` only asserts one `─`; this
// test locks in both the dim escape and that the rule actually spans
// the available width.
// =====================================================================

#[test]
fn horizontal_rule_is_dim_and_spans_available_width() {
    let out = render_to_string_full("---\n", true, ImageProtocol::None, false, 20);
    assert!(
        out.contains("\x1b[2m"),
        "HR must open with dim escape: {out:?}"
    );
    // Rule line must contain exactly `width` copies of `─` (no indent at
    // top level). Find the line that is made up entirely of the rule.
    let rule_line = out
        .lines()
        .find(|l| l.contains('─'))
        .expect("rule line present");
    let dash_count = rule_line.matches('─').count();
    assert_eq!(
        dash_count, 20,
        "HR must span full 20-col width; got {dash_count} in {rule_line:?}"
    );
}

// =====================================================================
// docs/ANSI.md — blockquote gutter
// Blockquote lines must carry the `│` (U+2502) vertical bar in dim
// (`\x1b[2m`) in color mode. Without color the ASCII fallback `|` is
// already covered by `nested_blockquote_within_list`.
// =====================================================================

#[test]
fn blockquote_gutter_is_box_drawing_and_dim_in_color_mode() {
    let out = render_to_string("> quoted text\n", true);
    assert!(
        out.contains("\x1b[2m│"),
        "blockquote gutter must be `│` (U+2502) wrapped in dim; got: {out:?}"
    );
    assert!(out.contains("quoted text"));
}

#[test]
fn nested_blockquote_emits_double_gutter() {
    // `>> nested` creates blockquote_depth=2 which must emit two gutter
    // bars before the content on the same line.
    let out = render_to_string("> outer\n>\n> > nested quote\n", false);
    let nested = out
        .lines()
        .find(|l| l.contains("nested quote"))
        .expect("nested line present");
    let bars = nested.matches('|').count();
    assert!(
        bars >= 2,
        "nested blockquote must show two gutter bars, got {bars} in {nested:?}"
    );
}

// =====================================================================
// docs/ANSI.md — task-list checked marker is dim
// `- [x] done` must render `☑ done` with the marker wrapped in dim so a
// checked task visually recedes ("done").
// =====================================================================

#[test]
fn task_list_checked_marker_is_dim() {
    let out = render_to_string("- [x] done item\n", true);
    let marker_idx = out.find("☑").expect("checked marker present");
    let before = &out[..marker_idx];
    assert!(
        before.ends_with("\x1b[2m"),
        "checked ☑ must be preceded by dim escape; got tail: {:?}",
        &before[before.len().saturating_sub(12)..]
    );
}

// =====================================================================
// docs/ANSI.md — table header row is bold + uses `╞═╪═╡` separator
// =====================================================================

#[test]
fn table_header_cells_are_bold_in_color_mode() {
    let md = "| name | age |\n|---|---|\n| Ada | 37 |\n";
    let out = render_to_string(md, true);
    // Header cell "name" must be wrapped in bold open/close.
    let name_idx = out.find("name").expect("header cell rendered");
    let before = &out[..name_idx];
    assert!(
        before.contains("\x1b[1m"),
        "header cell must open bold: {out:?}"
    );
    let after = &out[name_idx + 4..];
    assert!(
        after.contains("\x1b[22m"),
        "header cell must close bold with SGR 22: {after:?}"
    );
}

#[test]
fn table_separator_uses_double_horizontal_box_drawing() {
    let md = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let out = render_to_string(md, false);
    // docs/ANSI.md calls out `╞═╪═╡` for the header separator.
    assert!(out.contains('╞'), "left double-T present: {out:?}");
    assert!(out.contains('╪'), "middle double-cross present: {out:?}");
    assert!(out.contains('╡'), "right double-T present: {out:?}");
    assert!(out.contains('═'), "double-horizontal present: {out:?}");
}

// =====================================================================
// docs/ANSI.md — code block language label is dim + italic
// =====================================================================

#[test]
fn code_block_language_label_is_dim_italic() {
    let out = render_to_string("```rust\nfn main() {}\n```\n", true);
    assert!(
        out.contains("\x1b[2;3mrust\x1b[0m"),
        "language label must be dim+italic: {out:?}"
    );
}

// =====================================================================
// docs/Logic.md — ENABLE_SMART_PUNCTUATION is on
// Straight quotes, triple-dot ellipsis, and double-hyphen en-dash must
// be converted by pulldown-cmark before the renderer sees them.
// =====================================================================

#[test]
fn smart_punctuation_converts_quotes_dashes_and_ellipsis() {
    let out = render_to_string("\"hello\" -- world...\n", false);
    assert!(
        out.contains('\u{201C}') || out.contains('\u{201D}'),
        "curly double-quotes must replace ASCII: {out:?}"
    );
    assert!(
        out.contains('\u{2013}') || out.contains('\u{2014}'),
        "-- must become en/em dash: {out:?}"
    );
    assert!(
        out.contains('\u{2026}'),
        "... must become ellipsis (U+2026): {out:?}"
    );
}

// =====================================================================
// docs/ANSI.md — inline HTML silently stripped mid-paragraph
// The existing `html_block_silently_stripped` covers block-level HTML.
// This covers the inline case (`Event::InlineHtml`) per the v1 contract.
// =====================================================================

#[test]
fn inline_html_tags_silently_stripped_mid_paragraph() {
    let out = render_to_string("before <em>middle</em> after\n", false);
    assert!(out.contains("before"));
    assert!(out.contains("middle"), "inner text preserved: {out:?}");
    assert!(out.contains("after"));
    assert!(
        !out.contains("<em>") && !out.contains("</em>"),
        "inline HTML tags must be stripped: {out:?}"
    );
}

// =====================================================================
// docs/Logic.md — Event::InlineMath / DisplayMath → passthrough as code
//
// docs/Logic.md:85 specifies that math events render as code in v1.
// ENABLE_MATH must be on in both production (`src/markdown.rs`) and the
// test harness (`tests/common/mod.rs`), and `src/renderer.rs` must route
// both event variants to `handle_inline_code` so they share the exact
// reverse-video treatment as a backtick span.
// =====================================================================

#[test]
fn inline_math_renders_as_reverse_video_code() {
    let out = render_to_string("ratio $x+1$ please\n", true);
    // Math body must appear and be wrapped like inline code:
    // reverse-video on, space-padded content, full reset after.
    assert!(
        out.contains("\x1b[7m x+1 "),
        "inline math must render as a reverse-video code span with space \
         padding on each side: {out:?}"
    );
    // Surrounding prose must be preserved.
    assert!(out.contains("ratio"));
    assert!(out.contains("please"));
    // The raw `$` delimiters must not leak through.
    assert!(
        !out.contains("$x+1$"),
        "raw `$` delimiters must be consumed by the parser: {out:?}"
    );
}

#[test]
fn display_math_renders_as_reverse_video_code() {
    let out = render_to_string("$$E = mc^2$$\n", true);
    // Display math uses the same handler as InlineMath per v1 spec.
    assert!(
        out.contains("\x1b[7m"),
        "display math must open the reverse-video escape: {out:?}"
    );
    assert!(
        out.contains("E = mc^2") || out.contains("E  mc^2"),
        "display math body must render verbatim (smart-punctuation may \
         reflow the `=` spacing but the body characters must survive): \
         {out:?}"
    );
    // The `$$` delimiters must not leak through.
    assert!(
        !out.contains("$$"),
        "raw `$$` delimiters must be consumed: {out:?}"
    );
}

#[test]
fn dollar_sign_without_matching_pair_is_literal() {
    // Pulldown-cmark's math parser requires balanced delimiters; a lone
    // shell-var reference like `$TERM` must remain literal so prose that
    // happens to mention env vars (e.g. README.md, docs/ANSI.md) keeps
    // rendering verbatim after ENABLE_MATH is turned on.
    let out = render_to_string("check $TERM now\n", false);
    assert!(
        out.contains("$TERM"),
        "unbalanced `$` must remain literal, not be consumed as math: {out:?}"
    );
}
