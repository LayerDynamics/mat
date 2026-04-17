# Security

`mat` is `cat`, but specifically for markdown documents in the terminal
— which means it renders attacker-controlled input (markdown files,
sometimes fetched over the network) directly to the terminal. This
document covers the threat model, the mitigations in place, and how to
report a vulnerability.

## Reporting a vulnerability

**Do not open a public GitHub issue.** Instead:

- Email the maintainer at the address listed on
  <https://github.com/LayerDynamics>.
- Include a minimal reproducer — the markdown input, the terminal
  used, and the observed behavior.
- Expect an acknowledgement within a few days. Patches are
  prioritized over new features.

Please give us a reasonable window to ship a fix and release before
publicly disclosing.

## Threat model

The adversary is the author of a markdown document being rendered by
`mat`. The document can contain:

- Arbitrary text, including raw C0 / C1 control bytes.
- Links (`[…](…)`) and autolinks (`<https://…>`).
- Local image references (`![](path)`).
- Remote image references (`![](https://…)`).
- Fenced code blocks with arbitrary language tags.
- HTML-looking content (tags, entities).

Goals an attacker might pursue through a malicious markdown file:

1. Inject ANSI escape sequences to rewrite the terminal (change title,
   set clipboard, open a hostile OSC 8 link, corrupt later output).
2. Read a file from disk outside the source document's directory via
   a local image reference (`![](../../../../etc/passwd)`).
3. Probe the local network / cloud metadata service via a remote
   image reference (SSRF against 127.0.0.1, RFC1918, IMDS at
   169.254.169.254, IPv6 ULA / link-local).
4. Force `mat` to download unbounded data or hang on a remote image.
5. Crash the process.

## Mitigations

### ANSI / control-byte injection

Every byte originating in the document passes through
`sanitize_text` (`src/sanitize.rs:27`) before reaching any buffer or
the terminal. This strips:

- C0 controls (`0x00`–`0x1F`) **except** TAB (`0x09`), LF (`0x0A`),
  CR (`0x0D`).
- DEL (`0x7F`).
- The entire C1 range (`0x80`–`0x9F`).

The fast path returns `Cow::Borrowed` when no dangerous byte is
present, so ordinary input is not reallocated.

