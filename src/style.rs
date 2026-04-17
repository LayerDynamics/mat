//! ANSI styling primitives.
//!
//! `StyleFlag` is the closed set of styles we ever push onto the render
//! `style_stack`. `style_flag_ansi` maps each to the exact ANSI escape used
//! when color is enabled. Keeping both in one module means changing the
//! palette (e.g. heading colors) touches one file.

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum StyleFlag {
    Bold,
    Italic,
    Strike,
    Dim,
    Underline,
    Heading(u8),
    BlockQuote,
    InlineCode,
}

pub fn style_flag_ansi(f: StyleFlag) -> &'static str {
    match f {
        StyleFlag::Bold => "\x1b[1m",
        StyleFlag::Italic => "\x1b[3m",
        StyleFlag::Strike => "\x1b[9m",
        StyleFlag::Dim => "\x1b[2m",
        StyleFlag::Underline => "\x1b[4m",
        // Per docs/ANSI.md headings table: H1 bold+underline+bright cyan,
        // H2 bold + bright white, H3 bold + regular white, H4 bold + dim
        // white, H5 bold + dim, H6 dim italic.
        StyleFlag::Heading(1) => "\x1b[1;4;96m",
        StyleFlag::Heading(2) => "\x1b[1;97m",
        StyleFlag::Heading(3) => "\x1b[1;37m",
        StyleFlag::Heading(4) => "\x1b[1;2;37m",
        StyleFlag::Heading(5) => "\x1b[1;2m",
        StyleFlag::Heading(_) => "\x1b[3;2m",
        StyleFlag::BlockQuote => "\x1b[2;3m",
        StyleFlag::InlineCode => "\x1b[7m",
    }
}
