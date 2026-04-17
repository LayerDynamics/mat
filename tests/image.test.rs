//! Image tests — remote fetch (happy path, rejects, timeouts, redirects),
//! local path traversal fallback, image-in-table placeholder, and the TTY
//! short-circuit in RenderState::render_image.

mod common;

use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use common::{build_test_png, render_to_string_full, tiny_http_echo_once};
use mat::image::{MAX_REMOTE_IMAGE_BYTES, fetch_remote_image_to_temp};
use mat::resolve::AllowLoopbackGuard;
use mat::state::RenderState;
use mat::terminal::{DEFAULT_CELL_PIXEL_HEIGHT, DEFAULT_CELL_PIXEL_WIDTH, ImageProtocol, TermConfig};

// =====================================================================
// Remote fetch — happy path
// =====================================================================

#[test]
fn remote_image_no_longer_emits_remote_suffix() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let png = build_test_png();
    std::thread::spawn(move || {
        if let Ok(req) = server.recv() {
            let resp = tiny_http::Response::from_data(png.clone()).with_header(
                "Content-Type: image/png"
                    .parse::<tiny_http::Header>()
                    .unwrap(),
            );
            let _ = req.respond(resp);
        }
    });
    let url = format!("http://127.0.0.1:{port}/img.png");
    let md = format!("![alt]({url})\n");
    let tf = fetch_remote_image_to_temp(&url).expect("should download");
    assert!(tf.path().exists(), "temp file must be created");
    let bytes = std::fs::read(tf.path()).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "must be a real PNG");
    let out = render_to_string_full(&md, false, ImageProtocol::Halfblock, false, 80);
    assert!(
        !out.contains("(remote)"),
        "must not advertise remote-skip; got: {out:?}"
    );
}

#[test]
fn remote_image_e2e_downloads_real_png() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let png = build_test_png();
    std::thread::spawn(move || {
        if let Ok(req) = server.recv() {
            let resp = tiny_http::Response::from_data(png.clone()).with_header(
                "Content-Type: image/png"
                    .parse::<tiny_http::Header>()
                    .unwrap(),
            );
            let _ = req.respond(resp);
        }
    });
    let url = format!("http://127.0.0.1:{port}/test.png");
    let tf = fetch_remote_image_to_temp(&url).expect("download must succeed");
    let dim = image::image_dimensions(tf.path()).expect("must decode as image");
    assert_eq!(dim, (2, 2), "PNG must round-trip with original dimensions");
}

#[test]
fn fetch_rejects_non_image_content_type() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    tiny_http_echo_once(server, b"<html>hi</html>".to_vec(), "text/html");
    let url = format!("http://127.0.0.1:{port}/x.png");
    let err = fetch_remote_image_to_temp(&url).unwrap_err();
    assert!(err.contains("unsupported content-type"), "got: {err}");
}

#[test]
fn fetch_rejects_oversize_with_content_length() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let huge = vec![0u8; (MAX_REMOTE_IMAGE_BYTES as usize) + 1024];
    tiny_http_echo_once(server, huge, "image/png");
    let url = format!("http://127.0.0.1:{port}/big.png");
    let err = fetch_remote_image_to_temp(&url).unwrap_err();
    assert!(err.contains("too large"), "must refuse oversize body: {err}");
}

#[test]
fn fetch_honors_max_bytes_cap() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    // A valid PNG is needed so later dimension decoding wouldn't fail; but
    // this test only checks the cap, so any payload < limit works.
    let body = vec![0x89u8; 1024]; // 1KiB, well under cap
    tiny_http_echo_once(server, body.clone(), "image/png");
    let url = format!("http://127.0.0.1:{port}/small.png");
    let tf = fetch_remote_image_to_temp(&url).unwrap();
    let bytes = std::fs::read(tf.path()).unwrap();
    assert_eq!(bytes.len(), body.len());
}

#[test]
fn fetch_times_out_on_slow_server() {
    // Server accepts then hangs — client must give up within a reasonable
    // bound (much less than 30s).
    let _bypass = AllowLoopbackGuard::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        // Accept but never respond — hold connection until the test drops us.
        let _c = listener.accept();
        std::thread::sleep(Duration::from_secs(30));
    });
    let url = format!("http://127.0.0.1:{port}/slow.png");
    let start = Instant::now();
    let res = fetch_remote_image_to_temp(&url);
    let elapsed = start.elapsed();
    assert!(res.is_err(), "slow server must yield error, got Ok");
    assert!(
        elapsed < Duration::from_secs(25),
        "must time out fast, took {elapsed:?}"
    );
}

#[test]
fn fetch_on_connection_refused_returns_error_not_panic() {
    let _bypass = AllowLoopbackGuard::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let url = format!("http://127.0.0.1:{port}/dead.png");
    let res = fetch_remote_image_to_temp(&url);
    assert!(res.is_err(), "refused connection must error, got Ok");
}

