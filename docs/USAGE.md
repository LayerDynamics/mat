# Using mat

`mat` is `cat`, but specifically for markdown documents in the terminal.

## Basics

```bash
mat README.md                  # render a single file
mat docs/*.md                  # render every matching file in sequence
cat README.md | mat            # read from stdin
mat -                          # explicit stdin
mat file1.md - file2.md        # interleave files and stdin
```

With no arguments and stdin attached to a pipe, `mat` reads stdin.
With no arguments and stdin attached to a TTY, `mat` prints the usage
text and exits with code 2.

## The cat-compatibility contract

When stdout is **not** a TTY (pipe, redirect, subshell capture) and
`--force-color` is not set, `mat` emits the raw file bytes unchanged.

```bash
mat README.md | grep TODO      # works — mat passes through
mat README.md > backup.md      # byte-identical copy
diff README.md <(mat README.md)  # no diff
```

Force rendering into a pipe with `--force-color` or the `FORCE_COLOR`
environment variable:

```bash
mat --force-color README.md | less -R
FORCE_COLOR=1 mat README.md | tee rendered.txt
```

`less -R` passes ANSI escapes through; plain `less` does not.

## Common recipes

### Preview a README before committing

```bash
mat README.md
```

### Render every markdown file in a directory

```bash
mat $(find docs -name '*.md' | sort)
```

### Reproducible width for screenshots

```bash
mat --width 80 README.md
```

Locks output to 80 columns regardless of your actual terminal size —
useful for CI snapshots and documentation captures.

### Skip image rendering

```bash
mat --no-images examples/images.md
```

Images collapse to a dim `[image: alt]` label. Useful on terminals with
slow graphics protocols or over a laggy SSH link.

### Disable color

```bash
mat --no-color README.md
mat -n README.md
NO_COLOR=1 mat README.md       # honors https://no-color.org
```

All three are equivalent.

### Pipe through a pager

```bash
mat --force-color README.md | less -R
```

`-R` tells `less` to preserve ANSI escape sequences.

### Compare against `cat`

```bash
mat README.md > /tmp/rendered
cat README.md > /tmp/raw
diff /tmp/rendered /tmp/raw    # empty diff — cat-parity holds in pipes
```

### Use as a man-page-style viewer for repo docs

```bash
alias docs='mat'
docs docs/INSTALL.md
docs docs/CONFIGURATION.md
```

### Render in CI

CI runners don't have a TTY, so `mat` defaults to passthrough. Force
rendering explicitly to get the styled output into the log:

```bash
FORCE_COLOR=1 COLUMNS=120 mat CHANGELOG.md
```

### Piping between tools

```bash
# Live-preview a markdown file while editing (fswatch on macOS)
fswatch -0 README.md | xargs -0 -n1 -I{} sh -c 'clear; mat {}'

# Git hook — render a PR description in the terminal
git log -1 --pretty=%B | mat -
```

## Flag reference

See `docs/CONFIGURATION.md` for the full reference. The short version:

```text
-h, --help                       Show help
-V, --version                    Show version
-n, --no-color                   Disable ANSI colors
    --force-color                Render even when stdout isn't a TTY
    --no-images                  Skip image rendering
-w, --width N                    Override detected terminal width
    --allow-absolute-image-paths Permit image paths outside the source directory
--                               End of options; remaining args are files
-                                Read from stdin
```

## Exit codes

| Code | Meaning                                                   |
|-----:|-----------------------------------------------------------|
| `0`  | Every source rendered successfully.                       |
| `1`  | At least one source failed (permission denied, not found, I/O error). `mat` still renders the other sources. |
| `2`  | Argument parsing error (unknown flag, missing value, etc.). Usage is printed to stderr. |

Per-source failures write `mat: <source>: <error>` to stderr and
continue — matching `cat`'s multi-file behavior.

## What mat does NOT do

- **It doesn't page.** Pipe to `less -R` if you want scrolling.
- **It doesn't edit.** `mat` never writes to the input file.
- **It doesn't reflow markdown.** Source stays exactly as written; only
  the rendered view is transformed.
- **It doesn't serve HTTP, embed in editors, or fetch non-image URLs.**
  The only network call is for remote image references, and only when
  the terminal supports inline images.

## Interactions with `less`

`less` does not re-read stdin while paging, so piping a live source
(tailing a log, etc.) into `mat | less` will block until the source
closes. Use `mat --force-color file.md | less -R` for a static file.

## Interactions with SSH

Terminal capability probes (DA2, `CSI 14 t`) open `/dev/tty` and wait
up to 120 ms per probe for a response. Over a high-latency SSH link
the probes still complete within that budget for a real terminal;
dumb remote shells silently time out and fall back to defaults.

If you see startup lag on an unusual remote terminal, confirm with:

```bash
time mat --width 80 /dev/null < README.md
```

The 120 ms budget is per-probe, not cumulative across sources.
