# mat — feature showcase

> A one-page tour of every feature `mat` renders to the terminal. Run with `mat examples/showcase.md`.

---

## Headings

# H1 heading
## H2 heading
### H3 heading
#### H4 heading
##### H5 heading
###### H6 heading

---

## Inline formatting

A paragraph with **bold**, *italic*, ***bold-italic***, ~~strikethrough~~, and `inline code`. Smart punctuation is on, so "quoted" text becomes curly, 'apostrophes' do too, and `--` becomes an en-dash like this: — ellipses too…

---

## Lists

Unordered:

- first item
- second item with a **bold** word
  - nested level
    - deeper

Ordered:

1. one
2. two
3. three

Task list:

- [x] ship the renderer
- [x] word-wrap UTF-8 correctly
- [ ] add SVG support (someday)

Definition list:

Term 1
: First explanation. This block is rendered in italics.

Term 2
: Second explanation.

---

## Block quote

> "A CLI that prints markdown the way you meant it to look."
>
> > Nested quotes work too — the gutter doubles up to `│ │` so the indent
> > level is always visible at a glance.

---

## Code block (Rust, syntax-highlighted)

```rust
fn fibonacci(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a + b;
        a = b;
        b = next;
    }
    a
}
```

---

## Table

| Feature                | Status | Notes                        |
| :--------------------- | :----: | ---------------------------: |
| CommonMark + GFM       |   ✅   |                full coverage |
| Inline images          |   ✅   | Kitty / iTerm2 / Sixel / ½▀ |
| OSC 8 hyperlinks       |   ✅   |   terminals with VTE ≥ 0.50 |
| Syntax highlighting    |   ✅   |       24-bit truecolor      |
| Tables with alignment  |   ✅   |          left/center/right |

---

## Hyperlinks

Visit [the project repository](https://github.com/LayerDynamics/mat) — rendered as a clickable OSC 8 link on supporting terminals, with a `(url)` fallback otherwise. Bare URLs like <https://example.com> print once.

---

## Footnote

`mat` renders faster than the garbage collector can even wake up[^speed]. The
footnote body prints below the main content with a dim rule separator.

[^speed]: Measured on an M-series MacBook: cold-start ≈ 8 ms for a
  code-heavy document, because `syntect` is loaded lazily only when a
  fenced code block is actually encountered.

---

## Image

![a rainbow gradient, demo asset](assets/gradient.png)

![a deterministic color mosaic](assets/mosaic.png)

---

## Horizontal rule

Above the rule.

---

Below the rule.