#[test]
fn fetch_redirect_policy_follows_to_image() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let png = build_test_png();
    std::thread::spawn(move || {
        if let Ok(req) = server.recv() {
            if req.url() == "/redir.png" {
                let resp = tiny_http::Response::empty(302)
                    .with_header("Location: /final.png".parse::<tiny_http::Header>().unwrap());
                let _ = req.respond(resp);
            } else {
                let _ = req.respond(tiny_http::Response::empty(404));
            }
        }
        if let Ok(req) = server.recv() {
            let hdr: tiny_http::Header = "Content-Type: image/png".parse().unwrap();
            let resp = tiny_http::Response::from_data(png.clone()).with_header(hdr);
            let _ = req.respond(resp);
        }
    });
    let url = format!("http://127.0.0.1:{port}/redir.png");
    let tf = fetch_remote_image_to_temp(&url).expect("redirect chain must succeed");
    assert_eq!(
        &std::fs::read(tf.path()).unwrap()[..8],
        b"\x89PNG\r\n\x1a\n"
    );
}

#[test]
fn fetch_empty_response_is_error() {
    let _bypass = AllowLoopbackGuard::new();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    tiny_http_echo_once(server, Vec::new(), "image/png");
    let url = format!("http://127.0.0.1:{port}/empty.png");
    let err = fetch_remote_image_to_temp(&url).unwrap_err();
    assert!(err.contains("empty"), "got: {err}");
}

// =====================================================================
// Local image traversal — end-to-end fallback
// =====================================================================

#[test]
fn local_image_path_traversal_falls_back_safely() {
    // A local path containing `..` must not panic, must not leak unexpected
    // bytes, and must reach the graceful fallback string.
    let path = Path::new("../../../../../etc/does_not_exist_and_never_will");
    let md = format!("![alt]({})\n", path.display());
    let out = render_to_string_full(&md, false, ImageProtocol::Halfblock, false, 80);
    assert!(
        out.contains("[image: alt"),
        "fallback placeholder rendered: {out:?}"
    );
    // No ANSI image bytes leaked (no kitty APC, no iTerm2 escape).
    assert!(!out.contains("\x1b_G"), "no kitty APC leaked: {out:?}");
    assert!(
        !out.contains("\x1b]1337;"),
        "no iTerm2 image escape leaked: {out:?}"
    );
}

// =====================================================================
// Image-in-table placeholder (no viuer mid-buffer)
// =====================================================================

#[test]
fn image_in_table_does_not_emit_viuer_bytes_above_table() {
    let md = "| col |\n|---|\n| ![logo](missing.png) |\n";
    let out = render_to_string_full(md, false, ImageProtocol::Halfblock, false, 40);
    let table_start = out.find('┌').expect("table top border");
    let pre = &out[..table_start];
    assert!(
        !pre.contains("[image:"),
        "image placeholder must not leak above table; got pre: {pre:?}"
    );
}

#[test]
fn image_in_table_renders_placeholder_in_cell() {
    let md = "| col |\n|---|\n| ![logo](missing.png) |\n";
    let out = render_to_string_full(md, false, ImageProtocol::Halfblock, false, 40);
    assert!(
        out.contains("[image: logo]"),
        "alt text must appear inside the table cell; got: {out:?}"
    );
}

// =====================================================================
// Sixel feature-gate: no silent kitty escapes
// =====================================================================

#[test]
fn sixel_does_not_silently_emit_kitty_escapes() {
    let md = "![nope](nonexistent.png)\n";
    let out = render_to_string_full(md, false, ImageProtocol::Sixel, false, 80);
    assert!(
        !out.contains("\x1b_G"),
        "must not emit kitty APC for sixel terminal; got: {out:?}"
    );
}

// =====================================================================
// is_tty short-circuit on RenderState::render_image
// =====================================================================

#[test]
fn image_render_falls_back_when_not_tty() {
    let term = TermConfig {
        is_tty: false,
        render_active: true, // force_color was on
        width: 80,
        cell_pixel_width: DEFAULT_CELL_PIXEL_WIDTH,
        cell_pixel_height: DEFAULT_CELL_PIXEL_HEIGHT,
        color_enabled: true,
        image_protocol: ImageProtocol::Halfblock,
        osc8_supported: false,
        allow_absolute_image_paths: false,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let writer: &mut dyn std::io::Write = &mut buf;
        let mut state = RenderState::new(writer, &term);
        state.render_image("missing.png", "alt text").unwrap();
    }
    let out = String::from_utf8(buf).unwrap();
    assert!(
        out.contains("[image: alt text]"),
        "non-TTY must fall back to text: {out:?}"
    );
}

#[test]
fn is_tty_true_does_not_short_circuit_to_text_for_existing_image() {
    let png = build_test_png();
    let mut tf = tempfile::Builder::new().suffix(".png").tempfile().unwrap();
    tf.write_all(&png).unwrap();
    tf.flush().unwrap();
    let path_str = tf.path().to_string_lossy().to_string();
    let term = TermConfig {
        is_tty: true,
        render_active: true,
        width: 80,
        cell_pixel_width: DEFAULT_CELL_PIXEL_WIDTH,
        cell_pixel_height: DEFAULT_CELL_PIXEL_HEIGHT,
        color_enabled: false,
        image_protocol: ImageProtocol::Halfblock,
        osc8_supported: false,
        allow_absolute_image_paths: false,
    };
    let mut buf: Vec<u8> = Vec::new();
    {
        let writer: &mut dyn std::io::Write = &mut buf;
        let mut state = RenderState::new(writer, &term);
        state.render_image(&path_str, "alt").unwrap();
    }
    let out = String::from_utf8(buf).unwrap();
    let bare_short_circuit = out.trim() == "[image: alt]";
    assert!(
        !bare_short_circuit,
        "is_tty=true must not take the not-tty short circuit; got: {out:?}"
    );
}
