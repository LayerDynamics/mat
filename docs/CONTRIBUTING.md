# Contributing to mat

`mat` is `cat`, but specifically for markdown documents in the terminal.

## Quick start

```bash
git clone https://github.com/LayerDynamics/mat
cd mat
cargo build
cargo test
cargo run -- README.md
```

The repo uses Rust edition 2024. A stable toolchain with edition 2024
support (Rust 1.85+) is required.

## Commands you'll actually use

```bash
cargo build                     # debug build
cargo build --release           # release build (matches published binaries)
cargo run -- <file.md>          # render a file
cargo run -- -                  # render from stdin
cargo test                      # run every test in every crate
cargo test <name>               # run tests whose path/name contains <name>
cargo check                     # fast type-check, no codegen
cargo clippy -- -D warnings     # lint; warnings fail
cargo fmt                       # format
```

Before opening a PR:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test
```

All three must pass.

## Project layout

```text
src/
├── main.rs         binary entry — ExitCode wiring
├── lib.rs          module declarations
├── config.rs       argv parsing, AppConfig
├── process.rs      run(), passthrough, should_render
├── terminal.rs     resolve_terminal, tty probes, termios FFI
├── markdown.rs     preprocess_markdown, render()
├── state.rs        RenderState, TableState, TrailingNewlines
├── renderer.rs     every impl RenderState method
├── style.rs        StyleFlag and the ANSI palette
├── format.rs       pad_cell, border, heading_level_num
├── image.rs        remote image fetch
├── resolve.rs      SSRF guard, IP classifier, path resolution
├── sanitize.rs     C0/C1 stripping, URL/lang validation
└── utils.rs        link equality, source_base_dir, syntect lazy init

tests/
├── common/mod.rs   shared test helpers
└── *.test.rs       one test file per src/ module (explicit mapping)

examples/
├── *.md            showcase inputs
├── assets/         deterministic PNG/JPEG fixtures
└── screenshots/    recorded stills + GIFs

docs/               repo documentation (this directory)
```

Tests use a doubled `.test.rs` extension that Cargo's auto-discovery
misses, so each is wired explicitly in `Cargo.toml`. Add a new test
file the same way.

## Architectural rules

These are documented at length in `CLAUDE.md`. The highlights:

- **One file per concern.** Extract only when a concern is clearly
  separable and the existing file has genuinely outgrown it. Don't
  pre-factor.
- **`RenderState` is the whole model.** There is no layered
  architecture. All cross-event state lives on `RenderState`
  (`src/state.rs`). Behavior is `impl RenderState` in
  `src/renderer.rs`.
- **Tables and code blocks must be fully buffered before flush.** They
  cannot be streamed — column widths and syntax highlighting both
  depend on seeing the whole input.
- **Full-reset + re-apply for ANSI styles**, never matched on/off
  pairs. Early returns from event handlers would otherwise leak open
  escapes.
- **Image rendering is protocol-dispatched.** `resolve_terminal`
  picks a single `ImageProtocol`; `render_image` matches and calls the
  corresponding `viuer` encoder.
- **Inline HTML is silently stripped.** Do not add an HTML parser.
- **Every byte from the input document passes through `sanitize_text`
  before reaching the terminal.**

## Dependency policy

The current dep set in `Cargo.toml` is deliberate and minimal. Before
adding a crate:

1. Check whether an existing dep already covers the need.
2. Prefer `default-features = false` with explicit feature flags.
3. Add a comment in `Cargo.toml` explaining why the crate is needed,
   especially if it has transitive deps that merit attention.

Crates the project has intentionally avoided:

- `clap` — argv parsing is hand-written in `src/config.rs`.
- `anyhow` / `thiserror` — `io::Result<()>` flows through the renderer;
  the one error enum (`ExitAction`) is hand-written.
- `tokio` / async runtimes — `mat` is synchronous.
- `reqwest` — pulls a tokio runtime; `ureq` is used instead.
- `serde` — nothing is serialized.
- `crossterm` / `termcolor` — detection is domain-specific enough that
  the generic abstractions don't help.

Don't introduce these without a concrete reason that can't be solved
otherwise.

## Writing tests

- One test file per source module, named `<module>.test.rs`.
- Wire new test files into `Cargo.toml` under `[[test]]`.
- Prefer unit-testable pure functions (see `src/sanitize.rs`,
  `src/format.rs`, `src/resolve.rs`).
- For renderer tests, construct a `RenderState` backed by
  `Vec<u8>` and assert against the rendered bytes. See
  `tests/renderer.test.rs` for patterns.
- For remote-image tests, use `tiny_http` (dev-dependency) to stand
  up a local server. Wrap the test in an `AllowLoopbackGuard` so the
  SSRF guard permits loopback for the duration of the test:

  ```rust
  let _guard = mat::resolve::AllowLoopbackGuard::new();
  ```

  The guard drops loopback bypass when the scope ends; no other test
  can observe it.

## Writing code

### Style

- **No comments that describe what the code does.** Names do that.
  Comments explain *why* — a hidden invariant, a workaround, a
  constraint that isn't obvious from the surrounding code.
- **No TODO / FIXME / placeholder code.** Implement it or don't write
  it. Stubs hide missing work.
- **Mandatory error handling at system boundaries.** Internal code
  that can't fail doesn't need speculative validation.
- **Format codeblocks with language tags** in every markdown file.
  Fence info strings (` ```rust `) are used both for syntax
  highlighting and for the `sanitize_code_lang` filter.

### Security-sensitive changes

Anything that touches `src/sanitize.rs`, `src/resolve.rs`, or
`src/image.rs` is security-sensitive. Before merging:

1. Add a regression test that proves the new code behaves correctly.
2. Add a test that proves the attack the change prevents is actually
   prevented (e.g. a DNS-rebind mock for resolver changes, a
   `\x1b]52;` payload for sanitizer changes).
3. Read `docs/SECURITY.md` and confirm the change doesn't weaken any
   stated guarantee.

### Rules from `CLAUDE.md`

The project-level `CLAUDE.md` at the repo root captures constraints
that govern every change. Every contributor should read it before
touching code. Especially:

- When fixing bugs or security issues, write a regression test that
  confirms the fix **before** claiming it's done.
- Primary languages are TypeScript and Rust. The full test suite
  (`cargo test`) must pass with zero regressions.

## Release process

Releases are triggered by pushing a tag matching `v*`:

```bash
git tag -a v0.1.1 -m "mat 0.1.1"
git push origin v0.1.1
```

The `.github/workflows/release.yml` workflow builds prebuilt binaries
for five targets (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows
x86_64), uploads them as release assets, and writes a `SHA256SUMS.txt`
that the installer verifies.

## Reporting bugs

- **Behavior bugs** (wrong rendering, crash, bad output): open an
  issue with the exact input markdown, the terminal + version, and
  the output you saw vs expected.
- **Security issues**: see `docs/SECURITY.md` for the reporting
  channel. Do not open a public issue.

## License

By contributing you agree to license your work under the project's
dual MIT / Apache-2.0 license.
