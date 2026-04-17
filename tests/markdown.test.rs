//! Markdown tests — preprocessor normalization (indented-list promotion,
//! fenced-code preservation, legitimate-nested-list preservation) and the
//! silent-HTML-strip contract for v1.

mod common;

use common::render_to_string;
use mat::markdown::preprocess_markdown;

// =====================================================================
// preprocess_markdown — promotion rules
// =====================================================================

#[test]
fn preprocessor_promotes_indented_list_items() {
    // 4-space-indented `- ` after a paragraph becomes a real list.
    let md = "**Must Have**\n    - Alpha\n    - Beta\n";
    let out = render_to_string(md, false);
    assert!(out.contains("• Alpha"), "got: {out:?}");
    assert!(out.contains("• Beta"), "got: {out:?}");
}

#[test]
fn preprocessor_preserves_fenced_code() {
    // 4-space indent INSIDE a fenced code block must not be mutated.
    let md = "```\n    - not a list\n```\n";
    let out = render_to_string(md, false);
    assert!(out.contains("    - not a list"), "got: {out:?}");
}

#[test]
fn preprocessor_preserves_legitimate_nested_lists() {
    // `    - Deep` at indent 4 is legitimate CommonMark nesting when the
    // previous non-blank line is itself a list marker. We must NOT promote
    // it — doing so would flatten `▸ Deep` back to `• Deep`.
    let md = "- First\n- Second\n  - Nested A\n  - Nested B\n    - Deep\n";
    let out = render_to_string(md, false);
    assert!(out.contains("• First"), "got: {out:?}");
    assert!(out.contains("◦ Nested A"), "got: {out:?}");
    assert!(out.contains("▸ Deep"), "got: {out:?}");
}

#[test]
fn preprocessor_fast_path_returns_borrowed_when_clean() {
    use std::borrow::Cow;
    // No over-indented list markers → the fast path returns Borrowed with
    // no allocation. Verify by matching on the Cow variant.
    let src = "# title\n\nparagraph.\n\n- one\n- two\n";
    match preprocess_markdown(src) {
        Cow::Borrowed(s) => assert_eq!(s, src),
        Cow::Owned(_) => panic!("clean input must return Cow::Borrowed"),
    }
}

#[test]
fn preprocessor_allocates_owned_only_when_promotion_happens() {
    use std::borrow::Cow;
    let src = "paragraph\n    - promoted\n";
    match preprocess_markdown(src) {
        Cow::Owned(s) => assert!(s.contains("- promoted")),
        Cow::Borrowed(_) => panic!("promotion must produce Cow::Owned"),
    }
}

// =====================================================================
// HTML block / inline HTML dropped in v1
// =====================================================================

#[test]
fn html_block_silently_stripped() {
    let md = "before\n\n<script>alert(1)</script>\n\nafter\n";
    let out = render_to_string(md, false);
    assert!(out.contains("before"));
    assert!(out.contains("after"));
    assert!(
        !out.contains("<script>"),
        "raw HTML must be stripped: {out:?}"
    );
    assert!(
        !out.contains("alert(1)"),
        "script body must not render: {out:?}"
    );
}
