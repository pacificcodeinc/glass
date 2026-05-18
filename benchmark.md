# Glass Markdown Benchmark

This file is a visual and interaction benchmark for Glass. It intentionally mixes
Markdown that Glass supports today with Markdown that should stay readable even
before dedicated rendering support exists.

Use it to check:

- inactive-line Markdown concealment
- active-line raw Markdown editing
- wrapped-line cursor movement
- search highlighting with `/`, `n`, and `N`
- mouse click navigation and drag selection
- link navigation with `gf`, Enter, and Command-click
- table rendering added on the `table-rendering` branch

## Headings

# Heading level 1
## Heading level 2
### Heading level 3
#### Heading level 4 currently falls back to plain Markdown

Indented headings should stay plain:

  # This is indented, so it should not render as a heading

## Paragraphs And Wrapping

This paragraph is intentionally long so it wraps across several visual rows in a
normal terminal width. It includes plain words, punctuation, and a useful search
target: glass benchmark needle. Cursor movement should preserve the intended
visual column when moving through this wrapped paragraph, and selection should
copy the selected text immediately.

This query is split across a physical line break for search testing:
multi
line needle

## Inline Formatting

Plain text with *emphasis*, _alternate emphasis_, **strong text**, and
`inline code`. Glass conceals some inline syntax on inactive rows while keeping
the active row editable as source Markdown.

Wrapped inline formatting should not leak delimiters between visual rows:
This sentence has **bold text that keeps going for a while so the wrapped row
still looks clean** and then returns to normal text.

Known gap: ~~strikethrough is not rendered yet~~.

## Links

Markdown links:

- [Glass repository](https://github.com/pacificcodeinc/glass)
- [Relative note](README.md)
- [Nested path](docs/example.md)

Bare URLs:

- https://example.com
- https://example.com/wiki/Foo_(bar)
- https://example.com/some/really/long/path/that/should/wrap/without/leaking/url/fragments?query=glass

Autolink:

<https://example.com/autolink>

Wiki links:

- [[README]]
- [[ISSUES.md]]
- [[Projects/Glass Benchmark]]

Known gap: wiki links are navigable, but their visual treatment is still not as
distinct as it should be.

## Blockquotes

> A simple quote should render with a quiet quote marker.

> A longer quote should wrap without turning into noisy syntax. It should keep
> the quote style across wrapped rows and still feel like a calm reading surface.

Nested blockquote benchmark:

> Outer quote
> > Inner quote currently falls back toward plain Markdown behavior

## Lists

Bullets:

- First bullet item
- Second bullet item with `inline code`
- Third bullet item with [a link](README.md)
  - Nested bullet item
  - Another nested bullet item that wraps for a while so continuation indentation
    can be checked visually

Alternate bullet markers:

* Star bullet
+ Plus bullet

Numbered lists:

1. First numbered item
2. Second numbered item
10. Multi-digit marker should align cleanly

Task lists:

- [ ] Unchecked task
- [x] Checked task
- [ ] Task with [a relative link](ISSUES.md)
- [x] Completed task with `inline code`

Marker-only task row:

- [ ]

## Tables

Basic table:

| Name | Role | Status |
| --- | --- | --- |
| Ada | Editor core | Done |
| Linus | Terminal polish | In progress |
| Grace | Rendering | Planned |

Aligned table:

| Item | Count | Ratio | Notes |
| :--- | ---: | :---: | --- |
| Tables | 1 | 100% | Newly rendered |
| Links | 3 | 75% | Markdown, bare URL, wiki |
| Motions | 42 | 80% | More Vim parity needed |

Narrow-width pressure table:

| Column | Long content | Number |
| --- | --- | ---: |
| Alpha | This cell is intentionally long enough to force fitting or truncation in a narrow terminal | 1200 |
| Beta | Short value | 7 |

Escaped pipe table:

| Pattern | Meaning |
| --- | --- |
| `A \| B` | Escaped pipe should stay inside the cell |
| `x \| y \| z` | Multiple escaped pipes |

Markdown inside table cells:

| Cell type | Example |
| --- | --- |
| Inline code | `cargo test --locked` |
| Link | [README](README.md) |
| Emphasis | **bold** and *italic* |

Known gap: inline Markdown inside inactive table cells is aligned, but not yet
fully concealed or styled per cell.

## Code

Inline command: `cargo test --locked`

Fenced Rust code:

```rust
fn main() {
    let message = "glass benchmark";
    println!("{message}");
}
```

Fenced shell code:

```bash
cargo fmt --all -- --check
cargo test --locked
cargo build --release --locked
```

Known gap: fenced code blocks render as code fences, but the language marker is
not specially highlighted yet.

## Rules And Separators

Horizontal rules should remain readable, even if they are not custom-rendered:

---

***

___

## Images And HTML

Image syntax:

![Alt text for a local image](assets/example.png)

Inline HTML:

<kbd>Esc</kbd> exits insert mode.

<details>
<summary>HTML details summary</summary>

This is HTML content that should remain readable as source.

</details>

Known gap: images and raw HTML are not rendered as rich elements.

## Footnotes And References

Footnote reference[^one] and another reference[^two].

[^one]: Footnote definitions are not specially rendered yet.
[^two]: This is here to make sure the source stays readable.

Reference link:

[reference-style link][glass]

[glass]: https://github.com/pacificcodeinc/glass

## Definition Lists

Glass
: A terminal Markdown editor focused on feel.

Benchmark
: A file that catches visual and interaction regressions.

Known gap: definition lists are not custom-rendered yet.

## Command And Search Words

Use these repeated words to test search result counts:

needle alpha
needle beta
needle gamma

Try these command-ish strings without accidentally executing them while editing:

:w
:q
:e benchmark.md
/needle

## Mixed Stress Section

> Quote with [a link](README.md), `inline code`, and **strong text** inside it.

- [ ] A task with a bare URL https://example.com/todo
- [x] A completed task with a wiki link [[ISSUES.md]]

| Mixed | Example | Result |
| --- | --- | --- |
| Link | [README](README.md) | should align |
| Code | `glass benchmark.md` | should align |
| Text | long plain text that needs fitting in smaller windows | should not break the table |

Final long wrapped line with many constructs: **bold words**, `inline code`,
[a link](README.md), a bare URL https://example.com/final-check, and enough
plain text to wrap several times in a narrow viewport.
