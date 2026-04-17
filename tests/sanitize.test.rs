//! Sanitization tests — strip dangerous control bytes from display text,
//! reject ESC/BEL/NUL in URLs destined for OSC 8, and confirm the end-to-end
//! rendered output never leaks attacker-controlled escapes.

mod common;

use common::{render_to_string, render_to_string_full};
use mat::sanitize::{sanitize_display_text, sanitize_osc_url};
use mat::terminal::ImageProtocol;

#[test]
fn sanitize_display_text_strips_c0_c1_but_keeps_tabs_and_newlines() {
    let input = "hello\x1b[31mred\x1b[0m\tworld\nok\x07bell";
    let out = sanitize_display_text(input);
    assert!(!out.contains('\x1b'), "ESC stripped: {out:?}");
    assert!(!out.contains('\x07'), "BEL stripped: {out:?}");
    assert!(out.contains('\t'), "TAB preserved");
    assert!(out.contains('\n'), "LF preserved");
    assert!(out.contains("hello"));
    assert!(out.contains("world"));
}

#[test]
fn sanitize_osc_url_rejects_control_bytes() {
    assert_eq!(
        sanitize_osc_url("https://ok.example/"),
        Some("https://ok.example/")
    );
    assert!(sanitize_osc_url("").is_none());
    assert!(sanitize_osc_url("https://a/\x1b\\b").is_none());
    assert!(sanitize_osc_url("https://a/\x07").is_none());
    assert!(sanitize_osc_url("https://a/\x00").is_none());
    // C1 control (ST) as well — the raw 0x9c byte wrapped in UTF-8.
    assert!(sanitize_osc_url("https://a/\u{009c}x").is_none());
}

#[test]
fn finding01_osc8_rejects_escape_in_url() {
    assert!(sanitize_osc_url("https://example.com/\x1b]52;c;AAAA\x07").is_none());
    assert!(sanitize_osc_url("https://example.com/\x07bell").is_none());
    assert!(sanitize_osc_url("https://example.com/\x1b[31m").is_none());
    assert_eq!(
        sanitize_osc_url("https://example.com/?q=1"),
        Some("https://example.com/?q=1")
    );
}

#[test]
fn finding02_clipboard_osc_stripped_from_rendered_text() {
    let md = "hello \x1b]52;c;AAAA\x07 world";
    let out = render_to_string(md, false);
    assert!(!out.contains('\x1b'), "ESC must be stripped");
    assert!(!out.contains('\x07'), "BEL must be stripped");
    assert!(out.contains("hello"));
    assert!(out.contains("world"));
}

#[test]
fn finding02_inline_code_stripped() {
    let md = "before `code\x1b[31mbad` after";
    let out = render_to_string(md, false);
    assert!(!out.contains('\x1b'));
}

#[test]
fn markdown_with_osc_title_set_in_inline_code_does_not_change_terminal_title() {
    // Inline code containing an OSC 0 (set-title) sequence. Without
    // sanitization the bytes would flow straight to the terminal and rewrite
    // the title. Must be stripped.
    let md = "try `\x1b]0;HACKED\x07` now\n";
    let out = render_to_string(md, true);
    assert!(!out.contains("\x1b]0;"), "OSC 0 must be stripped: {out:?}");
    assert!(!out.contains('\x07'), "BEL must be stripped: {out:?}");
}

#[test]
fn markdown_with_osc52_clipboard_is_stripped() {
    // OSC 52 would write the base64 payload to the terminal clipboard IF an
    // ESC byte from the attacker's input survives to kick off the escape.
    // Our sanitizer strips input ESC / ST, so `]52;c;...` may survive as
    // inert text but no escape can form. The rendered output also contains
    // our OWN style escapes (`\x1b[7m` reverse-video for inline code,
    // `\x1b[0m` reset) — those are trusted renderer output, not attacker
    // bytes. Check the unsafe opener pair is absent.
    let md = "click `\x1b]52;c;aGk=\x1b\\` here\n";
    let out = render_to_string(md, true);
    assert!(
        !out.contains("\x1b]52;"),
        "OSC 52 opener must not survive: {out:?}"
    );
    assert!(
        !out.contains("\x1b]"),
        "no OSC introducer from input must survive: {out:?}"
    );
    assert!(!out.contains('\u{009c}'), "C1 ST must be stripped: {out:?}");
    // Sanity: the surrounding visible text still renders.
    assert!(out.contains("click"));
    assert!(out.contains("here"));
}

#[test]
fn link_url_with_esc_backslash_does_not_break_out_of_osc8() {
    // Exact "Critical injection" regression from the user-supplied audit:
    // ESC + backslash in the URL would close the OSC 8 early and let later
    // bytes run as terminal state. The sanitizer must refuse.
    let raw_url = "https://a.example/\x1b\\https://b.example/";
    let md = format!("[click]({raw_url})\n");
    let out = render_to_string_full(&md, false, ImageProtocol::None, true, 80);
    assert!(
        !out.contains("\x1b]8;;"),
        "must refuse OSC 8 emission for ESC-tainted URL: {out:?}"
    );
    // Display text still renders.
    assert!(out.contains("click"));
}

#[test]
fn osc8_url_with_control_bytes_is_sanitized() {
    // An ESC embedded in the URL would close the hyperlink escape early and
    // let following bytes run as arbitrary terminal state. Must NOT emit any
    // OSC 8 sequence at all — the URL is rejected.
    let url = "https://evil.example.com/\x1b]0;pwned\x07";
    let md = format!("[x]({url})\n");
    let out = render_to_string_full(&md, false, ImageProtocol::None, true, 80);
    assert!(
        !out.contains("\x1b]8;;"),
        "must refuse to emit OSC 8 for ESC-tainted URL; got: {out:?}"
    );
    assert!(
        !out.contains('\x1b') || !out.contains("\x1b]0;"),
        "must not pass the OSC 0 (title) through: {out:?}"
    );
}
