//! Terminal detection / probe-parser tests — every env-var branch of
//! `detect_image_protocol` / `detect_osc8`, the DA2 + cell-pixel response
//! parsers, and the ABSOLUTE_MAX_WIDTH / libc_termios field-width invariants.

mod common;

use std::env;

use common::{env_lock, with_clean_image_env};
use mat::config::AppConfig;
use mat::terminal::{
    ABSOLUTE_MAX_WIDTH, DaClass, ImageProtocol, detect_image_protocol, detect_osc8,
    parse_da2_response, parse_t_response, resolve_terminal,
};

// =====================================================================
// DA2 parser
// =====================================================================

#[test]
fn da2_parser_recognizes_class_4_as_sixel_capable() {
    let resp = b"\x1b[>4;2017;0c";
    let class = parse_da2_response(resp).expect("must parse");
    assert_eq!(class, DaClass::SixelCapable);
}

#[test]
fn da2_parser_distinguishes_other_and_garbage() {
    assert_eq!(
        parse_da2_response(b"\x1b[>1;95;0c").unwrap(),
        DaClass::Other
    );
    assert_eq!(
        parse_da2_response(b"\x1b[>0;136;0c").unwrap(),
        DaClass::Other
    );
    assert!(parse_da2_response(b"hello").is_none());
    assert!(parse_da2_response(b"").is_none());
}

// =====================================================================
// Cell-pixel (CSI 14 t) parser
// =====================================================================

#[test]
fn t_response_parses_xterm_cell_pixels() {
    let resp = b"\x1b[6;36;15t";
    let (w, h) = parse_t_response(resp).expect("must parse");
    assert_eq!((w, h), (15, 36));
}

#[test]
fn t_response_rejects_garbage_and_zeroes() {
    assert!(parse_t_response(b"\x1b[6;0;0t").is_none());
    assert!(parse_t_response(b"random").is_none());
    assert!(parse_t_response(b"").is_none());
}

// =====================================================================
// detect_image_protocol env matrix
// =====================================================================

#[test]
fn image_dispatch_kitty_window_id() {
    let p = with_clean_image_env(&[("KITTY_WINDOW_ID", "1")], || detect_image_protocol(None));
    assert_eq!(p, ImageProtocol::Kitty);
}

#[test]
fn image_dispatch_wezterm_by_term_program() {
    let p = with_clean_image_env(&[("TERM_PROGRAM", "WezTerm")], || {
        detect_image_protocol(None)
    });
    assert_eq!(p, ImageProtocol::Iterm2);
}

#[test]
fn image_dispatch_vscode_by_term_program() {
    let p = with_clean_image_env(&[("TERM_PROGRAM", "vscode")], || {
        detect_image_protocol(None)
    });
    assert_eq!(p, ImageProtocol::Iterm2);
}

#[test]
fn image_dispatch_ghostty_by_resource_dir() {
    let p = with_clean_image_env(&[("GHOSTTY_RESOURCES_DIR", "/opt/ghostty")], || {
        detect_image_protocol(None)
    });
    assert_eq!(p, ImageProtocol::Kitty);
}

#[test]
fn image_dispatch_mlterm_by_term() {
    let p = with_clean_image_env(&[("TERM", "mlterm")], || detect_image_protocol(None));
    assert_eq!(p, ImageProtocol::Sixel);
}

#[test]
fn image_dispatch_xterm_upgrades_to_sixel_via_da2() {
    // No image-detection env vars; DA2 reports class 4 (sixel-capable).
    let p = with_clean_image_env(&[("TERM", "xterm-256color")], || {
        detect_image_protocol(Some(DaClass::SixelCapable))
    });
    assert_eq!(p, ImageProtocol::Sixel);
}

#[test]
fn sixel_detection_returns_sixel_for_foot() {
    let p = with_clean_image_env(&[("TERM", "foot")], || detect_image_protocol(None));
    assert_eq!(p, ImageProtocol::Sixel);
}

#[test]
fn iterm_session_id_alone_detects_iterm2() {
    let p = with_clean_image_env(
        &[(
            "ITERM_SESSION_ID",
            "w0t0p0:00000000-0000-0000-0000-000000000000",
        )],
        || detect_image_protocol(None),
    );
    assert_eq!(p, ImageProtocol::Iterm2);
}

#[test]
fn iterm_session_id_unset_falls_back_to_halfblock() {
    let p = with_clean_image_env(&[], || detect_image_protocol(None));
    assert_eq!(p, ImageProtocol::Halfblock);
}

// =====================================================================
// detect_osc8 matrix
// =====================================================================

