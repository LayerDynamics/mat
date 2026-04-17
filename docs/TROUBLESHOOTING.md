# Troubleshooting

`mat` is `cat`, but specifically for markdown documents in the terminal.

## Installation

### `cargo: command not found`

The source-build path needs a Rust toolchain. Either install rustup:

```bash
curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh
```

or let the installer do it for you:

```bash
INSTALL_RUST=1 ./install.sh
```

### `error: package requires rust 1.85 or newer`

Edition 2024 needs a recent stable toolchain. Update:

```bash
rustup update stable
```

### `install.sh`: "checksum verification FAILED"

The downloaded archive does not match the hash in `SHA256SUMS.txt`.
Do **not** set `MAT_SKIP_CHECKSUM=1` and continue — either the
upload was corrupted or something in the middle is tampering with
the download. Retry with a fresh shell. If it still fails, report
the issue.

### `install.sh`: "could not fetch SHA256SUMS.txt"

The release either predates checksum publishing or the asset
upload is incomplete. Pin an explicit version:

```bash
MAT_VERSION=v0.1.0 ./install.sh
```

### Binary installs but `mat: command not found` in a new shell

The installer edits the rc file for your detected login shell, but
only new interactive sessions re-read it. Options:

```bash
exec $SHELL              # restart the current shell
source ~/.zshrc          # (or whichever rc file was edited)
open a new terminal window
```

If your login shell is `ksh` / `mksh`, also set `$ENV`:

```bash
export ENV="$HOME/.kshrc"
```

### PATH line was not added

`MAT_NO_PATH_UPDATE=1` was set, or the rc file is not writable, or
the directory holding it could not be created. Re-run the installer
without `MAT_NO_PATH_UPDATE`, and if it still skips, add the line
manually:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## Rendering

### Colors don't show up in a pipe

That's by design — the cat-compatibility contract. Force color on:

```bash
mat --force-color README.md | less -R
FORCE_COLOR=1 mat README.md | tee rendered.txt
```

`less -R` is required; plain `less` strips escapes.

### Colors still don't show up with `--force-color`

Check for `NO_COLOR`:

```bash
env | grep -i color
unset NO_COLOR
```

