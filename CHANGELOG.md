# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Document facade rendering now uses the same concealed wrapping path for display and full-height `--render` snapshots, preventing dropped wrap-boundary characters and preserving continuation indentation for facade list markers.

## [0.1.8] - 2026-05-21

Full commit range: [`v0.1.7...v0.1.8`](https://github.com/pacificcodeinc/glass/compare/v0.1.7...v0.1.8)

### Added

- `--render` and `--dump-render` for full-page ANSI debug snapshots that render the document body and status bar without opening the interactive editor.
- Automatic ANSI render output when `glass <path>` writes to piped or redirected stdout, such as `glass README.md | less -R`.
- Thorough `--help` and `-h` output covering usage, path behavior, render debugging, modes, keybindings, commands, mouse behavior, and examples.

### Changed

- Running `glass` without a path now prints the full help text instead of a terse usage error.

### Fixed

- Render debug output now uses the same Crossterm palette-index ANSI color mapping as the live terminal backend, so the status bar colors match interactive Glass more closely.

## [0.1.7] - 2026-05-18

Full commit range: [`v0.1.6...v0.1.7`](https://github.com/pacificcodeinc/glass/compare/v0.1.6...v0.1.7)

### Added

- Markdown table rendering with aligned columns, styled headers, escaped pipe support, and source mapping for search and selection highlights.
- Table cells wrap into additional visual rows instead of truncating long content.
- A broad `benchmark.md` fixture covering implemented Markdown behavior, known gaps, and renderer stress cases.
- GitHub Actions CI for formatting, tests, and release build checks.

### Changed

- Table body rows now use internal separators so wrapped rows remain visually distinct without adding an outside top or bottom border.
- Wrapped blockquotes keep their quiet quote marker and styling across visual rows.
- Nested bullets alternate between filled and hollow markers by indentation level.

### Fixed

- Nested blockquotes render repeated quote markers instead of falling back toward plain Markdown source.
- Long inactive table cells no longer collapse into ellipsized text in narrow article widths.
- Long benchmark prose now exercises the renderer's wrapping behavior instead of relying on manual hard wraps in the fixture.

## [0.1.6] - 2026-05-18

Full commit range: [`v0.1.5...v0.1.6`](https://github.com/pacificcodeinc/glass/compare/v0.1.5...v0.1.6)

### Added

- Bottom-attached command and search sheet for `:` and `/`, with shared fuzzy suggestions for files, commands, and in-document search results.
- Search result highlighting across visible rows, including wrapped and multi-line matches.
- Normal-mode `n` and `N` navigation through active search results, with the current result index shown in the status bar.
- Mouse support for click-to-move cursor, drag text selection, immediate clipboard copy, Command-click link opening, and wheel scrolling.

### Changed

- File suggestions appear before commands and are labeled as `FILE navigate <path>`.
- Command/search sheet styling now follows the status bar colors and attaches at the same full width.
- Temporary status labels such as opened-link and copied-selection messages disappear after 3 seconds.
- Dark terminal URL accents are lighter for better contrast.
- The status bar stays one row tall when the command/search sheet has no results.

### Fixed

- Mouse wheel scrolling now moves through visual rows in the editor without snapping the viewport back unexpectedly.
- Search commands now find text across line breaks instead of only within a single physical line.

## [0.1.5] - 2026-05-13

Full commit range: [`v0.1.4...v0.1.5`](https://github.com/pacificcodeinc/glass/compare/v0.1.4...v0.1.5)

### Changed

- Wrapped visual-row movement now preserves the intended on-screen cursor column through short wrapped rows and across physical lines.
- New note paths opened with `glass <new-file>.md` stay in memory until the first `:w`.
- Dirty files now show a dedicated status-bar indicator instead of tinting the status message.

### Fixed

- Active list rows no longer render past the current wrap segment, preventing duplicated text at wrapped line boundaries.
- Concealed inline formatting, shortened bare URLs, and covered links now wrap from their rendered text instead of raw Markdown source width.
- Vertical document jumps such as `G`, `gg`, and translated command-arrow jumps preserve the target cursor column.
- Saving a new note creates missing parent directories as part of the write.

## [0.1.4] - 2026-05-13

Full commit range: [`v0.1.3...v0.1.4`](https://github.com/pacificcodeinc/glass/compare/v0.1.3...v0.1.4)

### Added

- Structured inline link parsing for bare URLs, Markdown links, autolinks, and wiki links.
- Link following with `gf`, plus Enter activation when the cursor is on a link and not on a checkbox.
- Normal-mode `u` undo that restores text and cursor position for recent edits.

### Changed

- Links now use a subtle Pacific Code blue accent with underlines.
- Covered Markdown links render only their display text while inactive; revealed source keeps display text blue and URL syntax muted.
- The old fuzzy finder, command palette overlay, and Command+P binding have been removed ahead of a future picker rewrite.

### Fixed

- Covered link hrefs no longer leak URL fragments across wrapped visual rows.
- Link labels hide visible backticks while preserving spacing in active source rendering.
- Terminal-translated Command+Delete (`Ctrl+U`) deletes to line start before normal-mode undo can handle `u`.

## [0.1.3] - 2026-05-13

Full commit range: [`v0.1.2...v0.1.3`](https://github.com/pacificcodeinc/glass/compare/v0.1.2...v0.1.3)

### Added

- Release notes helper for generating the exact commits between a version tag and the previous tag.
- GitHub compare links for released versions.
- Wrapped-row viewport scrolling so `j` and `k` keep the cursor visible inside long wrapped lines.
- Command-line entry from visual mode, including `:q`.
- Normal-mode `A` support for entering insert mode at the end of the line.
- Command and Option navigation fallbacks for terminals that translate macOS shortcuts into control/alt key events.

### Fixed

- Synchronized the local package version in `Cargo.lock` with `Cargo.toml`.
- Inline code rendering keeps stable spacing and cursor alignment.
- Nested list content no longer renders headings or other block elements unless it is a sublist or inline formatting.
- `dd` preserves the cursor column when the next line is long enough.
- Command-line mode ignores translated navigation characters instead of inserting them.
- Command delete removes to the logical line start in insert mode.

## [0.1.2] - 2026-05-13

Full commit range: [`v0.1.1...v0.1.2`](https://github.com/pacificcodeinc/glass/compare/v0.1.1...v0.1.2)

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

Full commit range: [`v0.1.0...v0.1.1`](https://github.com/pacificcodeinc/glass/compare/v0.1.0...v0.1.1)

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

[Unreleased]: https://github.com/pacificcodeinc/glass/compare/v0.1.8...HEAD
[0.1.8]: https://github.com/pacificcodeinc/glass/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/pacificcodeinc/glass/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/pacificcodeinc/glass/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/pacificcodeinc/glass/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/pacificcodeinc/glass/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/pacificcodeinc/glass/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/pacificcodeinc/glass/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/pacificcodeinc/glass/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/pacificcodeinc/glass/releases/tag/v0.1.0
