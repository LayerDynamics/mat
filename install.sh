#!/usr/bin/env bash
#
# mat installer — drops the binary on your PATH.
#
# Resolution order:
#   1. If a prebuilt binary for this OS/arch is published to the GitHub release
#      tagged $MAT_VERSION (default: latest), download and verify it.
#   2. Otherwise build from source (requires `cargo`). Set INSTALL_RUST=1 to
#      bootstrap rustup automatically.
#
# Usage (from a cloned repo):
#     ./install.sh                   # installs to $CARGO_HOME/bin (default ~/.cargo/bin)
#     PREFIX=/usr/local ./install.sh # installs to /usr/local/bin (may need sudo)
#
# Usage (curl | bash):
#     curl -fsSL https://raw.githubusercontent.com/LayerDynamics/mat/main/install.sh | bash
#
# Knobs:
#     MAT_REPO        github org/repo (default LayerDynamics/mat)
#     MAT_VERSION     release tag to fetch (default: latest)
#     MAT_BRANCH      branch to build from when source-building (default: master)
#     INSTALL_RUST    bootstrap rustup if cargo is missing (default 0)
#     PREFIX          install prefix; binary goes to $PREFIX/bin
#     FORCE_SOURCE    skip prebuilt download, always build from source

set -euo pipefail

# ---------- knobs (env-tunable) ----------------------------------------------
MAT_REPO="${MAT_REPO:-LayerDynamics/mat}"
MAT_VERSION="${MAT_VERSION:-latest}"
MAT_BRANCH="${MAT_BRANCH:-master}"
INSTALL_RUST="${INSTALL_RUST:-0}"
FORCE_SOURCE="${FORCE_SOURCE:-0}"
PREFIX="${PREFIX:-}"
# MAT_NO_PATH_UPDATE=1 skips the rc-file edit entirely. For users with a
# dotfile manager (chezmoi / yadm / stow), who prefer to wire PATH themselves.
MAT_NO_PATH_UPDATE="${MAT_NO_PATH_UPDATE:-0}"

# ---------- logging helpers ---------------------------------------------------
is_tty=0; [[ -t 1 ]] && is_tty=1
_c() { [[ $is_tty -eq 1 ]] && printf '\033[%sm' "$1" || true; }
bold()   { _c "1"; }
dim()    { _c "2"; }
red()    { _c "31"; }
green()  { _c "32"; }
reset()  { _c "0"; }

info()  { printf '%s[mat]%s %s\n' "$(bold)" "$(reset)" "$*"; }
ok()    { printf '%s[mat]%s %s%s%s\n' "$(bold)" "$(reset)" "$(green)" "$*" "$(reset)"; }
warn()  { printf '%s[mat]%s %swarning:%s %s\n' "$(bold)" "$(reset)" "$(red)" "$(reset)" "$*" >&2; }
die()   { printf '%s[mat]%s %serror:%s %s\n' "$(bold)" "$(reset)" "$(red)" "$(reset)" "$*" >&2; exit 1; }

need() { command -v "$1" >/dev/null 2>&1 || die "missing '$1' — please install it and rerun"; }

# ---------- platform detection -----------------------------------------------
os="$(uname -s)"; arch="$(uname -m)"
case "$os" in
    Linux)  os_tag="unknown-linux-gnu" ;;
    Darwin) os_tag="apple-darwin" ;;
    *)      os_tag="" ;;
esac
case "$arch" in
    x86_64|amd64) arch_tag="x86_64" ;;
    arm64|aarch64) arch_tag="aarch64" ;;
    *) arch_tag="" ;;
esac
target=""
[[ -n "$os_tag" && -n "$arch_tag" ]] && target="${arch_tag}-${os_tag}"

# ---------- destination -------------------------------------------------------
if [[ -n "$PREFIX" ]]; then
    dest_dir="$PREFIX/bin"
else
    dest_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
fi
mkdir -p "$dest_dir" 2>/dev/null \
    || die "cannot create $dest_dir (try with sudo, or set PREFIX to a writable path)"
dest="$dest_dir/mat"

