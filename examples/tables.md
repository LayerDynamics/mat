# Tables

Every table is drawn with Unicode box-drawing characters (`┌─┬─┐`, `├─┼─┤`, `└─┴─┘`) and a double-line header separator (`╞═╪═╡`). Column widths are measured using the real Unicode display width, so CJK and emoji columns line up correctly. Cells that exceed the terminal width are truncated with `…`.

---

## Basic table

| Language | Year | Paradigm     |
| -------- | ---- | ------------ |
| Rust     | 2010 | systems      |
| Python   | 1991 | multi        |
| Haskell  | 1990 | functional   |
| Erlang   | 1986 | concurrent   |

---

## Column alignment

The separator row controls per-column alignment:

| Left         |     Center     |          Right |
| :----------- | :------------: | -------------: |
| apple        |      red       |           1.29 |
| banana       |     yellow     |           0.59 |
| cherry       |      deep red  |          14.99 |
| dragonfruit  |   magenta ✨   |          9.50 |

`mat` honors `:---`, `---:`, and `:---:` verbatim. Padding is inserted symmetrically for centered columns and on the correct side for left/right.

---

## CJK + emoji widths

| Name  | Language  | Greeting          |
| :---- | :-------- | :---------------- |
| Aya   | 日本語    | こんにちは 👋     |
| Chen  | 中文      | 你好 🌏           |
| Lee   | 한국어    | 안녕하세요 🇰🇷      |
| Aleks | русский   | Привет 👨‍👩‍👧 |

Double-width and zero-width-joiner sequences are measured with `unicode-width`, so the pipes still align.

---

## Ragged rows (short rows get padded)

| Column A | Column B | Column C |
| -------- | -------- | -------- |
| 1        | 2        | 3        |
| 4        | 5        |          |
| 6        |          |          |
| 7        | 8        | 9        |

Missing cells render as blank padding — no panic, no drift.

---

## Wide table that truncates

On a narrow terminal this table overflows and `mat` truncates each cell with a trailing `…` so the borders still fit inside `$COLUMNS`.

| id  | description                                          | tags                                | owner              |
| --- | ---------------------------------------------------- | ----------------------------------- | ------------------ |
| 1   | Implement word-wrap that respects Unicode width.     | renderer, unicode, wrap             | core-team          |
| 2   | Detect OSC 8 support by probing VTE version.         | terminal, osc8, detection           | @LayerDynamics     |
| 3   | Cache compiled syntaxes for subsequent code blocks.  | perf, syntect, code                 | core-team          |
| 4   | Fall back to half-block when Sixel is disabled.      | images, viuer, fallback             | core-team          |

Try rendering this file with `mat --width 60 examples/tables.md` to see truncation in action.
