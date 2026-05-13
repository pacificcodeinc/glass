# Glass

A fast terminal markdown editor.

## What it is

Glass is a markdown editor for people who live in the terminal. Vim inspired keybindings, centered article-style editing, live Markdown rendering, and syntax highlighting via Tree Sitter. Built with Rust and Ratatui.

## Features

- **Vim inspired editing** — Normal, Insert, Visual, and Command line modes
- **Live Markdown rendering** — Readable headings, lists, checkboxes, inline code, and links
- **Syntax highlighting** — Powered by Tree Sitter markdown grammar
- **Link navigation** — Follow Markdown, wiki, and URL links from normal mode
- **Themes** — Configurable color schemes

## Install

```bash
cargo install --path .
```

## Usage

```bash
glass <notes-directory>
```

## Keybindings

### Normal mode

| Key | Action |
|-----|--------|
| `i` | Insert mode |
| `a` | Append (insert after cursor) |
| `v` | Visual line mode |
| `:` | Command line mode |
| `h j k l` | Move cursor |
| `w b` | Word forward / backward |
| `0 ^ $` | Line start / first non-blank / end |
| `gg G` | Document top / bottom |
| `dd` | Delete line |
| `d` + motion | Delete motion |
| `x` | Delete character |
| `u` | Undo last edit |
| `gf` / `Enter` on link | Follow link under cursor |
| `Enter` on checkbox | Toggle checkbox |
| `Ctrl+C` | Quit |

### Insert mode

| Key | Action |
|-----|--------|
| `Esc` | Normal mode |
| `Tab` | Insert 4 spaces |
| `Backspace` | Delete previous character |

### Command line

| Command | Action |
|---------|--------|
| `:w` | Save |
| `:q` | Quit |
| `:q!` | Force quit |
| `:wq` | Save and quit |
| `:e <path>` | Open file |

## Build

```bash
cargo build --release
```

## Release

Glass uses the Cargo package version, git tags, and GitHub compare pages for releases.

```bash
cargo test
# Bump Cargo.toml to the next version and update CHANGELOG.md.
git add -- Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: release vX.Y.Z"
git tag vX.Y.Z
git push origin main vX.Y.Z
scripts/release-notes.sh vX.Y.Z
```

The generated release notes include every commit between `vX.Y.Z` and the previous version tag, with GitHub commit links when `origin` points at GitHub.

## License

MIT
