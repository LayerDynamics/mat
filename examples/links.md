# Links, footnotes, and inline math

## Hyperlinks

`mat` emits OSC 8 (`\x1b]8;;URL\x1b\\text\x1b]8;;\x1b\\`) on every terminal that advertises support — Kitty, iTerm2, WezTerm, Ghostty, Hyper, vscode, and any VTE-based terminal (gnome-terminal, tilix, ptyxis) at VTE ≥ 0.50. On unsupported terminals the display text is followed by a dim `(url)` suffix.

- A plain-display link: [mat on GitHub](https://github.com/LayerDynamics/mat)
- A link whose text is already the URL: <https://github.com/LayerDynamics/mat> — printed once, not twice.
- An autolinked email: <hello@example.com>
- A relative-ish link (rendered as-is): [release notes](./CHANGELOG.md)

The OSC 8 opener is deferred until the first visible word of the link text, so a link that wraps across the terminal width keeps its clickable region on one line instead of being split by the soft line break.

---

## Footnotes

`mat` buffers footnote bodies and flushes them at the end of the document in **first-reference order**, with a dim rule separator. Referencing a definition before it appears is fine[^forward]. Multiple references get sequential numbers[^alpha][^beta][^gamma]. An orphan reference with no definition keeps its marker literal.

[^alpha]: First body — appears as `[1]` because `[^alpha]` was the first distinct reference.
[^beta]: Second body — `[2]`.
[^gamma]: Third body — `[3]`.
[^forward]: A forward reference still gets a numeric slot and prints in the order its marker first appears in the text.

---

## Inline and display math

Math is parsed when `Options::ENABLE_MATH` is on (it is) and rendered as reverse-video code per the v1 contract in `docs/Logic.md`.

Inline: the Pythagorean identity is $a^2 + b^2 = c^2$, and Euler's formula is $e^{i\pi} + 1 = 0$.

Display:

$$
\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}
$$

Shell-variable references that look like `$TERM` or `$COLUMNS` remain literal because the delimiters aren't balanced adjacent to non-space text.

---

## Mixing

A paragraph that bundles **bold**, [a link](https://example.com), `inline code`, *italic*, a footnote[^bundle], and inline math $x + y = z$ — all on the same line — without any style leak across token boundaries.

[^bundle]: The renderer's style stack handles this by using a full-reset + replay pattern on every text write rather than paired on/off escapes, so an early return from any event handler cannot leak formatting into the next token.