# ---------- prebuilt path -----------------------------------------------------
fetch_prebuilt() {
    [[ "$FORCE_SOURCE" = "1" ]] && return 1
    [[ -n "$target" ]] || return 1
    need curl
    need tar

    # Every HTTPS fetch in this installer pins TLS 1.2+ and refuses a
    # plaintext redirect. A user running `curl | bash` must not be MITM'd
    # into downloading a binary over http or via a downgraded TLS session.
    local curl_opts=(--proto '=https' --tlsv1.2 -fsSL)

    local tag="$MAT_VERSION"
    if [[ "$tag" = "latest" ]]; then
        info "resolving latest release for $MAT_REPO"
        tag="$(curl "${curl_opts[@]}" "https://api.github.com/repos/$MAT_REPO/releases/latest" \
              | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1 || true)"
        [[ -n "$tag" ]] || { warn "could not resolve latest tag — falling back to source"; return 1; }
    fi
    local asset="mat-${tag}-${target}.tar.gz"
    local url="https://github.com/${MAT_REPO}/releases/download/${tag}/${asset}"
    local sha_url="https://github.com/${MAT_REPO}/releases/download/${tag}/SHA256SUMS.txt"

    info "trying prebuilt: $url"
    local tmp
    tmp="$(mktemp -d -t mat-dl-XXXXXX)"
    if ! curl "${curl_opts[@]}" "$url" -o "$tmp/$asset"; then
        warn "no prebuilt for $target at tag $tag — falling back to source"
        rm -rf "$tmp"
        return 1
    fi

    # Mandatory checksum verification. A SHA256SUMS.txt that is unreachable
    # or a hash that fails to match is a hard error — installing a binary we
    # cannot verify defeats the entire purpose of pinning releases, and is
    # the exact attack surface supply-chain compromise exploits. Use
    # MAT_SKIP_CHECKSUM=1 to knowingly override (air-gapped mirrors, release
    # drafts); warn loudly when that happens so it never becomes routine.
    if [[ "${MAT_SKIP_CHECKSUM:-0}" = "1" ]]; then
        warn "MAT_SKIP_CHECKSUM=1 — integrity verification disabled by user"
    elif curl "${curl_opts[@]}" "$sha_url" -o "$tmp/SHA256SUMS.txt" 2>/dev/null; then
        if ! ( cd "$tmp" && grep -F "$asset" SHA256SUMS.txt | shasum -a 256 -c - >/dev/null 2>&1 ); then
            rm -rf "$tmp"
            die "checksum verification FAILED for $asset — refusing to install a binary that does not match $sha_url"
        fi
    else
        rm -rf "$tmp"
        die "could not fetch $sha_url — refusing to install an unverified binary (set MAT_SKIP_CHECKSUM=1 to override, at your own risk)"
    fi

    tar -xzf "$tmp/$asset" -C "$tmp"
    local extracted="$tmp/mat-${tag}-${target}/mat"
    [[ -x "$extracted" ]] || { warn "archive contents unexpected"; rm -rf "$tmp"; return 1; }
    install -m 0755 "$extracted" "$dest"
    rm -rf "$tmp"
    ok "installed prebuilt mat ($tag, $target) → $dest"
    return 0
}

# ---------- source-build path -------------------------------------------------
build_from_source() {
    need git
    if ! command -v cargo >/dev/null 2>&1; then
        if [[ "$INSTALL_RUST" = "1" ]]; then
            info "cargo not found — installing Rust via rustup"
            curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs \
                | sh -s -- -y --default-toolchain stable
            # shellcheck disable=SC1090
            source "${CARGO_HOME:-$HOME/.cargo}/env"
        else
            die "cargo not found on PATH. Install Rust from https://rustup.rs or re-run with INSTALL_RUST=1"
        fi
    fi

    local src_dir="" cleanup_src=0
    if [[ -f "./Cargo.toml" ]] && grep -q '^name *= *"mat"' "./Cargo.toml"; then
        src_dir="$PWD"
        info "using local source tree: $src_dir"
    else
        src_dir="$(mktemp -d -t mat-src-XXXXXX)"
        cleanup_src=1
        info "cloning https://github.com/$MAT_REPO.git (branch: $MAT_BRANCH) into $src_dir"
        git clone --depth 1 --branch "$MAT_BRANCH" \
            "https://github.com/$MAT_REPO.git" "$src_dir" >/dev/null 2>&1 \
            || die "git clone failed — check MAT_REPO / MAT_BRANCH / network"
    fi

    info "building release binary (this takes ~30s on a warm cache)"
    ( cd "$src_dir" && cargo build --release --locked 2>&1 | sed 's/^/  /' ) \
        || die "cargo build failed"

    local bin_src="$src_dir/target/release/mat"
    [[ -x "$bin_src" ]] || die "build completed but $bin_src is not executable"

    install -m 0755 "$bin_src" "$dest"
    ok "installed mat → $dest (built from source)"
    [[ $cleanup_src -eq 1 ]] && rm -rf "$src_dir" || true
}

