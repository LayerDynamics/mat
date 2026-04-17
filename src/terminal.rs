//! Terminal detection and probing.
//!
//! Owns the width/color/image-protocol resolution plus the DA2 and
//! `\x1b[14t` (cell-pixel) probes and the minimal termios FFI shim used
//! to put `/dev/tty` into raw mode for those probes.

use std::env;
use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Read, Write};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use crate::config::AppConfig;
use crate::process::should_render;

pub const DEFAULT_WIDTH: usize = 80;
/// Hard upper bound — protects against absurd $COLUMNS values, not user terminals.
pub const ABSOLUTE_MAX_WIDTH: usize = 1024;
/// Default cell pixel dimensions used when the terminal does not respond to
/// `\x1b[14t`. Roughly matches a 14pt monospace font on a typical retina cell.
/// The 2:1 height:width ratio mirrors the aspect of nearly every monospace
/// glyph cell; used to size images when the real cell pixel size is unknown.
pub const DEFAULT_CELL_PIXEL_WIDTH: u16 = 9;
pub const DEFAULT_CELL_PIXEL_HEIGHT: u16 = 18;
/// Wall-clock budget for a single terminal control-sequence probe (DA2,
/// pixel-cell query, etc). Long enough that a real terminal reliably
/// responds; short enough that a non-responding tty does not noticeably
/// delay startup. Empirically 120 ms covers Kitty/iTerm2/foot/xterm on
/// loopback; remote SSH sessions see the same budget per probe.
pub const TTY_PROBE_TIMEOUT: Duration = Duration::from_millis(120);

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ImageProtocol {
    Kitty,
    Iterm2,
    Sixel,
    Halfblock,
    None,
}

pub struct TermConfig {
    /// Whether stdout is attached to a real TTY (raw `is_terminal()` value).
    /// Distinct from `render_active`: --force-color flips render_active without
    /// flipping is_tty.
    pub is_tty: bool,
    pub render_active: bool,
    pub width: usize,
    /// Cell pixel dimensions, queried via `\x1b[14t` when possible.
    /// Used by image scaling to compute aspect-correct row counts.
    pub cell_pixel_width: u16,
    pub cell_pixel_height: u16,
    pub color_enabled: bool,
    pub image_protocol: ImageProtocol,
    pub osc8_supported: bool,
    /// Opt-in — permit local image paths outside the source directory.
    /// Propagated from `AppConfig::allow_absolute_image_paths`.
    pub allow_absolute_image_paths: bool,
}

pub fn resolve_terminal(cfg: &AppConfig) -> TermConfig {
    let is_tty = io::stdout().is_terminal();
    let render_active = should_render(is_tty, cfg.force_color);

    let width = cfg
        .width_override
        .or_else(|| env::var("COLUMNS").ok().and_then(|s| s.parse().ok()))
        .or_else(|| terminal_size::terminal_size().map(|(w, _)| w.0 as usize))
        .unwrap_or(DEFAULT_WIDTH)
        .clamp(10, ABSOLUTE_MAX_WIDTH);

    let color_enabled = render_active && !cfg.no_color;

    // Probe DA2 once — its outcome may upgrade Sixel detection on terminals
    // that don't advertise themselves through env vars (xterm with sixel
    // compiled in, for example).
    let da2 = if is_tty { probe_da2() } else { None };

    let image_protocol = if render_active && !cfg.no_images {
        detect_image_protocol(da2)
    } else {
        ImageProtocol::None
    };

    let osc8_supported = render_active && detect_osc8();

    let (cell_pixel_width, cell_pixel_height) = if is_tty {
        probe_cell_pixels().unwrap_or((DEFAULT_CELL_PIXEL_WIDTH, DEFAULT_CELL_PIXEL_HEIGHT))
    } else {
        (DEFAULT_CELL_PIXEL_WIDTH, DEFAULT_CELL_PIXEL_HEIGHT)
    };

    TermConfig {
        is_tty,
        render_active,
        width,
        cell_pixel_width,
        cell_pixel_height,
        color_enabled,
        image_protocol,
        osc8_supported,
        allow_absolute_image_paths: cfg.allow_absolute_image_paths,
    }
}

