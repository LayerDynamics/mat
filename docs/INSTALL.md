# Installing mat

`mat` is `cat`, but specifically for markdown documents in the terminal.

## Quick install

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/LayerDynamics/mat/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
iwr -useb https://raw.githubusercontent.com/LayerDynamics/mat/main/install.ps1 | iex
```

Both installers resolve in the same order:

1. Download the prebuilt binary from the latest GitHub release that
   matches your OS + CPU architecture, verify its SHA-256, and drop it
   on your PATH.
2. If no prebuilt is available for your platform (or `FORCE_SOURCE=1`
   is set), fall back to a source build with `cargo build --release`.

## From source

```bash
git clone https://github.com/LayerDynamics/mat
cd mat
cargo build --release
./target/release/mat README.md
```

Edition is `2024`. Minimum Rust toolchain is stable with 2024-edition
support (Rust 1.85 or newer).

## From crates.io

```bash
cargo install mat
```

Installs to `$CARGO_HOME/bin` (default `~/.cargo/bin`).

## Installer knobs

Every knob below is read from the environment before `install.sh` runs.

| Variable             | Default                | Effect                                                               |
|----------------------|------------------------|----------------------------------------------------------------------|
| `MAT_REPO`           | `LayerDynamics/mat`    | GitHub `org/repo` to fetch releases from.                            |
| `MAT_VERSION`        | `latest`               | Release tag (e.g. `v0.1.0`) to pin. `latest` resolves via the API.   |
| `MAT_BRANCH`         | `master`               | Branch to clone for source fallback.                                 |
| `PREFIX`             | `$CARGO_HOME/bin`      | Install prefix; the binary lands in `$PREFIX/bin`.                   |
| `INSTALL_RUST`       | `0`                    | `1` → bootstrap rustup if `cargo` is missing.                        |
| `FORCE_SOURCE`       | `0`                    | `1` → skip the prebuilt download and always build from source.       |
| `MAT_SKIP_CHECKSUM`  | `0`                    | `1` → disable SHA-256 verification (air-gapped mirrors only).        |
| `MAT_NO_PATH_UPDATE` | `0`                    | `1` → don't edit any shell rc file; print the line to add manually.  |

All HTTPS fetches in `install.sh` pin TLS 1.2+ (`--proto '=https'
--tlsv1.2`) and refuse plaintext redirects. Running `curl | bash` does
not expose you to a downgrade attack.

### Example: system-wide install

```bash
curl -fsSL https://raw.githubusercontent.com/LayerDynamics/mat/main/install.sh \
  | PREFIX=/usr/local sudo -E bash
```

### Example: pinned version, source build only

```bash
MAT_VERSION=v0.1.0 FORCE_SOURCE=1 ./install.sh
```

### Example: bootstrap Rust then build

```bash
INSTALL_RUST=1 FORCE_SOURCE=1 ./install.sh
```

## PATH setup

The installer detects your login shell via `$SHELL` → `getent passwd` →
`dscl` (macOS) → `/bin/sh`, then edits the rc file that a **new
interactive** session of that shell actually re-reads:

| Shell        | File                                              |
|--------------|---------------------------------------------------|
| zsh          | `$ZDOTDIR/.zshrc` (falls back to `~/.zshrc`)      |
| bash (macOS) | `~/.bash_profile` (Terminal.app opens login bash) |
| bash (Linux) | `~/.bashrc`                                       |
| fish         | `~/.config/fish/config.fish`                      |
| ksh / mksh   | `~/.kshrc` (requires `$ENV` to point at it)       |
| tcsh / csh   | `~/.tcshrc` / `~/.cshrc`                          |
| dash/ash/sh  | `~/.profile`                                      |

The injected snippet is wrapped in `# >>> mat installer: …` markers so
re-running the installer is a no-op. Set `MAT_NO_PATH_UPDATE=1` if you
manage dotfiles yourself (chezmoi / yadm / stow).

## Enabling DEC SIXEL

Sixel rendering (foot, mlterm, xterm-with-sixel) requires building
with the `sixel` cargo feature and a system `libsixel` install.

```bash
# macOS
brew install libsixel
cargo build --release --features sixel

# Debian / Ubuntu
sudo apt install libsixel-dev
cargo build --release --features sixel

# Arch
sudo pacman -S libsixel
cargo build --release --features sixel
```

Without the feature, Sixel-capable terminals fall back to Unicode
half-block — still correct output, just lower resolution.

## Supported release targets

The release workflow (`.github/workflows/release.yml`) publishes
prebuilt binaries for:

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Any other target goes through the source-build path.

## Verifying a release manually

```bash
VERSION=v0.1.0
TARGET=x86_64-apple-darwin
curl -fsSLO "https://github.com/LayerDynamics/mat/releases/download/${VERSION}/mat-${VERSION}-${TARGET}.tar.gz"
curl -fsSLO "https://github.com/LayerDynamics/mat/releases/download/${VERSION}/SHA256SUMS.txt"
grep "mat-${VERSION}-${TARGET}.tar.gz" SHA256SUMS.txt | shasum -a 256 -c -
```

## Uninstalling

```bash
# Remove the binary
rm "$(command -v mat)"

# Remove the PATH line (edit the rc file)
# Look for the block between these markers:
#   # >>> mat installer: add ... to PATH >>>
#   # <<< mat installer: add ... to PATH <<<
```