#[test]
fn vte_version_5000_enables_osc8() {
    let _env_guard = env_lock();
    let prev = env::var_os("VTE_VERSION");
    let prev_term = env::var_os("TERM");
    let prev_term_program = env::var_os("TERM_PROGRAM");
    let prev_kitty = env::var_os("KITTY_WINDOW_ID");
    let prev_ghostty = env::var_os("GHOSTTY_RESOURCES_DIR");
    let prev_no_osc8 = env::var_os("NO_OSC8");
    unsafe {
        env::remove_var("TERM");
        env::remove_var("TERM_PROGRAM");
        env::remove_var("KITTY_WINDOW_ID");
        env::remove_var("GHOSTTY_RESOURCES_DIR");
        env::remove_var("NO_OSC8");
        env::set_var("VTE_VERSION", "5800");
    }
    let supported = detect_osc8();
    unsafe {
        match prev {
            Some(v) => env::set_var("VTE_VERSION", v),
            None => env::remove_var("VTE_VERSION"),
        }
        if let Some(v) = prev_term {
            env::set_var("TERM", v);
        }
        if let Some(v) = prev_term_program {
            env::set_var("TERM_PROGRAM", v);
        }
        if let Some(v) = prev_kitty {
            env::set_var("KITTY_WINDOW_ID", v);
        }
        if let Some(v) = prev_ghostty {
            env::set_var("GHOSTTY_RESOURCES_DIR", v);
        }
        if let Some(v) = prev_no_osc8 {
            env::set_var("NO_OSC8", v);
        }
    }
    assert!(supported, "VTE 5800 must enable OSC 8");
}

#[test]
fn vte_version_below_5000_does_not_enable_osc8() {
    let _env_guard = env_lock();
    let prev = env::var_os("VTE_VERSION");
    let prev_term = env::var_os("TERM");
    let prev_term_program = env::var_os("TERM_PROGRAM");
    let prev_kitty = env::var_os("KITTY_WINDOW_ID");
    let prev_ghostty = env::var_os("GHOSTTY_RESOURCES_DIR");
    let prev_no_osc8 = env::var_os("NO_OSC8");
    unsafe {
        env::remove_var("TERM");
        env::remove_var("TERM_PROGRAM");
        env::remove_var("KITTY_WINDOW_ID");
        env::remove_var("GHOSTTY_RESOURCES_DIR");
        env::remove_var("NO_OSC8");
        env::set_var("VTE_VERSION", "4900");
    }
    let supported = detect_osc8();
    unsafe {
        match prev {
            Some(v) => env::set_var("VTE_VERSION", v),
            None => env::remove_var("VTE_VERSION"),
        }
        if let Some(v) = prev_term {
            env::set_var("TERM", v);
        }
        if let Some(v) = prev_term_program {
            env::set_var("TERM_PROGRAM", v);
        }
        if let Some(v) = prev_kitty {
            env::set_var("KITTY_WINDOW_ID", v);
        }
        if let Some(v) = prev_ghostty {
            env::set_var("GHOSTTY_RESOURCES_DIR", v);
        }
        if let Some(v) = prev_no_osc8 {
            env::set_var("NO_OSC8", v);
        }
    }
    assert!(!supported, "VTE 4900 must not enable OSC 8");
}

/// Shared env-scrub harness for Apple_Terminal OSC 8 detection. Saves every
/// var `detect_osc8` reads, blanks them, runs `f`, then restores — so these
/// tests don't flake against the real TERM_PROGRAM / TERM set by the shell
/// that invoked `cargo test`.
fn with_scrubbed_osc8_env(program: Option<&str>, version: Option<&str>, f: impl FnOnce() -> bool) -> bool {
    let _env_guard = env_lock();
    let prev_term = env::var_os("TERM");
    let prev_term_program = env::var_os("TERM_PROGRAM");
    let prev_term_program_version = env::var_os("TERM_PROGRAM_VERSION");
    let prev_kitty = env::var_os("KITTY_WINDOW_ID");
    let prev_ghostty = env::var_os("GHOSTTY_RESOURCES_DIR");
    let prev_vte = env::var_os("VTE_VERSION");
    let prev_no_osc8 = env::var_os("NO_OSC8");
    unsafe {
        env::remove_var("TERM");
        env::remove_var("TERM_PROGRAM");
        env::remove_var("TERM_PROGRAM_VERSION");
        env::remove_var("KITTY_WINDOW_ID");
        env::remove_var("GHOSTTY_RESOURCES_DIR");
        env::remove_var("VTE_VERSION");
        env::remove_var("NO_OSC8");
        if let Some(p) = program {
            env::set_var("TERM_PROGRAM", p);
        }
        if let Some(v) = version {
            env::set_var("TERM_PROGRAM_VERSION", v);
        }
    }
    let result = f();
    unsafe {
        env::remove_var("TERM_PROGRAM");
        env::remove_var("TERM_PROGRAM_VERSION");
        if let Some(v) = prev_term { env::set_var("TERM", v); }
        if let Some(v) = prev_term_program { env::set_var("TERM_PROGRAM", v); }
        if let Some(v) = prev_term_program_version { env::set_var("TERM_PROGRAM_VERSION", v); }
        if let Some(v) = prev_kitty { env::set_var("KITTY_WINDOW_ID", v); }
        if let Some(v) = prev_ghostty { env::set_var("GHOSTTY_RESOURCES_DIR", v); }
        if let Some(v) = prev_vte { env::set_var("VTE_VERSION", v); }
        if let Some(v) = prev_no_osc8 { env::set_var("NO_OSC8", v); }
    }
    result
}

