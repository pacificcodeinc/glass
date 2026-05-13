# Glass

A fast terminal markdown editor.

## What it is

Glass is a markdown editor for people who live in the terminal. Vim inspired keybindings, fuzzy file picker, split view with live rendered preview, and syntax highlighting via Tree Sitter. Built with Rust and Ratatui.

## Features

- **Vim inspired editing** — Normal, Insert, Visual, and Command line modes
- **Fuzzy file picker** — Open files instantly with fuzzy search
- **Live preview** — Split view showing rendered markdown alongside the source
- **Syntax highlighting** — Powered by Tree Sitter markdown grammar
- **Command palette** — Quick access to save, quit, and other actions
- **File tree** — Sidebar view of your notes directory
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
| `Ctrl+C` | Quit |
| `Ctrl+P` | Command palette |
| `Space p v` | File picker |

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