`$NO_COLOR` (per <https://no-color.org>) wins over `--force-color`
for the color toggle; `--force-color` only flips the render/passthrough
switch.

### Word wrap is wrong / too narrow / too wide

`mat` picks a width in this order:

1. `--width N`
2. `$COLUMNS`
3. `terminal_size` detection
4. 80 (hard fallback)

Check each:

```bash
echo "COLUMNS=$COLUMNS"
tput cols
mat --width 100 README.md
```

If `$COLUMNS` is stale (set by an old shell, never re-exported), either
`unset COLUMNS` or pass `--width` explicitly.

### Tables are truncated / compressed

The table was wider than `term.width - current_indent`. `mat`
proportionally scales column widths down with a trailing `…`
truncation. Options:

```bash
mat --width 200 wide-tables.md     # more horizontal room
mat --no-color wide-tables.md      # no change to width math, but removes color escapes from the diff
```

Cell content itself is measured with `unicode-width` — CJK / emoji
take 2 columns, combining marks 0.

### Images don't render

Check the image protocol detection:

```bash
# What protocol does mat think your terminal supports?
env | grep -Ei 'TERM|KITTY|ITERM|GHOSTTY|VTE|COLORTERM'
```

Expected mapping:

| Terminal                 | Protocol        | Requires                                   |
|--------------------------|-----------------|--------------------------------------------|
| Kitty / Ghostty          | Kitty graphics  | —                                          |
| iTerm2 / WezTerm / VSCode| iTerm2 inline   | —                                          |
| foot / mlterm            | Sixel           | `cargo build --release --features sixel`   |
| Everything else          | Half-block      | —                                          |

If `mat` falls back to `[image: …]`:

- Stdout might not be a TTY — image escapes aren't emitted into pipes.
- `--no-images` might be set.
- For SIXEL terminals: binary was built without `--features sixel`.
  Rebuild with it and install `libsixel` at the system level.

### SIXEL output is garbled / blank

Your binary wasn't built with `--features sixel`. Without the
feature, a SIXEL-detected terminal is forced to half-block instead
of emitting a Kitty or iTerm2 escape that the terminal can't read.
Rebuild:

```bash
# macOS
brew install libsixel
cargo build --release --features sixel

# Debian / Ubuntu
sudo apt install libsixel-dev
cargo build --release --features sixel
```

### Image is too tall / spills off the viewport

Tune `MAT_IMAGE_MAX_HEIGHT_SCALE`:

```bash
MAT_IMAGE_MAX_HEIGHT_SCALE=1.0 mat README.md   # cap at one viewport
MAT_IMAGE_MAX_HEIGHT_SCALE=0.5 mat README.md   # cap at half a viewport
```

Default is `2.5`, which lets landscape-oriented screenshots render
at natural aspect without squashing their contents.

### Image fails with "(outside source directory)"

The markdown references a path that canonicalizes outside its own
directory tree (relative `..` or absolute). Move the image inside
the document's directory, or opt in:

```bash
mat --allow-absolute-image-paths article.md
```

Be cautious with attacker-authored markdown — the default exists so
`![](/etc/passwd)` doesn't read whatever the process can.

### Image fails with "(no source dir)"

The markdown was read from stdin, which has no trusted filesystem
root. Local image access is refused for stdin input. Remote images
still work. To use local images, read from a file:

```bash
mat article.md                 # works
cat article.md | mat           # local images refused
```

### Image fetch fails with "blocked IP"

The remote URL resolved to an IP in a forbidden range (loopback,
RFC1918, link-local, IPv6 ULA, etc.). This is the SSRF guard
refusing to connect. Expected behavior.

### Image fetch fails with "redirect downgrades https→http"

The remote server returned a 3xx pointing at an `http://` URL.
`mat` refuses the downgrade. The fix is server-side (issue the
redirect to an `https://` target).

### Clickable links don't work

OSC 8 hyperlink support is detected via `$KITTY_WINDOW_ID`,
`$GHOSTTY_RESOURCES_DIR`, `$VTE_VERSION`, `$TERM_PROGRAM`, `$TERM`,
and `$NO_OSC8`. If `mat` thinks your terminal doesn't support OSC 8,
it prints `(url)` after the display text instead.

Check:

```bash
env | grep -E 'NO_OSC8|VTE_VERSION|TERM_PROGRAM'
```

Explicitly turn OSC 8 off with `NO_OSC8=1`. Otherwise, if your
terminal supports OSC 8 but `mat` doesn't recognize it, file an
issue — the detection in `src/terminal.rs:137` is a short explicit
list and we expand it per terminal.

### Syntax highlighting is missing for a language

`mat` uses the syntect default syntaxes (Sublime Text's
`Packages/`). If the fence label doesn't match any of them, the
code renders without highlighting. Check with:

```bash
mat <(echo '```rust
fn main() {}
```')
```

If that works, the language label in your document doesn't match
any known syntect token / extension. Use a canonical name (`rust`
not `rust-lang`, `typescript` not `ts`).

### Code block language label is wrong

`mat` sanitizes the fence language tag to `[A-Za-z0-9+\-._#]+` with
max 32 chars. Anything else is dropped. If you need an unusual
label verbatim, the tag will show what survived the filter; for
syntax lookup, the filtered tag is also what syntect sees.

## Performance

### Startup is noticeably slow

`mat` is designed to start within tens of milliseconds. If you see
seconds of lag:

- Terminal probes might be timing out. The budget is 120 ms each
  for DA2 and `CSI 14 t`. Over a very laggy SSH session two probes
  can add ~240 ms. That's still sub-second on any real link.
- Source-building at install time is cargo, not mat — the one-time
  cost is ~30 seconds on a warm cache.
- The syntect default syntax set and theme are loaded lazily the
  first time a code block is rendered. A document with no code
  blocks pays zero syntect cost.

Measure:

```bash
time mat --width 80 README.md > /dev/null
```

Should be on the order of 10–50 ms for a typical README.

### Rendering a huge markdown file is slow

`mat` streams events but buffers tables and code blocks whole. A
single ~50 MB markdown file with a ~10 MB code block will allocate
that much memory before flushing. Use `cat` or `less` for enormous
inputs.

## Getting help

- Run `mat --help` for the flag reference.
- Read `docs/USAGE.md` for recipes.
- Read `docs/CONFIGURATION.md` for every knob.
- Search existing issues at <https://github.com/LayerDynamics/mat/issues>
  before filing a new one.

When opening a bug report, include:

- `mat --version`
- The terminal emulator and its version.
- Output of `env | grep -E 'TERM|COLUMNS|NO_COLOR|FORCE_COLOR'`.
- A minimal markdown file that reproduces the problem.
- What you expected vs what you saw.
