//! Style tests — confirm the heading palette matches docs/ANSI.md and that
//! style-stack resets correctly surround nested bold / italic / strike runs.

mod common;

use common::render_to_string;
use mat::style::{StyleFlag, style_flag_ansi};

#[test]
fn h2_and_h3_use_white_not_cyan() {
    let h2 = style_flag_ansi(StyleFlag::Heading(2));
    let h3 = style_flag_ansi(StyleFlag::Heading(3));
    assert_eq!(h2, "\x1b[1;97m", "H2 must be bold + bright white");
    assert_eq!(h3, "\x1b[1;37m", "H3 must be bold + regular white");
    // H1 stays bright cyan + underline as the spec allows
    // ("bright white/cyan").
    assert!(style_flag_ansi(StyleFlag::Heading(1)).contains("96"));
}

#[test]
fn style_reset_after_inline_emphasis() {
    // Regression: after **bold** ended we used to leak bold into "rest".
    let out = render_to_string("**bold** rest", true);
    assert!(out.contains("\x1b[1mbold"), "bold should open: {out:?}");
    // Between the end of the bold word and "rest" there must be a reset.
    let boldend = out.find("bold").unwrap() + "bold".len();
    let slice = &out[boldend..];
    assert!(
        slice.starts_with("\x1b[0m"),
        "reset must follow bold; got tail: {:?}",
        &slice[..slice.len().min(12)]
    );
}

#[test]
fn nested_bold_italic_strike_reset_order() {
    // ***~~hi~~*** — bold + italic + strike. Reset after must clear all.
    let out = render_to_string("***~~hi~~*** rest\n", true);
    // All three opening escapes must appear before "hi".
    let hi = out.find("hi").unwrap();
    let prefix = &out[..hi];
    assert!(prefix.contains("\x1b[1m"), "bold opened: {out:?}");
    assert!(prefix.contains("\x1b[3m"), "italic opened: {out:?}");
    assert!(prefix.contains("\x1b[9m"), "strike opened: {out:?}");
    // After "hi" there must be a full reset before "rest" is written.
    let rest = out.find("rest").unwrap();
    let between = &out[hi + 2..rest];
    assert!(
        between.contains("\x1b[0m"),
        "full reset before 'rest': {between:?}"
    );
}
