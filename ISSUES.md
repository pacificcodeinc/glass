# Current Glass Issues

## Test Coverage

- Add app-level typing tests that drive Glass through key and mouse events.
- Use render snapshots for static layout checks and app harness tests for editing behavior.

## Recently Addressed In The v0.2.0 PR

- Markdown shortcuts now convert reliably through the document model, including `# ` headings and `- [ ]` checklist items.
- Inline Markdown syntax edits route through the shared surface layer instead of corrupting facade text.
- Drag selection copies once on mouse release, and `"+y` copies through the clipboard/register path.
- Full-block Markdown selections no longer add extra blank lines between serialized blocks.
- Rendered table cells stay editable after being cleared, empty cells remain focusable, and typing at table row edges updates the nearest model cell.
- `G` and large visual jumps reuse wrap calculations so larger documents respond more quickly.
