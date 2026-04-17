//! Process-layer tests — the cat-compatibility passthrough pump (byte-exact
//! round-tripping for stdin, files, and large streams) and the
//! `should_render` predicate that gates render vs. passthrough.

use std::fs::File;
use std::io::{Cursor, Write};

use mat::process::{passthrough_bytes, should_render};

#[test]
fn passthrough_stdin_byte_exact() {
    // Any arbitrary byte sequence fed to passthrough_bytes must round-trip
    // exactly — ESC, NUL, CR, invalid UTF-8 included.
    let input: Vec<u8> = (0u8..=255u8).collect();
    let mut src = Cursor::new(input.clone());
    let mut dst: Vec<u8> = Vec::new();
    let copied = passthrough_bytes(&mut src, &mut dst).unwrap();
    assert_eq!(copied as usize, input.len());
    assert_eq!(dst, input, "passthrough must be byte-exact");
}

#[test]
fn passthrough_file_byte_exact() {
    // Temp file containing every byte value + a PNG magic header —
    // passthrough must not interpret the bytes.
    let mut input = Vec::new();
    input.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    input.extend_from_slice(b"## not rendered as heading\n");
    input.extend(0u8..=255u8);
    let mut tf = tempfile::NamedTempFile::new().unwrap();
    tf.write_all(&input).unwrap();
    tf.flush().unwrap();
    let mut f = File::open(tf.path()).unwrap();
    let mut dst: Vec<u8> = Vec::new();
    let copied = passthrough_bytes(&mut f, &mut dst).unwrap();
    assert_eq!(copied as usize, input.len());
    assert_eq!(dst, input);
}

#[test]
fn passthrough_large_file_streams() {
    // 4 MiB of pseudo-random bytes — proves the copy is streaming and
    // doesn't OOM by round-tripping through memory.
    let mut input = vec![0u8; 4 * 1024 * 1024];
    for (i, b) in input.iter_mut().enumerate() {
        *b = ((i.wrapping_mul(1103515245) >> 16) & 0xFF) as u8;
    }
    let mut src = Cursor::new(&input);
    let mut dst = Vec::with_capacity(input.len());
    let copied = passthrough_bytes(&mut src, &mut dst).unwrap();
    assert_eq!(copied as usize, input.len());
    assert_eq!(dst.len(), input.len());
    assert_eq!(&dst[..128], &input[..128]);
    assert_eq!(&dst[dst.len() - 128..], &input[input.len() - 128..]);
}

#[test]
fn render_mode_triggered_when_force_color_set() {
    assert!(should_render(true, false), "real TTY renders");
    assert!(should_render(false, true), "force_color forces render");
    assert!(
        !should_render(false, false),
        "neither TTY nor force → passthrough"
    );
    assert!(should_render(true, true), "both true still renders");
}