# ---------- main --------------------------------------------------------------
if ! fetch_prebuilt; then
    build_from_source
fi

# ---------- PATH setup -------------------------------------------------------
#
# Generic, user-system-driven PATH update:
#
#   1. Detect the user's login shell via $SHELL → getent passwd → dscl →
#      /bin/sh. Hits every POSIX-compliant mechanism the OS publishes, so
#      LDAP / sssd / AD-backed users and macOS Open Directory users both
#      resolve correctly — without the script having to hard-code anything
#      about the machine it's running on.
#
#   2. Map that shell basename to the rc file the user's new *interactive*
#      sessions actually re-read (`.zshrc`, `.bashrc` on Linux,
#      `.bash_profile` on macOS, `.config/fish/config.fish`, `.kshrc`,
#      `.tcshrc`, `.cshrc`, `.profile`).
#
#   3. Write a shell-correct, idempotent snippet bracketed by a marker pair
#      so re-running the installer is a no-op.
#
#   4. Honor `MAT_NO_PATH_UPDATE=1` for users with a dotfile manager.

# Detect the user's login shell in portability order. Returns a shell path.
detect_login_shell() {
    local sh=""
    [[ -n "${SHELL:-}" ]] && sh="$SHELL"
    if [[ -z "$sh" ]] && command -v getent >/dev/null 2>&1; then
        sh="$(getent passwd "${USER:-$LOGNAME}" 2>/dev/null | awk -F: '{print $7}')"
    fi
    if [[ -z "$sh" && "$(uname -s)" = "Darwin" ]] && command -v dscl >/dev/null 2>&1; then
        sh="$(dscl . -read "/Users/${USER:-$LOGNAME}" UserShell 2>/dev/null | awk '{print $2}')"
    fi
    [[ -n "$sh" ]] || sh="/bin/sh"
    printf '%s' "$sh"
}

# Pick the rc file that a *new interactive* session of $sh_base actually
# sources on this OS. That's the one where PATH sticks for the common case of
# "user runs `mat` from a freshly opened terminal".
rc_file_for_shell() {
    local sh_base="$1"
    case "$sh_base" in
        zsh)
            # Respect ZDOTDIR so users who relocate their zsh dotfiles
            # (e.g. under ~/.config/zsh) get the right file.
            printf '%s/.zshrc' "${ZDOTDIR:-$HOME}"
            ;;
        bash)
            # macOS Terminal.app opens bash as a LOGIN shell (sources
            # .bash_profile, not .bashrc — unless chained). Linux terminals
            # open bash as a non-login interactive shell (sources .bashrc).
            # Target the file each OS actually re-reads.
            if [[ "$(uname -s)" = "Darwin" ]]; then
                printf '%s/.bash_profile' "$HOME"
            else
                printf '%s/.bashrc' "$HOME"
            fi
            ;;
        fish)        printf '%s/.config/fish/config.fish' "$HOME" ;;
        ksh|mksh)    printf '%s/.kshrc' "$HOME" ;;
        tcsh)        printf '%s/.tcshrc' "$HOME" ;;
        csh)         printf '%s/.cshrc'  "$HOME" ;;
        dash|ash|sh) printf '%s/.profile' "$HOME" ;;
        *)           printf '%s/.profile' "$HOME" ;;
    esac
}

# Render a shell-syntax-correct snippet that prepends $path_expr to PATH
# idempotently. $path_expr is passed in its *literal* form (e.g. the text
# `$HOME/.cargo/bin`) so the rc line stays portable if the user's $HOME
# changes later — no absolute paths baked in.
render_path_snippet() {
    local sh_base="$1"
    local path_expr="$2"
    case "$sh_base" in
        fish)
            cat <<FISH_EOF
