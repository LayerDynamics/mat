//! CLI configuration — argument parsing, help/version text, and the
//! `AppConfig` + `Source` types passed from argv into the pipeline.

use std::borrow::Cow;
use std::env;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

#[derive(Debug)]
pub enum ExitAction {
    PrintUsage,
    PrintVersion,
    Usage(String),
}

pub struct AppConfig {
    pub sources: Vec<Source>,
    pub no_color: bool,
    pub force_color: bool,
    pub width_override: Option<usize>,
    pub no_images: bool,
    /// Opt-in: render local image paths that fall outside the source
    /// document's directory (absolute paths, or relatives escaping via
    /// `..`). Off by default — prevents information exfiltration through
    /// Kitty/iTerm2 APC image frames.
    pub allow_absolute_image_paths: bool,
}

#[derive(Clone)]
pub enum Source {
    Stdin,
    File(PathBuf),
}

impl Source {
    pub fn display(&self) -> Cow<'_, str> {
        match self {
            Source::Stdin => Cow::Borrowed("<stdin>"),
            Source::File(p) => p.to_string_lossy(),
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<AppConfig, ExitAction> {
    let mut cfg = AppConfig {
        sources: Vec::new(),
        no_color: env::var_os("NO_COLOR").is_some(),
        force_color: env::var_os("FORCE_COLOR").is_some(),
        width_override: None,
        no_images: false,
        allow_absolute_image_paths: false,
    };

    let mut i = 0;
    let mut only_positional = false;
    while i < args.len() {
        let a = &args[i];
        if !only_positional {
            match a.as_str() {
                "--" => {
                    only_positional = true;
                    i += 1;
                    continue;
                }
                "-h" | "--help" => return Err(ExitAction::PrintUsage),
                "-V" | "--version" => return Err(ExitAction::PrintVersion),
                "-n" | "--no-color" => {
                    cfg.no_color = true;
                    i += 1;
                    continue;
                }
                "--force-color" => {
                    cfg.force_color = true;
                    i += 1;
                    continue;
                }
                "--no-images" => {
                    cfg.no_images = true;
                    i += 1;
                    continue;
                }
                "--allow-absolute-image-paths" => {
                    cfg.allow_absolute_image_paths = true;
                    i += 1;
                    continue;
                }
                "-w" | "--width" => {
                    i += 1;
                    let v = args
                        .get(i)
                        .ok_or_else(|| ExitAction::Usage("--width requires a value".into()))?;
                    let w: usize = v
                        .parse()
                        .map_err(|_| ExitAction::Usage(format!("invalid width: {v}")))?;
                    if w < 10 {
                        return Err(ExitAction::Usage("width must be >= 10".into()));
                    }
                    cfg.width_override = Some(w);
                    i += 1;
                    continue;
                }
                s if s.starts_with("--width=") => {
                    let v = &s["--width=".len()..];
                    let w: usize = v
                        .parse()
                        .map_err(|_| ExitAction::Usage(format!("invalid width: {v}")))?;
                    if w < 10 {
                        return Err(ExitAction::Usage("width must be >= 10".into()));
                    }
                    cfg.width_override = Some(w);
                    i += 1;
                    continue;
                }
                "-" => {
                    cfg.sources.push(Source::Stdin);
                    i += 1;
                    continue;
                }
                s if s.starts_with("--") => {
                    return Err(ExitAction::Usage(format!("unknown option: {s}")));
                }
                s if s.starts_with('-') && s.len() > 1 => {
                    for c in s[1..].chars() {
                        match c {
                            'h' => return Err(ExitAction::PrintUsage),
                            'V' => return Err(ExitAction::PrintVersion),
                            'n' => cfg.no_color = true,
                            _ => {
                                return Err(ExitAction::Usage(format!(
                                    "unknown short option: -{c}"
                                )));
                            }
                        }
                    }
                    i += 1;
                    continue;
                }
                _ => {}
            }
        }
        cfg.sources.push(Source::File(PathBuf::from(a)));
        i += 1;
    }

    if cfg.sources.is_empty() {
        if !io::stdin().is_terminal() {
            cfg.sources.push(Source::Stdin);
        } else {
            return Err(ExitAction::PrintUsage);
        }
    }

    Ok(cfg)
}

pub fn print_usage() {
    /// Usage text. Kept as a single `&str` so `print_usage` is one `write(2)`
    /// syscall rather than ~17 line-buffered `println!` calls. The `{v}`
    /// placeholder is substituted via `format_args!` at call time.
    const USAGE: &str = "mat {v} — cat for rendered markdown

USAGE:
    mat [OPTIONS] [FILE...]

    With no FILE, or when FILE is -, read from stdin.

OPTIONS:
    -h, --help           Show this help and exit
    -V, --version        Show version and exit
    -n, --no-color       Disable ANSI colors (honors $NO_COLOR)
        --force-color    Render even when stdout is not a TTY
        --no-images      Skip image rendering
    -w, --width N        Override terminal width (default: auto)
        --allow-absolute-image-paths
                         Permit absolute or escaping image paths
                         (off by default; restricts `![](...)` to the
                         document's own directory tree)

When stdout is not a TTY and --force-color is not set, mat behaves
exactly like `cat` (raw passthrough).
";
    let v = env!("CARGO_PKG_VERSION");
    let out = USAGE.replacen("{v}", v, 1);
    let _ = io::stdout().write_all(out.as_bytes());
}

pub fn print_version() {
    println!("mat {}", env!("CARGO_PKG_VERSION"));
}