pub fn detect_image_protocol(da2: Option<DaClass>) -> ImageProtocol {
    if env::var_os("KITTY_WINDOW_ID").is_some() {
        return ImageProtocol::Kitty;
    }
    if env::var_os("GHOSTTY_RESOURCES_DIR").is_some() {
        return ImageProtocol::Kitty;
    }
    if env::var_os("ITERM_SESSION_ID").is_some() {
        return ImageProtocol::Iterm2;
    }
    if let Ok(tp) = env::var("TERM_PROGRAM") {
        match tp.as_str() {
            "iTerm.app" | "WezTerm" | "vscode" | "ghostty" => return ImageProtocol::Iterm2,
            _ => {}
        }
    }
    if let Ok(t) = env::var("TERM") {
        if t.contains("kitty") {
            return ImageProtocol::Kitty;
        }
        if t.contains("foot") || t.contains("mlterm") {
            return ImageProtocol::Sixel;
        }
    }
    // DA2 fallback: terminal classes 4 (VT132/VT240/VT330/VT340/xterm-with-sixel)
    // advertise sixel support.
    if matches!(da2, Some(DaClass::SixelCapable)) {
        return ImageProtocol::Sixel;
    }
    ImageProtocol::Halfblock
}

pub fn detect_osc8() -> bool {
    if env::var_os("NO_OSC8").is_some() {
        return false;
    }
    if env::var_os("KITTY_WINDOW_ID").is_some() {
        return true;
    }
    if env::var_os("GHOSTTY_RESOURCES_DIR").is_some() {
        return true;
    }
    // VTE 0.50+ supports OSC 8. $VTE_VERSION is encoded as e.g. 5800 for 0.58.0.
    if let Ok(v) = env::var("VTE_VERSION")
        && v.parse::<u32>().unwrap_or(0) >= 5000
    {
        return true;
    }
    if let Ok(tp) = env::var("TERM_PROGRAM") {
        if matches!(
            tp.as_str(),
            "iTerm.app" | "WezTerm" | "vscode" | "Hyper" | "ghostty"
        ) {
            return true;
        }
        // macOS Terminal.app 2.14+ (shipped with macOS 14 Sonoma) implements
        // the OSC 8 hyperlink escape; older builds silently strip unknown
        // OSCs, so emitting the sequence there would just yield non-clickable
        // text without visual garbage — but gating on the version keeps the
        // detection honest. `$TERM_PROGRAM_VERSION` is an integer build
        // number (e.g. "440" for 2.14, "470" for 2.15).
        if tp == "Apple_Terminal"
            && let Ok(v) = env::var("TERM_PROGRAM_VERSION")
            && v.parse::<u32>().unwrap_or(0) >= 440
        {
            return true;
        }
    }
    if let Ok(t) = env::var("TERM")
        && (t.contains("kitty")
            || t.contains("alacritty")
            || t.contains("foot")
            || t.contains("wezterm"))
    {
        return true;
    }
    false
}

// =====================================================================
// terminal probes — DA2 and pixel-cell query
// =====================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DaClass {
    SixelCapable,
    Other,
}

/// Send the DA2 query (`CSI > 0 c`) on the controlling tty and parse the
/// response. Returns `None` if the terminal doesn't reply within
/// `TTY_PROBE_TIMEOUT` or the response can't be parsed.
pub fn probe_da2() -> Option<DaClass> {
    let bytes = tty_query(b"\x1b[>0c", TTY_PROBE_TIMEOUT)?;
    parse_da2_response(&bytes)
}

