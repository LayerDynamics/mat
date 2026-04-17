//! Formatting helpers — table padding, box borders, heading-level numbering.
//!
//! Pure functions with no state. Used by the renderer when drawing tables
//! (`pad_cell`, `border`) and by event dispatch to convert
//! `pulldown_cmark::HeadingLevel` into the numeric level the style stack
//! keys on (`heading_level_num`).

use pulldown_cmark::{Alignment, HeadingLevel};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn heading_level_num(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub fn border(widths: &[usize], left: char, mid: char, right: char, h: char) -> String {
    let mut s = String::new();
    s.push(left);
    for (i, w) in widths.iter().enumerate() {
        for _ in 0..(*w + 2) {
            s.push(h);
        }
        if i + 1 == widths.len() {
            s.push(right);
        } else {
            s.push(mid);
        }
    }
    s
}

pub fn pad_cell(text: &str, width: usize, align: Alignment) -> String {
    let current = UnicodeWidthStr::width(text);
    if current > width {
        let mut out = String::new();
        let mut acc = 0usize;
        for c in text.chars() {
            let cw = UnicodeWidthChar::width(c).unwrap_or(0);
            if acc + cw + 1 > width {
                break;
            }
            out.push(c);
            acc += cw;
        }
        out.push('…');
        while UnicodeWidthStr::width(out.as_str()) < width {
            out.push(' ');
        }
        return out;
    }
    let rem = width - current;
    match align {
        Alignment::Right => {
            let mut s = String::new();
            for _ in 0..rem {
                s.push(' ');
            }
            s.push_str(text);
            s
        }
        Alignment::Center => {
            let left = rem / 2;
            let right = rem - left;
            let mut s = String::new();
            for _ in 0..left {
                s.push(' ');
            }
            s.push_str(text);
            for _ in 0..right {
                s.push(' ');
            }
            s
        }
        Alignment::Left | Alignment::None => {
            let mut s = String::from(text);
            for _ in 0..rem {
                s.push(' ');
            }
            s
        }
    }
}
