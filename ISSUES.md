# Issues

Bug & feature log for glass.

## Rendering

- [ ] **Fenced code blocks**: highlight the language identifier (e.g., `rust` in ` ```rust `).
- [ ] **Tables**: render Markdown tables.
- [ ] **Strikethrough**: render `~~text~~`, including inside list items.
- [ ] **URL display**: strip unnecessary parts (e.g., `https://`) from bare URLs that lack pretty titles.
- [ ] **Link expansion**: only expand URLs to their real Markdown form on hover, not whenever their line is active in Normal mode.
- [ ] **Inline elements**: fix broken behavior of inline elements across line breaks.
- [ ] **Wiki-links**: render `[[File.md]]` distinctly and support jumping to the linked file.

## Editor & Input

- [ ] **Mouse support**: click anywhere in the editor to move the cursor.
- [ ] **Vim motions**: achieve full parity with standard Vim motions.
- [ ] **Simple mode**: add a non-Vim editing mode (post-1.0).
- [ ] **Spell check**: add spell-checking support.
- [ ] **Line breaking**: fix general line-breaking bugs.
- [ ] **Search**: in Normal mode, pressing `/` opens a bottom popup to search for text in the current document.
- [ ] **Command bar**: typing `:` in Normal mode opens the status bar for command input. Show a fuzzy-searchable suggestion popup above the status bar with all available commands; use Tab to cycle completions and Up/Down to navigate. Example: `:table` inserts a Markdown table at the cursor. Or `:read` sets the view to read-only without expanding the markdwon when you're active on the line.

## File Management

- [ ] **File picker**: rewrite the fuzzy-find / Command+P picker from scratch.
- [x] **Old picker cleanup**: remove the fuzzy finder, command palette overlay, and Command+P binding before the rewrite.
- [x] **Lazy file creation**: when opening a new file via `glass <new-file>.md`, only create it on disk after `:w` is used.
- [ ] **New file UI**: show the unsaved-change indicator and the target filename (instead of `[no note]`) when opening a non-existent file.

## UI / Status Bar

- [x] **Sidebar branding**: fix the "Glass" text in the sidebar randomly turning red.
- [ ] **Unsaved indicator**: use a white (instead of red) unsaved-change icon next to the filename in the status bar.

## Cursor Behavior

- [x] **`G` motion**: preserve the current column, like `dd` does.
- [x] **Column preservation**: preserve the cursor column when jumping lines with `h`/`j`/arrows/`g`/`gg`.