/// Send the xterm "report cell pixel size" query (`CSI 14 t`) and parse the
/// `CSI 6 ; H ; W t` response.
pub fn probe_cell_pixels() -> Option<(u16, u16)> {
    let bytes = tty_query(b"\x1b[14t", TTY_PROBE_TIMEOUT)?;
    parse_t_response(&bytes)
}

/// Open `/dev/tty`, switch to raw, write `query`, read until either a response
/// terminator (`c`, `t`, `~`, `R`) is seen or `timeout` elapses, then restore
/// the terminal state. Returns the captured bytes (without the leading ESC).
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
pub fn tty_query(query: &[u8], timeout: Duration) -> Option<Vec<u8>> {
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();

    let original = unsafe {
        let mut t: libc_termios = std::mem::zeroed();
        if tcgetattr(fd, &mut t) != 0 {
            return None;
        }
        t
    };

    let mut raw = original;
    unsafe {
        // Disable canonical mode, echo, and signal generation; minimal raw read.
        raw.c_lflag &= !(ICANON | ECHO | ISIG);
        raw.c_iflag &= !(ICRNL | INPCK | ISTRIP | IXON);
        raw.c_cc[VMIN] = 0;
        raw.c_cc[VTIME] = 1; // 100ms per read
        if tcsetattr(fd, TCSANOW, &raw) != 0 {
            return None;
        }
    }

    let mut writer = &tty;
    let _ = writer.write_all(query);
    let _ = writer.flush();

    let mut buf = Vec::with_capacity(64);
    let deadline = Instant::now() + timeout;
    let mut chunk = [0u8; 64];
    let mut reader = &tty;
    while Instant::now() < deadline {
        match reader.read(&mut chunk) {
            Ok(0) => continue,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Some(b) = buf.last()
                    && matches!(*b, b'c' | b't' | b'~' | b'R')
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    unsafe {
        let _ = tcsetattr(fd, TCSANOW, &original);
    }

    if buf.is_empty() { None } else { Some(buf) }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
pub fn tty_query(_query: &[u8], _timeout: Duration) -> Option<Vec<u8>> {
    // BSDs + other unixes: struct termios layout differs, so probing
    // would be UB. terminal_size still supplies the column count and
    // image scaling uses DEFAULT_CELL_PIXEL_{WIDTH,HEIGHT}.
    None
}

// Minimal libc/termios shim — we only need the few constants and the
// two functions, scoped to this module so we don't take a libc
// dependency. `tcflag_t` / `speed_t` width is platform-specific:
// macOS declares them as `unsigned long` (64-bit on LP64); glibc,
// musl, and Linux declare them as `unsigned int` (32-bit on every
// architecture). Using the wrong width corrupts the termios struct
// passed to tcgetattr / tcsetattr — on Linux that is undefined
// behavior and can leave the tty in raw mode after the probe returns.
#[cfg(all(unix, target_os = "macos"))]
#[allow(non_camel_case_types)]
pub type tcflag_t = u64;
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(non_camel_case_types)]
pub type tcflag_t = u32;
#[cfg(unix)]
#[allow(non_camel_case_types)]
pub type cc_t = u8;
#[cfg(all(unix, target_os = "macos"))]
#[allow(non_camel_case_types)]
pub type speed_t = u64;
#[cfg(all(unix, not(target_os = "macos")))]
#[allow(non_camel_case_types)]
pub type speed_t = u32;

#[cfg(all(unix, target_os = "macos"))]
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub struct libc_termios {
    pub c_iflag: tcflag_t,
    pub c_oflag: tcflag_t,
    pub c_cflag: tcflag_t,
    pub c_lflag: tcflag_t,
    pub c_cc: [cc_t; 20],
    pub c_ispeed: speed_t,
    pub c_ospeed: speed_t,
}

// Linux + Android `struct termios` layout: c_iflag..c_lflag are
// `unsigned int` (tcflag_t = u32), there is a c_line field, NCCS=32,
// and the speed fields are also `unsigned int`. BSD-family systems
// (FreeBSD / NetBSD / OpenBSD) have a DIFFERENT layout (no c_line,
// NCCS=20, different widths) — the extern block is also scoped to
// macos/linux/android so BSDs aren't linked against a struct that
// disagrees with the kernel.
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub struct libc_termios {
    pub c_iflag: tcflag_t,
    pub c_oflag: tcflag_t,
    pub c_cflag: tcflag_t,
    pub c_lflag: tcflag_t,
    pub c_line: cc_t,
    pub c_cc: [cc_t; 32],
    pub c_ispeed: speed_t,
    pub c_ospeed: speed_t,
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
#[link(name = "c")]
unsafe extern "C" {
    fn tcgetattr(fd: i32, termios_p: *mut libc_termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const libc_termios) -> i32;
}

#[cfg(all(unix, target_os = "macos"))]
pub const ICANON: tcflag_t = 0x00000100;
#[cfg(all(unix, target_os = "macos"))]
pub const ECHO: tcflag_t = 0x00000008;
#[cfg(all(unix, target_os = "macos"))]
pub const ISIG: tcflag_t = 0x00000080;
#[cfg(all(unix, target_os = "macos"))]
pub const ICRNL: tcflag_t = 0x00000100;
#[cfg(all(unix, target_os = "macos"))]
pub const INPCK: tcflag_t = 0x00000010;
#[cfg(all(unix, target_os = "macos"))]
pub const ISTRIP: tcflag_t = 0x00000020;
#[cfg(all(unix, target_os = "macos"))]
pub const IXON: tcflag_t = 0x00000200;
#[cfg(all(unix, target_os = "macos"))]
pub const VMIN: usize = 16;
#[cfg(all(unix, target_os = "macos"))]
pub const VTIME: usize = 17;
#[cfg(all(unix, target_os = "macos"))]
pub const TCSANOW: i32 = 0;

#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const ICANON: tcflag_t = 0o0000002;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const ECHO: tcflag_t = 0o0000010;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const ISIG: tcflag_t = 0o0000001;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const ICRNL: tcflag_t = 0o0000400;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const INPCK: tcflag_t = 0o0000020;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const ISTRIP: tcflag_t = 0o0000040;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const IXON: tcflag_t = 0o0002000;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const VMIN: usize = 6;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const VTIME: usize = 5;
#[cfg(all(unix, any(target_os = "linux", target_os = "android")))]
pub const TCSANOW: i32 = 0;

/// Parse a DA2 response of the form `ESC [ > Pp ; Pv ; Pc c`.
/// Pp is the terminal class. Class 4 is VT132/VT240/VT330/VT340 / xterm with
/// sixel compiled in — these advertise sixel capability.
pub fn parse_da2_response(bytes: &[u8]) -> Option<DaClass> {
    let s = std::str::from_utf8(bytes).ok()?;
    let s = s.trim_end_matches('\0');
    let idx = s.find("\x1b[>")?;
    let tail = &s[idx + 3..];
    let end = tail.find('c')?;
    let body = &tail[..end];
    let pp = body.split(';').next()?;
    let pp_n: u32 = pp.parse().ok()?;
    Some(if pp_n == 4 {
        DaClass::SixelCapable
    } else {
        DaClass::Other
    })
}

/// Parse an xterm CSI 14 t response of the form `ESC [ 6 ; H ; W t`.
/// Returns (cell_width_px, cell_height_px).
pub fn parse_t_response(bytes: &[u8]) -> Option<(u16, u16)> {
    let s = std::str::from_utf8(bytes).ok()?;
    let s = s.trim_end_matches('\0');
    let idx = s.find("\x1b[6;")?;
    let tail = &s[idx + 4..];
    let end = tail.find('t')?;
    let body = &tail[..end];
    let mut parts = body.split(';');
    let h: u16 = parts.next()?.parse().ok()?;
    let w: u16 = parts.next()?.parse().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    Some((w, h))
}
