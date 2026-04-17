//! Shared helpers for integration tests.
//!
//! Each `tests/*.test.rs` that needs one of these helpers includes this
//! module with `mod common;` — Cargo ignores subdirectories under `tests/`,
//! so this `mod.rs` is not itself compiled as a standalone test binary.

#![allow(dead_code)]

use std::env;
use std::io::Write;
use std::sync::{Mutex, MutexGuard};

use pulldown_cmark::{Options, Parser};

use mat::markdown::preprocess_markdown;
use mat::state::RenderState;
use mat::terminal::{
    DEFAULT_CELL_PIXEL_HEIGHT, DEFAULT_CELL_PIXEL_WIDTH, ImageProtocol, TermConfig,
};

/// Process-wide lock around anything that mutates `std::env`. cargo test runs
/// tests inside one binary concurrently by default, and `env::set_var` /
/// `env::remove_var` are `unsafe` because they are NOT thread-safe on POSIX —
/// one test racing against another's probe of `TERM_PROGRAM` can return stale
/// or garbage data, producing flaky failures under `-j N`. Every test that
/// touches the env must call `env_lock()` first and hold the returned guard
/// for the duration of its read+mutate+restore window.
pub fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    // A poisoned mutex in this suite means a previous env test panicked
    // mid-mutation; the inner data is `()` and there is nothing to corrupt,
    // so clear poison and proceed rather than cascading unrelated failures.
    match LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn render_to_string(md: &str, color: bool) -> String {
    render_to_string_full(md, color, ImageProtocol::None, false, 80)
}

pub fn render_to_string_full(
    md: &str,
    color: bool,
    image_protocol: ImageProtocol,
    osc8: bool,
    width: usize,
) -> String {
    let term = TermConfig {
        is_tty: true,
        render_active: true,
        width,
        cell_pixel_width: DEFAULT_CELL_PIXEL_WIDTH,
        cell_pixel_height: DEFAULT_CELL_PIXEL_HEIGHT,
        color_enabled: color,
        image_protocol,
        osc8_supported: osc8,
        allow_absolute_image_paths: false,
    };

    let mut buf: Vec<u8> = Vec::new();
    {
        let writer: &mut dyn Write = &mut buf;
        let mut state = RenderState::new(writer, &term);
        let mut opts = Options::empty();
        opts.insert(Options::ENABLE_TABLES);
        opts.insert(Options::ENABLE_FOOTNOTES);
        opts.insert(Options::ENABLE_STRIKETHROUGH);
        opts.insert(Options::ENABLE_TASKLISTS);
        opts.insert(Options::ENABLE_SMART_PUNCTUATION);
        opts.insert(Options::ENABLE_DEFINITION_LIST);
        // Keep the test harness parser options byte-identical to
        // `src/markdown.rs::render` so every test sees the same event
        // stream production does. Drifting these would let a test pass
        // with events that can't actually reach the renderer in prod.
        opts.insert(Options::ENABLE_MATH);
        let normalized = preprocess_markdown(md);
        let parser = Parser::new_ext(&normalized, opts);
        for ev in parser {
            state.dispatch(ev).unwrap();
        }
        state.flush_footnotes().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

/// Build the smallest valid 2×2 RGBA PNG entirely in memory so download
/// tests do not depend on any test fixture files.
pub fn build_test_png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let mut buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    buf
}

/// Spawn a one-shot HTTP responder on `server` that echoes `body` with
/// content-type `ct` and then drops. Used by remote-fetch tests that just
/// want to observe the client round-tripping a single GET.
pub fn tiny_http_echo_once(server: tiny_http::Server, body: Vec<u8>, ct: &str) {
    let ct = ct.to_string();
    std::thread::spawn(move || {
        if let Ok(req) = server.recv() {
            let hdr: tiny_http::Header = format!("Content-Type: {ct}").parse().unwrap();
            let resp = tiny_http::Response::from_data(body).with_header(hdr);
            let _ = req.respond(resp);
        }
    });
}

/// Save the env, run `f` with a clean image-detection env, then restore.
/// Holds `env_lock()` for the duration of the read/mutate/restore window
/// so two concurrent tests in the same binary cannot interleave their env
/// rewrites — without the lock, one test setting `ITERM_SESSION_ID`
/// between our remove and our assertion would make us observe the wrong
/// protocol (flaky Halfblock vs Iterm2/Kitty failures under `-j N`).
pub fn with_clean_image_env<F, R>(set: &[(&str, &str)], f: F) -> R
where
    F: FnOnce() -> R,
{
    let _env_guard = env_lock();
    let names = [
        "KITTY_WINDOW_ID",
        "GHOSTTY_RESOURCES_DIR",
        "ITERM_SESSION_ID",
        "TERM_PROGRAM",
        "TERM",
        "COLORTERM",
        "WEZTERM_EXECUTABLE",
        "VSCODE_INJECTION",
    ];
    let prev: Vec<(String, Option<std::ffi::OsString>)> = names
        .iter()
        .map(|n| (n.to_string(), env::var_os(n)))
        .collect();
    unsafe {
        for n in &names {
            env::remove_var(n);
        }
        for (k, v) in set {
            env::set_var(k, v);
        }
    }
    let out = f();
    unsafe {
        for (k, v) in &prev {
            match v {
                Some(val) => env::set_var(k, val),
                None => env::remove_var(k),
            }
        }
    }
    out
}
