# Issues

- [ ] Fenced code blocks: highlight the language marker in fences like ```rust.
- [ ] File picker: rewrite the fuzzy-find / Command+P picker from scratch.
- [x] Old picker: remove the fuzzy finder, command palette overlay, and Command+P binding before the rewrite.
- [ ] Mouse support: click anywhere in the editor to move the cursor.
- [ ] Tables: render Markdown tables.
- [x] Wiki-links: render `[[File.md]]` distinctly and jump to the linked file.
- [ ] Strikethrough: render `~~text~~`, including inside list items.
- [x] When using glass to create a new file e.g., `glass <new-file>.md` only actually create the file when `:w`
- [x] "Glass" text in sidebar randomly turning red
- [x] Unsaved icon next to filename in status bar
- [x] `G` motion should preserve cursor position like `dd` does
- [ ] Simple mode (as an alternative to Vim motions)
- [x] Preserve cursor column location when jumping lines with h/j/arrows/and g and gg motions
- [x] Fix `inline` elements weird behavior on line breaks
- [ ] Strip out unnecessary information from URLs without pretty titles (e.g., https:)
- [x] Line breaking is very broken in general
