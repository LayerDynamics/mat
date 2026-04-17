//! State tests — `TrailingNewlines` counter, `RenderState::new` /
//! `with_source_base` initial invariants, `TableState` field behavior.

use std::path::PathBuf;

use mat::state::{RenderState, TableState, TrailingNewlines};
use mat::terminal::{DEFAULT_CELL_PIXEL_HEIGHT, DEFAULT_CELL_PIXEL_WIDTH, ImageProtocol, TermConfig};

// =====================================================================
// TrailingNewlines monotonic bump
// =====================================================================

#[test]
fn trailing_newlines_zero_bumps_to_one() {
    assert_eq!(TrailingNewlines::Zero.bump(), TrailingNewlines::One);
}

#[test]
fn trailing_newlines_one_bumps_to_two() {
    assert_eq!(TrailingNewlines::One.bump(), TrailingNewlines::Two);
}

#[test]
fn trailing_newlines_two_saturates() {
    assert_eq!(TrailingNewlines::Two.bump(), TrailingNewlines::Two);
}

#[test]
fn trailing_newlines_has_blank_line_only_at_two() {
    assert!(!TrailingNewlines::Zero.has_blank_line());
    assert!(!TrailingNewlines::One.has_blank_line());
    assert!(TrailingNewlines::Two.has_blank_line());
}

// =====================================================================
// RenderState::new initial invariants
// =====================================================================

fn fake_term() -> TermConfig {
    TermConfig {
        is_tty: true,
        render_active: true,
        width: 80,
        cell_pixel_width: DEFAULT_CELL_PIXEL_WIDTH,
        cell_pixel_height: DEFAULT_CELL_PIXEL_HEIGHT,
        color_enabled: false,
        image_protocol: ImageProtocol::None,
        osc8_supported: false,
        allow_absolute_image_paths: false,
    }
}

#[test]
fn render_state_starts_with_two_trailing_newlines_to_suppress_leading_blank() {
    let term = fake_term();
    let mut buf: Vec<u8> = Vec::new();
    let writer: &mut dyn std::io::Write = &mut buf;
    let state = RenderState::new(writer, &term);
    assert_eq!(state.trailing_nl, TrailingNewlines::Two);
    assert_eq!(state.col_pos, 0);
    assert_eq!(state.blockquote_depth, 0);
    assert!(state.list_stack.is_empty());
    assert!(state.style_stack.is_empty());
    assert!(!state.in_code_block);
    assert!(state.pending_link_url.is_none());
    assert!(state.pending_link_text.is_none());
    assert!(state.pending_bullet.is_none());
    assert!(state.source_base.is_none());
    assert!(state.footnote_bodies.is_empty());
    assert_eq!(state.footnote_counter, 0);
    assert!(state.table.is_none());
}

#[test]
fn render_state_with_source_base_sets_the_directory() {
    let term = fake_term();
    let mut buf: Vec<u8> = Vec::new();
    let writer: &mut dyn std::io::Write = &mut buf;
    let base = PathBuf::from("/tmp/docs");
    let state = RenderState::new(writer, &term).with_source_base(Some(base.clone()));
    assert_eq!(state.source_base, Some(base));
}

// =====================================================================
// TableState scratch fields
// =====================================================================

#[test]
fn table_state_round_trips_cells_through_rows() {
    let mut t = TableState {
        aligns: Vec::new(),
        header: Vec::new(),
        rows: Vec::new(),
        current_row: Vec::new(),
        current_cell: String::new(),
        in_header: false,
    };
    t.current_cell.push_str("hello");
    assert_eq!(t.current_cell, "hello");
    let cell = std::mem::take(&mut t.current_cell);
    t.current_row.push(cell);
    assert_eq!(t.current_row, vec!["hello".to_string()]);
    let row = std::mem::take(&mut t.current_row);
    t.rows.push(row);
    assert_eq!(t.rows.len(), 1);
    assert_eq!(t.rows[0][0], "hello");
    assert!(t.current_cell.is_empty());
    assert!(t.current_row.is_empty());
}
