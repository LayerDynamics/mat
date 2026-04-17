//! Config + project-metadata tests — shell / PowerShell installers, Cargo
//! publish metadata, README sections, and the constant-declaration audit
//! for the long-defunct `MAX_USEFUL_WIDTH`.

use std::path::Path;

use mat::config::{ExitAction, parse_args};

#[test]
fn parse_args_returns_help_for_dash_h() {
    let args = vec!["-h".to_string()];
    let got = parse_args(&args);
    assert!(
        matches!(got, Err(ExitAction::PrintUsage)),
        "expected PrintUsage; got Err? {}",
        got.is_err()
    );
}

#[test]
fn parse_args_returns_version_for_dash_capital_v() {
    let args = vec!["-V".to_string()];
    assert!(matches!(parse_args(&args), Err(ExitAction::PrintVersion)));
}

#[test]
fn parse_args_honors_no_color_short() {
    let args = vec!["-n".to_string(), "file.md".to_string()];
    let cfg = parse_args(&args).expect("must parse");
    assert!(cfg.no_color);
    assert_eq!(cfg.sources.len(), 1);
}

#[test]
fn parse_args_honors_width_flag() {
    let args = vec!["--width".to_string(), "100".to_string(), "f.md".to_string()];
    let cfg = parse_args(&args).expect("must parse");
    assert_eq!(cfg.width_override, Some(100));
}

#[test]
fn parse_args_honors_width_equals_flag() {
    let args = vec!["--width=120".to_string(), "f.md".to_string()];
    let cfg = parse_args(&args).expect("must parse");
    assert_eq!(cfg.width_override, Some(120));
}

#[test]
fn parse_args_rejects_sub_10_width() {
    let args = vec!["--width".to_string(), "5".to_string()];
    match parse_args(&args) {
        Err(ExitAction::Usage(msg)) => assert!(msg.contains(">= 10")),
        Err(other) => panic!("expected Usage error; got: {other:?}"),
        Ok(_) => panic!("expected Usage error; got Ok"),
    }
}

#[test]
fn parse_args_treats_dash_as_stdin_source() {
    let args = vec!["-".to_string()];
    let cfg = parse_args(&args).expect("must parse");
    assert_eq!(cfg.sources.len(), 1);
    assert!(matches!(cfg.sources[0], mat::config::Source::Stdin));
}

#[test]
fn parse_args_rejects_unknown_long_option() {
    let args = vec!["--not-a-real-flag".to_string()];
    assert!(matches!(parse_args(&args), Err(ExitAction::Usage(_))));
}

#[test]
fn parse_args_double_dash_switches_to_positional_only() {
    let args = vec!["--".to_string(), "--not-a-flag.md".to_string()];
    let cfg = parse_args(&args).expect("must parse");
    assert_eq!(cfg.sources.len(), 1);
    assert!(matches!(cfg.sources[0], mat::config::Source::File(_)));
}

// =====================================================================
// Project-meta tests — installer scripts, Cargo.toml, README.md
// =====================================================================

#[test]
fn release_workflow_exists_with_matrix() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("release workflow missing at {}", path.display()));
    assert!(content.contains("x86_64-unknown-linux-gnu"));
    assert!(content.contains("aarch64-apple-darwin"));
    assert!(content.contains("x86_64-pc-windows-msvc"));
}

#[test]
fn install_sh_prefers_prebuilt() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    let content = std::fs::read_to_string(&path).expect("install.sh exists");
    assert!(
        content.contains("releases/download"),
        "install.sh must reach for prebuilt binaries: {content:?}"
    );
    assert!(
        content.contains("cargo build --release") || content.contains("cargo install"),
        "install.sh must still keep a source-build fallback"
    );
}

#[test]
fn install_ps1_exists() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.ps1");
    assert!(
        path.exists(),
        "install.ps1 must exist at {}",
        path.display()
    );
}

#[test]
fn install_ps1_downloads_and_updates_path() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("install.ps1");
    let content = std::fs::read_to_string(&path).expect("install.ps1");
    assert!(
        content.contains("Invoke-WebRequest") || content.contains("Start-BitsTransfer"),
        "must download a binary: {content:?}"
    );
    assert!(
        content.to_lowercase().contains("path"),
        "must touch PATH: {content:?}"
    );
}

#[test]
fn cargo_toml_has_publish_metadata() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = std::fs::read_to_string(&path).expect("Cargo.toml");
    for needle in [
        "repository",
        "homepage",
        "readme",
        "keywords",
        "categories",
        "documentation",
    ] {
        assert!(
            content.contains(needle),
            "Cargo.toml must declare {needle}; got: {content}"
        );
    }
}

#[test]
fn cargo_toml_keywords_well_formed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let content = std::fs::read_to_string(&path).expect("Cargo.toml");
    assert!(content.contains("\"markdown\""));
    assert!(content.contains("\"terminal\""));
    assert!(content.contains("\"cli\""));
}

#[test]
fn readme_exists() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    assert!(path.exists(), "README.md missing");
}

#[test]
fn readme_has_required_sections() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md");
    let content = std::fs::read_to_string(&path).expect("README.md");
    for h in ["# mat", "## Install", "## Usage", "## Features"] {
        assert!(content.contains(h), "README.md missing section {h}");
    }
}

#[test]
fn max_useful_width_constant_is_removed() {
    // Regression: the old width-resolution chain clamped to a 120-column
    // constant via a tautological max-min combination (max of 120 and 80 is
    // always 120). The constant declaration must no longer appear in ANY
    // src/*.rs file. Scan the whole src tree so post-refactor placement
    // doesn't let the const sneak back in.
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let bad_name = ["MAX", "USEFUL", "WIDTH"].join("_");
    let mut declared = false;
    for entry in std::fs::read_dir(&src_root).expect("src dir") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("read src file");
        if content.lines().any(|l| {
            let t = l.trim_start();
            t.starts_with("const ") && t.contains(&bad_name) && t.contains(':')
        }) {
            declared = true;
            break;
        }
    }
    assert!(
        !declared,
        "{bad_name} must not be declared as a const anywhere in src/"
    );
}
