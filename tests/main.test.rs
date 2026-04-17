//! End-to-end CLI tests — spawn the built binary and exercise the full
//! argv → pipeline path (help, version, passthrough on non-TTY, exit codes).

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn mat_binary() -> PathBuf {
    // `env!("CARGO_BIN_EXE_mat")` — Cargo populates this for every
    // `[[bin]]` target at build time when running integration tests. Gives
    // the exact path to the freshly-built binary, independent of whether
    // the user invoked `cargo test` from a workspace parent, `--target`
    // directory, release profile, etc.
    PathBuf::from(env!("CARGO_BIN_EXE_mat"))
}

#[test]
fn dash_h_prints_usage_exit_zero() {
    let out = Command::new(mat_binary())
        .arg("-h")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("USAGE:"), "usage header present: {stdout}");
    assert!(stdout.contains("mat"), "binary name present");
}

#[test]
fn dash_big_v_prints_version_exit_zero() {
    let out = Command::new(mat_binary())
        .arg("-V")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("mat "), "version line: {stdout}");
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "version string present: {stdout}"
    );
}

#[test]
fn unknown_long_option_exits_with_code_2() {
    let out = Command::new(mat_binary())
        .arg("--not-a-real-flag")
        .output()
        .expect("spawn");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown option"), "err: {stderr}");
}

#[test]
fn passthrough_from_stdin_byte_exact_when_not_tty() {
    // When stdin is piped (non-TTY) and no `--force-color`, mat behaves
    // exactly like cat. Give it arbitrary bytes; get them back verbatim.
    let input = b"# heading\n\nparagraph with \xf0\x9f\x98\x80 and a NUL\x00 byte.\n";
    let mut child = Command::new(mat_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input)
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "exit status {:?}", out.status);
    assert_eq!(
        out.stdout, input,
        "non-TTY must passthrough byte-for-byte; stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn force_color_renders_even_on_pipe() {
    // `--force-color` overrides the non-TTY detection. The output should
    // contain ANSI escape bytes (at minimum \x1b[ from a style or a reset).
    let input = b"# title\n\n**bold** body\n";
    let mut child = Command::new(mat_binary())
        .arg("--force-color")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input)
        .unwrap();
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success(), "exit status {:?}", out.status);
    assert!(
        out.stdout.windows(2).any(|w| w == b"\x1b["),
        "force-color output must contain ANSI escapes; got {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Bold + title text still present.
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("title"));
    assert!(text.contains("bold"));
}

#[test]
fn missing_file_arg_writes_error_to_stderr_and_fails() {
    let out = Command::new(mat_binary())
        .arg("/no/such/file/definitely/not-there.md")
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "should fail on missing file");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("mat:"), "mat prefix on error: {stderr}");
}

#[test]
fn file_source_round_trips_when_stdout_not_tty() {
    // Write a known markdown file, then spawn the binary with stdout piped
    // (non-TTY). Passthrough contract: output bytes equal file bytes.
    let mut tf = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
    let payload = b"# hi\n\ntext\n\n```\ncode\n```\n";
    tf.write_all(payload).unwrap();
    tf.flush().unwrap();
    let out = Command::new(mat_binary())
        .arg(tf.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    assert!(out.status.success());
    assert_eq!(out.stdout, payload);
}
