# Typography

A concentrated tour of every text-rendering contract `mat` implements. Use this page to screenshot the per-element palette in isolation.

---

## The six heading levels

# Heading level 1 — bold underline bright cyan
## Heading level 2 — bold bright white
### Heading level 3 — bold regular white
#### Heading level 4 — bold dim white
##### Heading level 5 — bold dim
###### Heading level 6 — italic dim

---

## Inline styles

Normal prose with **bold words**, *italic words*, ***bold italic words***, and ~~struck-through words~~ right alongside `inline code` and `let x = 42;` code snippets.

`mat` also renders **bold *italic inside bold*** and *italic **bold inside italic*** without leaking styles across the reset boundary.

---

## Smart punctuation

Before the smart-punct filter:

```text
"Hello," she said -- and 'he' replied... then nodded.
```

After `mat` renders it (curly quotes, en-dash, ellipsis):

"Hello," she said -- and 'he' replied... then nodded.

---

## Lists — every flavor

Unordered with mixed depth:

- Top level bullet `•`
- Another top-level bullet
  - Second-level bullet `◦`
    - Third-level bullet `▸`
      - Fourth-level still `▸`
- Back to top

Ordered:

1. First step
2. Second step
3. Third step

Task list:

- [x] render headings
- [x] render tables
- [x] render inline math
- [ ] render LaTeX layout (not planned for v1)

Definition list:

CommonMark
: The base specification `mat` implements — paragraphs, lists, emphasis, code spans, links, images, blockquotes, horizontal rules.

GFM
: GitHub's flavored extensions — tables, strikethrough, task lists, autolinks. All enabled in `mat`.

Extra
: Footnotes, smart punctuation, definition lists, and `$math$` passthrough — also enabled.

---

## Block quotes

A single-level quote:

> Markdown is easy to write but hard to render well. Hard means catching every edge case: wrapping CJK text, respecting combining marks, keeping inline code escapes from leaking into the next token.

Nested two levels deep:

> First level of quoting.
>
> > Second level nested inside the first. The gutter doubles so you can tell how deep you are at a glance.
> >
> > > And a third level for good measure.

---

## Horizontal rule

Above the rule.

---

Below the rule. Rules stretch to the full terminal width in a dim shade so they recede visually.
