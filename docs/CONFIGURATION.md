# Configuration

`mat` is `cat`, but specifically for markdown documents in the terminal.
It has no config file — everything is a command-line flag or an
environment variable.

## Flags

```text
USAGE:
    mat [OPTIONS] [FILE...]
```

| Flag                             | Effect                                                                                          |
|----------------------------------|-------------------------------------------------------------------------------------------------|
| `-h`, `--help`                   | Print help and exit 0.                                                                          |
| `-V`, `--version`                | Print version and exit 0.                                                                       |
| `-n`, `--no-color`               | Disable ANSI colors. Still renders structure (word wrap, tables, list glyphs).                  |
| `--force-color`                  | Render even when stdout is not a TTY. Without this, non-TTY stdout triggers raw passthrough.    |
| `--no-images`                    | Skip inline image rendering. Images become `[image: alt]` placeholders.                         |
| `-w N`, `--width N`              | Force terminal width to `N` columns. Minimum 10, maximum 1024. Overrides detection and `$COLUMNS`. |
| `--width=N`                      | Alternative form (equals-sign syntax).                                                          |
| `--allow-absolute-image-paths`   | Permit local image references that are absolute paths or escape the source directory via `..`. Off by default — see **Security** below. |
| `--`                             | End of options; treat everything after as a positional file argument.                           |
| `-`                              | Read from stdin. May appear once per invocation.                                                |

Short flags can be bundled: `-nh` is `--no-color --help`.

## Environment variables

| Variable                         | Effect                                                                                          |
|----------------------------------|-------------------------------------------------------------------------------------------------|
| `NO_COLOR`                       | When set (any value), equivalent to `--no-color`. Follows <https://no-color.org>.               |
| `FORCE_COLOR`                    | When set (any value), equivalent to `--force-color`.                                            |
| `NO_OSC8`                        | When set, disable OSC 8 hyperlinks even on supporting terminals. Emits the `(url)` fallback.    |
| `COLUMNS`                        | Override detected terminal width. Lower precedence than `--width`.                              |
| `MAT_IMAGE_MAX_HEIGHT_SCALE`     | Max vertical scale for a single image, as a fraction of the terminal height. Default `2.5`. Any positive finite decimal; non-numeric values are ignored. |

### Precedence order for terminal width

1. `--width N`
2. `$COLUMNS`
3. `terminal_size` crate (ioctl `TIOCGWINSZ` / Windows console API)
4. `80` (hard fallback)

Final width is clamped to `[10, 1024]`.

### Precedence order for color

1. `--force-color` / `$FORCE_COLOR` overrides "not a TTY" and flips rendering on.
2. `--no-color` / `$NO_COLOR` disables all color escapes; structure still renders.
3. Otherwise: color is enabled iff stdout is a TTY.

## Installer environment variables

These are read by `install.sh` / `install.ps1`, not by the `mat`
binary. Full table in `docs/INSTALL.md`.

| Variable              | Default                | Effect                                                   |
|-----------------------|------------------------|----------------------------------------------------------|
| `MAT_REPO`            | `LayerDynamics/mat`    | GitHub `org/repo` to download releases from.             |
| `MAT_VERSION`         | `latest`               | Release tag to pin.                                      |
| `MAT_BRANCH`          | `master`               | Branch to clone for source-build fallback.               |
| `PREFIX`              | `$CARGO_HOME/bin`      | Destination directory for the binary.                    |
| `INSTALL_RUST`        | `0`                    | Bootstrap rustup if `cargo` is missing.                  |
| `FORCE_SOURCE`        | `0`                    | Skip the prebuilt download, always build from source.    |
| `MAT_SKIP_CHECKSUM`   | `0`                    | Disable SHA-256 verification (air-gapped mirrors only).  |
| `MAT_NO_PATH_UPDATE`  | `0`                    | Don't edit any shell rc file.                            |

## Build features

Defined in `Cargo.toml` under `[features]`.

| Feature   | Adds                                                                           | Requires                                    |
|-----------|--------------------------------------------------------------------------------|---------------------------------------------|
| (default) | Kitty, iTerm2, half-block image rendering; all CommonMark / GFM extensions.    | Nothing outside the crate dependency tree.  |
| `sixel`   | Real DEC SIXEL rendering for foot / mlterm / xterm-with-sixel.                 | System `libsixel` (`brew install libsixel`, `apt install libsixel-dev`). |

```bash
cargo build --release                  # default feature set
cargo build --release --features sixel # with DEC SIXEL
```

## Terminal detection

`mat` picks an image protocol by consulting, in order:

1. `$KITTY_WINDOW_ID` → Kitty graphics.
2. `$GHOSTTY_RESOURCES_DIR` → Kitty graphics (Ghostty).
3. `$ITERM_SESSION_ID` → iTerm2 inline.
4. `$TERM_PROGRAM` ∈ {`iTerm.app`, `WezTerm`, `vscode`, `ghostty`} → iTerm2 inline.
5. `$TERM` contains `kitty` → Kitty graphics.
6. `$TERM` contains `foot` / `mlterm` → Sixel.
7. DA2 probe (`\x1b[>0c`) response class 4 → Sixel.
8. Fallback → Unicode half-block.

OSC 8 hyperlink support is detected separately via `$VTE_VERSION`,
`$TERM_PROGRAM`, `$TERM`, and `$NO_OSC8`. Detection rules live in
`src/terminal.rs:137` (`detect_osc8`).

## Security-relevant defaults

- **Local image paths must stay within the source directory.** A
  markdown file at `docs/post.md` can reference `images/foo.png` but
  not `../../../../etc/passwd`. Opt out with
  `--allow-absolute-image-paths`.
- **Stdin has no trusted filesystem root.** Local image references
  from stdin-fed markdown are refused entirely. Remote images still
  work.
- **Remote image fetches are SSRF-hardened.** DNS is resolved and
  classified before the socket is opened; the resolver is pinned to
  the validated IPs; redirects are followed manually (max 5) with
  every hop re-validated; `https→http` downgrade is refused; byte cap
  is 16 MiB; timeouts are 5/15/15s (connect/read/write).
- **Input bytes cannot escape ANSI.** Every byte originating in the
  markdown source is stripped of C0 (except TAB/LF/CR), DEL, and C1
  controls before reaching the terminal.

See `docs/SECURITY.md` for the full threat model.