#[test]
fn apple_terminal_v440_or_higher_enables_osc8() {
    // macOS 14 Sonoma shipped Terminal.app 2.14 (TERM_PROGRAM_VERSION=440)
    // with OSC 8 hyperlink support. Every version at or above that must
    // light up the clickable-link path so links in a rendered markdown file
    // are actually clickable on a stock macOS setup.
    for v in ["440", "445", "470", "500", "999"] {
        assert!(
            with_scrubbed_osc8_env(Some("Apple_Terminal"), Some(v), detect_osc8),
            "Apple_Terminal version {v} must enable OSC 8"
        );
    }
}

#[test]
fn apple_terminal_below_v440_does_not_enable_osc8() {
    // Older Terminal.app builds ignored OSC 8 entirely. Emitting the escape
    // there would not render the link as clickable, so keep it off.
    for v in ["399", "420", "439"] {
        assert!(
            !with_scrubbed_osc8_env(Some("Apple_Terminal"), Some(v), detect_osc8),
            "Apple_Terminal version {v} must not enable OSC 8"
        );
    }
    // Missing version var defaults to 0 — also must not enable.
    assert!(
        !with_scrubbed_osc8_env(Some("Apple_Terminal"), None, detect_osc8),
        "Apple_Terminal without TERM_PROGRAM_VERSION must not enable OSC 8"
    );
}

// =====================================================================
// resolve_terminal wiring — no_images short-circuits detection
// =====================================================================

#[test]
fn detect_image_protocol_no_images_flag_forces_none() {
    // The flag lives on AppConfig → TermConfig. resolve_terminal skips
    // detect_image_protocol entirely when cfg.no_images is true, so we
    // verify through the config boundary instead.
    let cfg = AppConfig {
        sources: Vec::new(),
        no_color: false,
        force_color: true, // render_active so non-tty still sets fields
        width_override: Some(80),
        no_images: true,
        allow_absolute_image_paths: false,
    };
    let term = resolve_terminal(&cfg);
    assert_eq!(term.image_protocol, ImageProtocol::None);
}

// =====================================================================
// Width clamp invariants
// =====================================================================

#[test]
fn absolute_max_width_is_at_least_1024() {
    // Compile-time check — the `const _` binding forces the inequality to
    // be evaluated during compilation. A runtime assert! on the same
    // expression is redundant (and clippy::assertions_on_constants flags
    // it), so the const binding is the whole test body.
    const _: () = assert!(ABSOLUTE_MAX_WIDTH >= 1024);
    // Observe the constant at runtime so the test is not fully empty and
    // so that accidental removal of the symbol also fails this test, not
    // just the above compile-time check.
    let n: usize = ABSOLUTE_MAX_WIDTH;
    assert_ne!(n, 0);
}

// =====================================================================
// tcflag_t / libc_termios struct invariants
// =====================================================================

#[cfg(all(unix, target_os = "macos"))]
#[test]
fn tcflag_t_is_64bit_on_macos() {
    assert_eq!(std::mem::size_of::<mat::terminal::tcflag_t>(), 8);
}

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
#[test]
fn tcflag_t_is_32bit_on_linux() {
    assert_eq!(std::mem::size_of::<mat::terminal::tcflag_t>(), 4);
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
#[test]
fn libc_termios_field_widths_match_tcflag_t() {
    let t: mat::terminal::libc_termios = unsafe { std::mem::zeroed() };
    let iflag_width = std::mem::size_of_val(&t.c_iflag);
    let lflag_width = std::mem::size_of_val(&t.c_lflag);
    let tcflag_width = std::mem::size_of::<mat::terminal::tcflag_t>();
    assert_eq!(iflag_width, tcflag_width);
    assert_eq!(lflag_width, tcflag_width);
}