if test -d "$path_expr"; and not contains "$path_expr" \$PATH
    set -gx PATH "$path_expr" \$PATH
end
FISH_EOF
            ;;
        tcsh|csh)
            cat <<CSH_EOF
if ( -d "$path_expr" ) then
    setenv PATH "$path_expr":"\${PATH}"
endif
CSH_EOF
            ;;
        *)
            # bash / zsh / ksh / mksh / dash / ash / sh / unknown POSIX.
            # The case-guard prevents duplicate entries if the user
            # re-sources their rc in the same session.
            cat <<POSIX_EOF
case ":\$PATH:" in
    *":$path_expr:"*) ;;
    *) [ -d "$path_expr" ] && export PATH="$path_expr:\$PATH" ;;
esac
POSIX_EOF
            ;;
    esac
}

ensure_dir_on_path() {
    local dir="$1"
    # Convert $HOME-rooted paths to a literal `$HOME/...` form so the rc line
    # stays portable across user moves / rehomed volumes.
    local path_expr="$dir"
    if [[ "$dir" = "$HOME" || "$dir" = "$HOME"/* ]]; then
        path_expr='$HOME'"${dir#"$HOME"}"
    fi

    # Already on PATH in the *current* process? The rc file might still be
    # wiring it via some other mechanism, but there's no visible defect to
    # fix — report and stop.
    case ":$PATH:" in
        *":$dir:"*)
            ok "$dir is already on PATH"
            return 0
            ;;
    esac

    if [[ "$MAT_NO_PATH_UPDATE" = "1" ]]; then
        warn "$dir is not on PATH, and MAT_NO_PATH_UPDATE=1 — skipping rc edit"
        printf '       add this line to your shell rc manually:\n'
        printf '           export PATH="%s:$PATH"\n' "$path_expr"
        return 0
    fi

    local login_sh sh_base rc
    login_sh="$(detect_login_shell)"
    sh_base="$(basename "$login_sh")"
    rc="$(rc_file_for_shell "$sh_base")"

    local marker_begin="# >>> mat installer: add $path_expr to PATH >>>"
    local marker_end="# <<< mat installer: add $path_expr to PATH <<<"

    # Fish keeps its config under ~/.config/fish/, which may not exist on a
    # clean profile. Make the parent dir before touching the file.
    if ! mkdir -p "$(dirname "$rc")" 2>/dev/null; then
        warn "could not create $(dirname "$rc") — skipping rc edit"
        printf '       add this line to %s manually:\n           export PATH="%s:$PATH"\n' \
            "$rc" "$path_expr"
        return 0
    fi
    [[ -e "$rc" ]] || : >"$rc"

    # Idempotent: our marker already present? Nothing to do.
    if grep -Fq "$marker_begin" "$rc" 2>/dev/null; then
        ok "$dir is already registered in $rc ($sh_base)"
        return 0
    fi

    if [[ ! -w "$rc" ]]; then
        warn "$rc is not writable — skipping rc edit"
        printf '       add this line to %s manually:\n           export PATH="%s:$PATH"\n' \
            "$rc" "$path_expr"
        return 0
    fi

    local snippet
    snippet="$(render_path_snippet "$sh_base" "$path_expr")"
    {
        printf '\n%s\n' "$marker_begin"
        printf '%s\n' "$snippet"
        printf '%s\n' "$marker_end"
    } >>"$rc"

    ok "added $dir to PATH in $rc ($sh_base)"
    printf '       %sopen a new terminal or run \`source %s\` to pick it up%s\n' \
        "$(dim)" "$rc" "$(reset)"

    # ksh's $ENV contract is opt-in — many distros don't set it by default,
    # so .kshrc may never be re-read. Flag this so the user isn't confused
    # when a new ksh session still doesn't see mat.
    if [[ "$sh_base" = "ksh" || "$sh_base" = "mksh" ]]; then
        warn "ksh only sources $rc when \$ENV points at it"
        printf '       set this in your login shell: export ENV="%s"\n' "$rc"
    fi
}

ensure_dir_on_path "$dest_dir"

# ---------- success ----------------------------------------------------------
"$dest" --version >/dev/null 2>&1 && ok "$("$dest" --version)"
printf '%stry it:%s  mat README.md\n' "$(bold)" "$(reset)"
