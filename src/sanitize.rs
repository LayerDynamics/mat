//! Output sanitization.
//!
//! Markdown content is attacker-controlled from our perspective. Every byte
//! we hand to the terminal that originates from the input document must go
//! through `sanitize_text` so injected control sequences (ESC / BEL / CSI /
//! C1 controls) cannot prematurely close our own ANSI output, open a foreign
//! OSC sequence (OSC 52 clipboard, OSC 0 title), or otherwise steer the
//! terminal emulator. `write_raw` (in `renderer`) remains available for our
//! own trusted escape bytes (style codes, box-drawing glyphs) so the
//! author/hostile distinction is visible at every call site.

use std::borrow::Cow;

pub fn is_dangerous_control(c: char) -> bool {
    // TAB, LF, CR are the three C0 controls legitimate markdown content
    // legitimately contains (indentation, paragraph breaks, Windows line
    // endings). Everything else under U+0020, plus DEL (U+007F) and the
    // full C1 range (U+0080..=U+009F), is attacker-usable for escape
    // injection and must be stripped.
    let cp = c as u32;
    if cp == 0x09 || cp == 0x0A || cp == 0x0D {
        return false;
    }
    cp < 0x20 || cp == 0x7F || (0x80..=0x9F).contains(&cp)
}

pub fn sanitize_text(s: &str) -> Cow<'_, str> {
    if !s.chars().any(is_dangerous_control) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if !is_dangerous_control(c) {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

pub fn url_safe_for_osc8(url: &str) -> bool {
    !url.is_empty() && url.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// Public alias for `sanitize_text` — call-sites (render handlers, table /
/// footnote buffers) use this name so the intent is self-documenting.
#[inline]
pub fn sanitize_display_text(s: &str) -> Cow<'_, str> {
    sanitize_text(s)
}

/// Sanitize a fenced code-block language tag. The language string is a lookup
/// key for syntect AND is printed verbatim as a dim label above the block; a
/// hostile fence like ```\x1b]0;owned\x07 would set the terminal title without
/// this filter. Allow only a conservative character set that covers every
/// language token syntect ships plus the common compound forms (`c++`, `c#`,
/// `f#`, `shell-session`, `zsh.sh`, `nginx-conf`). Anything outside the set is
/// dropped silently — there is no legitimate reason for a language tag to
/// contain ESC / BEL / whitespace / `/` etc. An over-long tag is truncated so
/// an absurd 64 KiB fence header cannot blow up the label width.
pub fn sanitize_code_lang(raw: &str) -> String {
    const MAX_LANG_LEN: usize = 32;
    let mut out = String::with_capacity(raw.len().min(MAX_LANG_LEN));
    for c in raw.chars() {
        if out.chars().count() >= MAX_LANG_LEN {
            break;
        }
        let keep = c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.' | '_' | '#');
        if keep {
            out.push(c);
        }
    }
    out
}

/// Validate a URL for safe inclusion inside an OSC 8 hyperlink escape
/// (`ESC ] 8 ;; URL ESC \`). Any C0/C1 control byte in the URL could close the
/// OSC early or open a nested escape, letting a hostile URL rewrite terminal
/// state or retarget the visible link. When forbidden bytes are present we
/// return None — the caller MUST then skip OSC 8 emission entirely. We do not
/// "repair" the URL, because a repaired URL would silently retarget the click.
pub fn sanitize_osc_url(url: &str) -> Option<&str> {
    // Delegate the character-class check to `url_safe_for_osc8` — the file's
    // canonical predicate for "what bytes are safe inside OSC 8?". Keeping
    // one policy in one place means a future tweak (e.g. widening to allow
    // percent-encoded UTF-8) only needs editing there.
    if url_safe_for_osc8(url) {
        Some(url)
    } else {
        None
    }
}