**Fence language tags** (` ```lang `) have an additional filter —
`sanitize_code_lang` (`src/sanitize.rs:60`) — that restricts the tag
to `[A-Za-z0-9+\-._#]+`, max 32 chars. The tag is printed verbatim
above the block and used as a syntect lookup key, so a fence like
```` ```\x1b]0;title\x07 ```` cannot set the terminal title.

**OSC 8 URLs** are narrower still — `sanitize_osc_url`
(`src/sanitize.rs:81`) admits only bytes in `[0x20, 0x7E]` (printable
ASCII). A URL containing ESC / BEL / NUL / ST could close the
hyperlink escape early and retarget every subsequent link on the
line. When the check fails, `mat` emits the display text without the
OSC 8 wrapper and falls back to the plain `(url)` suffix.

### Path traversal through local images

Local image references (`![](foo.png)`) resolve against the source
document's parent directory. `resolve_local_image_path`
(`src/resolve.rs:226`):

1. Refuses the empty path.
2. If the path is absolute, refuses unless
   `--allow-absolute-image-paths` is set.
3. Canonicalizes the target and confirms it sits under the
   canonicalized base directory. Paths escaping via `..` are refused
   with `outside source directory`.

**Stdin has no trusted filesystem root.** When the document is read
from stdin, local image access is refused entirely (`src/renderer.rs:1079`).

### SSRF through remote images

Remote image fetches (`src/image.rs:50`, `fetch_remote_image_to_temp`)
go through a multi-layered guard in `src/resolve.rs`:

1. **IP classification** (`is_ip_forbidden_for_remote_fetch`,
   `src/resolve.rs:26`). Forbidden ranges:
   - IPv4: loopback, RFC1918 private (10/8, 172.16/12, 192.168/16),
     link-local (169.254/16, including the AWS IMDS), multicast,
     broadcast, unspecified, CG-NAT (100.64/10), 192.0.0/24,
     documentation (192.0.2/24, 198.51.100/24, 203.0.113/24),
     benchmark (198.18/15), class E (240/4 and 255/8).
   - IPv6: loopback, unspecified, multicast, ULA (`fc00::/7`),
     link-local (`fe80::/10`), documentation (`2001:db8::/32`),
     ORCHID (`100::/64`), Teredo (`2001::/32`), any IPv4-mapped /
     IPv4-compatible address that maps to a forbidden IPv4.

2. **Up-front DNS resolution** (`resolve_and_validate_host`,
   `src/resolve.rs:269`). Every address returned by the system
   resolver is tested against the classifier before the socket is
   opened. If any returned IP is forbidden, the fetch is refused.

3. **Pinned-address resolver** (`AllowListResolver`,
   `src/resolve.rs:212`). `ureq` is wired to dial **exactly** the
   validated IPs. A second DNS lookup at connect time cannot return
   a different (possibly internal) address. This defeats DNS
   rebinding.

4. **Manual redirect handling.** `ureq` is configured with
   `.redirects(0)` so it never follows a redirect on our behalf.
   `fetch_remote_image_to_temp` loops up to 5 times, re-running
   steps 1–3 on every hop's URL. A redirect from `https://…` to
   `http://…` is refused (scheme downgrade).

5. **Resource caps.** Connect timeout 5s, read/write timeouts 15s,
   hard 16 MiB byte cap (`MAX_REMOTE_IMAGE_BYTES`,
   `src/image.rs:19`), `User-Agent: mat/<version>`.

6. **Content-Type filter.** Responses must declare `image/*` or
   `application/octet-stream`. `text/html` and friends are refused.

### Test-only SSRF bypass

`AllowLoopbackGuard` (`src/resolve.rs:360`) is a thread-local RAII
guard that relaxes the loopback deny. It exists so integration tests
can spin up `tiny_http` on 127.0.0.1 and exercise the remote-image
path. The binary never constructs a guard, so the bypass is never
active in production code. The guard is always compiled (not
`#[cfg(test)]`-gated) because integration tests link against the
non-test build of the library.

### Non-TTY pipe safety

Image escape sequences are meaningless — and potentially harmful —
when stdout is not a real TTY. `render_image` (`src/renderer.rs:1041`)
falls back to the textual `[image: alt]` placeholder when
`!term.is_tty`, even if the user forced rendering with
`--force-color`. This prevents a pipe consumer from receiving raw
image bytes it doesn't know how to handle.

### Resource exhaustion

- **Terminal width** is clamped to `[10, 1024]` in `resolve_terminal`
  (`src/terminal.rs:69`). Absurd `$COLUMNS` values cannot push the
  renderer into O(width²) work through table layout.
- **TTY probes** have a 120 ms budget each
  (`TTY_PROBE_TIMEOUT`, `src/terminal.rs:30`). A non-responsive
  terminal does not block startup.
- **Remote image size** is capped at 16 MiB (above).
- **Code blocks** are buffered in memory; `mat` does not attempt to
  render multi-gigabyte markdown files. Use `cat` or `less` for
  those.

### Install-time integrity

`install.sh` and `install.ps1` pin TLS 1.2+ on every HTTPS request
(`curl --proto '=https' --tlsv1.2`) and **require** that the
published `SHA256SUMS.txt` verify the downloaded archive before
installing. Verification failure is a hard error. `MAT_SKIP_CHECKSUM=1`
disables the check for air-gapped mirrors and warns loudly. The
release workflow (`.github/workflows/release.yml`) writes
`SHA256SUMS.txt` server-side, so the hash is under GitHub's
release-asset permissions model.

## What mat does NOT defend against

- **Compromised terminal emulators.** If the terminal itself has a
  vulnerability in its ANSI parser (historically a thing), `mat`
  cannot protect against it. Stripped C0/C1 controls reduce the
  attack surface but do not eliminate it.
- **Side channels in rendering time.** `mat` does not attempt
  constant-time behavior.
- **Resource exhaustion via deeply-nested markdown.** A document
  with 10 000 levels of nested blockquotes will render; it will also
  emit 10 000 `│` bars per line and thrash the terminal. Use within
  reason.
- **Typosquatting on `cargo install`.** Users who run
  `cargo install mat` are trusting that the `mat` name on crates.io
  points to this project. Verify via the repository link on the
  crate page.

## Versioned guarantees

The guarantees above apply to `mat` 0.1.0 and later on the current
`master` branch. Security-relevant behavior changes (loosening a
default, adding a new protocol, changing the IP classifier) will be
called out in release notes.
