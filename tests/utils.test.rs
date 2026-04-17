//! Utility tests — link-text equality, source-base-dir derivation, and the
//! syntect lazy-init contract (no syntect for codeless docs, once-per-process
//! init for any docs that use fenced code).

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use common::render_to_string;
use mat::config::Source;
use mat::utils::{
    SYNTAX_LOAD_COUNT, THEME_LOAD_COUNT, highlight_theme, link_text_equals_url, source_base_dir,
    syntax_set,
};

#[test]
fn link_text_equals_url_treats_scheme_variations_as_equivalent() {
    assert!(link_text_equals_url("example.com", "https://example.com"));
    assert!(link_text_equals_url("example.com", "http://example.com"));
    assert!(link_text_equals_url(
        "https://example.com/",
        "https://example.com"
    ));
    assert!(link_text_equals_url("example.com", "EXAMPLE.COM"));
    assert!(link_text_equals_url(
        "mailto:a@example.com",
        "a@example.com"
    ));
}

#[test]
fn link_text_equals_url_rejects_distinct_paths() {
    assert!(!link_text_equals_url(
        "click here",
        "https://example.com"
    ));
    assert!(!link_text_equals_url(
        "https://a.example",
        "https://b.example"
    ));
    assert!(!link_text_equals_url(
        "example.com/a",
        "example.com/b"
    ));
}

#[test]
fn source_base_dir_for_file_returns_parent() {
    let src = Source::File(PathBuf::from("/tmp/doc/readme.md"));
    let base = source_base_dir(&src);
    assert_eq!(base, PathBuf::from("/tmp/doc"));
}

#[test]
fn source_base_dir_for_bare_filename_returns_dot() {
    let src = Source::File(PathBuf::from("readme.md"));
    let base = source_base_dir(&src);
    assert_eq!(base, PathBuf::from("."));
}

#[test]
fn source_base_dir_for_stdin_returns_cwd() {
    let base = source_base_dir(&Source::Stdin);
    assert!(
        base == Path::new(".") || base.is_absolute(),
        "stdin base should be CWD or `.`: got {base:?}"
    );
}

#[test]
fn finding05_codeless_render_does_not_load_syntect() {
    // Warm-start first so prior concurrent tests don't skew the delta.
    let _ = syntax_set();
    let _ = highlight_theme();
    let syn_before = SYNTAX_LOAD_COUNT.load(Ordering::SeqCst);
    let theme_before = THEME_LOAD_COUNT.load(Ordering::SeqCst);
    let out = render_to_string("# Title\n\nplain paragraph.\n", false);
    assert!(out.contains("Title"));
    assert_eq!(
        SYNTAX_LOAD_COUNT.load(Ordering::SeqCst),
        syn_before,
        "SyntaxSet must not reload for codeless doc"
    );
    assert_eq!(
        THEME_LOAD_COUNT.load(Ordering::SeqCst),
        theme_before,
        "Theme must not reload for codeless doc"
    );
}

#[test]
fn finding05_syntect_loads_at_most_once() {
    let _ = syntax_set();
    let _ = highlight_theme();
    let syn_base = SYNTAX_LOAD_COUNT.load(Ordering::SeqCst);
    let theme_base = THEME_LOAD_COUNT.load(Ordering::SeqCst);
    for _ in 0..4 {
        let _ = render_to_string("```rust\nfn main() {}\n```\n", false);
    }
    assert_eq!(SYNTAX_LOAD_COUNT.load(Ordering::SeqCst), syn_base);
    assert_eq!(THEME_LOAD_COUNT.load(Ordering::SeqCst), theme_base);
}
