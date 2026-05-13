# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2] - 2026-05-13

### Added

- **List auto-continuation** — Pressing Enter inside checkbox (`- [ ]`), numbered (`1.`), or bullet (`-`) list items creates the next list item with the correct marker and incremented number.
- **Double-Enter to exit lists** — Pressing Enter on an empty list item strips the marker and leaves a clean blank line. A second Enter inserts a normal paragraph break.
- **Checkbox toggle** — Normal mode `Enter` toggles `- [ ]` / `- [x]` on the current line.
- **Completed-item styling** — Checked checkbox items and their wrapped continuation lines render with a dim, muted style so completed tasks are visually distinct.
- **Ghost-line fix** — Trailing file newlines are no longer counted as visible lines, so `dd` and visual delete can remove the final blank line without leaving a phantom row.
- **Empty checkbox visibility** — Marker-only checkbox lines like `- [ ] ` are now fully visible, scrollable, and reachable with `G`.

### Changed

- Normal mode `Space` is now strictly the leader key (for `<Space>pv` file picker, etc.). It no longer doubles as the checkbox toggle.
- Active (cursor) line rendering now applies raw source style to **all** wrap segments on the cursor line, not just the single segment under the cursor.

### Fixed

- Word-wrap rendering of empty content after list markers.
- Active rendering and cursor positioning on wrapped continuation lines.

## [0.1.1] - 2026-05-12

### Added

- `--version` flag.
- Numbered list rendering with muted markers and aligned continuation lines.
- Fuzzy-find file picker foundation.

## [0.1.0] - 2026-05-11

### Added

- Initial release of Glass, a Markdown editor in the terminal.
- Vim-like normal/insert/visual modes.
- Live Markdown rendering with concealed syntax markers.
- Checkbox (`- [ ]` / `- [x]`) rendering.
