//! Cross-cutting helpers — link-URL comparison, source-directory derivation,
//! and lazy syntect init (SyntaxSet + highlight theme).
//!
//! Kept here rather than beside their callers so the "is this link == url?"
//! policy and the one-syntect-init-per-process contract stay discoverable
//! at a single known location.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::config::Source;

// ---------------------------------------------------------------------------
// syntect lazy-init
// ---------------------------------------------------------------------------
//
// SYNTAX_LOAD_COUNT / THEME_LOAD_COUNT are always-on counters (not
// #[cfg(test)]-gated) so integration tests can observe them. The binary
// never reads them; the counters are cheap atomics.

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static HIGHLIGHT_THEME: OnceLock<Theme> = OnceLock::new();

pub static SYNTAX_LOAD_COUNT: AtomicU32 = AtomicU32::new(0);
pub static THEME_LOAD_COUNT: AtomicU32 = AtomicU32::new(0);

pub fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(|| {
        SYNTAX_LOAD_COUNT.fetch_add(1, Ordering::SeqCst);
        SyntaxSet::load_defaults_newlines()
    })
}

pub fn highlight_theme() -> &'static Theme {
    HIGHLIGHT_THEME.get_or_init(|| {
        THEME_LOAD_COUNT.fetch_add(1, Ordering::SeqCst);
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.values().next())
            .expect("syntect must ship at least one default theme")
            .clone()
    })
}

// ---------------------------------------------------------------------------
// link equality and base-dir derivation
// ---------------------------------------------------------------------------

/// Returns true when the displayed text and the URL refer to the same
/// resource, so the `(url)` fallback would be redundant noise. Treats
/// `https://example.com` and `http://example.com` and bare `example.com`
/// as equivalent.
pub fn link_text_equals_url(text: &str, url: &str) -> bool {
    /// Strip protocol/scheme prefixes and a trailing `/` **on a borrow** so
    /// only the final `to_lowercase` touches the heap (one allocation per
    /// argument, down from three in the previous `String`-mutating version).
    fn normalize(s: &str) -> String {
        let s = s.trim();
        let s = s
            .strip_prefix("https://")
            .or_else(|| s.strip_prefix("http://"))
            .unwrap_or(s);
        let s = s.strip_prefix("mailto:").unwrap_or(s);
        s.trim_end_matches('/').to_lowercase()
    }
    normalize(text) == normalize(url)
}

/// Derive the directory against which relative local-image paths
/// (`![alt](x.png)`) should be resolved. For a file source, this is the
/// file's parent (or `.` if the source is a bare filename). For stdin,
/// there is no trusted root — callers at the `render()` level use `None`
/// to refuse local-image access.
pub fn source_base_dir(source: &Source) -> PathBuf {
    match source {
        Source::File(p) => p
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        Source::Stdin => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}
