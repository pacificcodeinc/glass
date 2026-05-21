# Current Glass Issues

## Markdown Typing

- Typing Markdown shortcuts into the document model is unreliable.
- Inline Markdown syntax does not consistently convert into the document model.
- `- [ ]` on a new line should create a checkbox item.
- `# ` before text should create a heading instead of dropping the marker or leaving invalid facade text.

## Scrolling And Visual Motion

- Scrolling can feel laggy or delayed.
- `G` and `Shift+V` are noticeably slow in larger documents.

## Selection And Copying

- Drag selection currently copies too eagerly.
- Copying should happen once when the user finishes dragging a selection.
- Starting a new selection should clear the previous copied-selection state.
- Copied Markdown can contain extra lines or incorrectly translated Markdown.
- `"+y` should copy through the clipboard/register path.

## Tables

- Newly created tables become glitchy when a cell is cleared.
- Empty cells should remain focusable and editable.
- Typing should not be allowed into invalid source positions beside a rendered table.

## Test Coverage

- Add app-level typing tests that drive Glass through key and mouse events.
- Use render snapshots for static layout checks and app harness tests for editing behavior.
