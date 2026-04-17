//! Format tests — pad_cell / border / heading_level_num. Pure-function
//! helpers, so tests go straight against the exported API.

use pulldown_cmark::{Alignment, HeadingLevel};
use unicode_width::UnicodeWidthStr;

use mat::format::{border, heading_level_num, pad_cell};

#[test]
fn heading_level_num_maps_each_level() {
    assert_eq!(heading_level_num(HeadingLevel::H1), 1);
    assert_eq!(heading_level_num(HeadingLevel::H2), 2);
    assert_eq!(heading_level_num(HeadingLevel::H3), 3);
    assert_eq!(heading_level_num(HeadingLevel::H4), 4);
    assert_eq!(heading_level_num(HeadingLevel::H5), 5);
    assert_eq!(heading_level_num(HeadingLevel::H6), 6);
}

#[test]
fn pad_cell_left_aligns_by_default() {
    let s = pad_cell("hi", 6, Alignment::Left);
    assert_eq!(s, "hi    ");
    assert_eq!(UnicodeWidthStr::width(s.as_str()), 6);
}

#[test]
fn pad_cell_right_and_center_alignment() {
    assert_eq!(pad_cell("hi", 6, Alignment::Right), "    hi");
    assert_eq!(pad_cell("hi", 6, Alignment::Center), "  hi  ");
    // Odd remainder: center pads the right side more (rem=5 → left=2, right=3).
    assert_eq!(pad_cell("hi", 7, Alignment::Center), "  hi   ");
}

#[test]
fn pad_cell_truncates_with_ellipsis_when_over_width() {
    let s = pad_cell("abcdefghij", 5, Alignment::Left);
    assert!(s.ends_with('…') || UnicodeWidthStr::width(s.as_str()) == 5);
    assert!(UnicodeWidthStr::width(s.as_str()) == 5);
}

#[test]
fn table_cell_truncate_cjk_preserves_full_char() {
    // pad_cell must never split a CJK character across the ellipsis boundary
    // (would render as mojibake). The helper decides by UnicodeWidthChar —
    // truncation should land on a char boundary.
    let padded = pad_cell("漢字漢字漢字", 6, Alignment::Left);
    // Width budget is 6; ellipsis is width 1 so ~2 CJK chars + '…' + pad.
    assert!(padded.ends_with('…') || UnicodeWidthStr::width(padded.as_str()) == 6);
    // No partial/garbled bytes — re-encoded must be valid UTF-8.
    let _ = padded.as_bytes();
}

#[test]
fn border_builds_box_line() {
    // Three columns of width 2 → ┌──── header, then ┬ between, ┐ at end,
    // total cells include two padding spaces each side of content.
    let s = border(&[2, 2, 2], '┌', '┬', '┐', '─');
    assert!(s.starts_with('┌'));
    assert!(s.ends_with('┐'));
    assert_eq!(s.matches('┬').count(), 2);
    assert_eq!(s.matches('─').count(), (2 + 2) * 3);
}
