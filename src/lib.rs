//! `mat` — cat for rendered markdown.
//!
//! Library surface used by `src/main.rs` (the binary entry) and the
//! integration tests under `tests/`. Modules mirror the file-layout
//! philosophy in `CLAUDE.md`: extract by concern once a concern has a
//! clearly-separable boundary.
//!
//! Pipeline:
//!
//! ```text
//! stdin/file → BufReader → read_to_string
//!            → pulldown_cmark::Parser (streaming events)
//!            → RenderState::dispatch loop
//!            → BufWriter<Stdout>
//! ```
//!
//! When stdout is not a TTY and `--force-color` is not set, `mat` behaves
//! exactly like `cat` (raw passthrough) — the `cat`-compatibility contract.

pub mod config;
pub mod format;
pub mod image;
pub mod markdown;
pub mod process;
pub mod renderer;
pub mod resolve;
pub mod sanitize;
pub mod state;
pub mod style;
pub mod terminal;
pub mod utils;
