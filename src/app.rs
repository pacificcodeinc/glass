use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    config::theme::Theme,
    editor::{
        buffer::DocumentBuffer,
        commands::{Command, parse_command},
        cursor::Cursor,
        motions,
    },
    fs::tree::FileTree,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    CommandLine,
    VisualLine,
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub top_line: usize,
    pub horizontal_offset: usize,
    pub visible_height: usize,
    pub visible_width: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            top_line: 0,
            horizontal_offset: 0,
            visible_height: 1,
            visible_width: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    CommandPalette,
}

#[derive(Debug, Clone)]
pub struct OverlayState {
    pub kind: OverlayKind,
    pub query: String,
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct PickerItem {
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy)]
enum PaletteAction {
    Write,
    Quit,
    WriteQuit,
}

#[derive(Debug, Clone, Copy)]
struct PaletteCommand {
    label: &'static str,
    detail: &'static str,
    action: PaletteAction,
}

#[derive(Debug)]
pub struct App {
    pub notes_dir: PathBuf,
    pub buffer: DocumentBuffer,
    pub cursor: Cursor,
    pub viewport: Viewport,
    pub mode: Mode,
    pub theme: Theme,
    pub command_line: String,
    pub status_message: String,
    pub should_quit: bool,
    pub overlay: Option<OverlayState>,
    pub visual_line_anchor: Option<usize>,
    pending_g: bool,
    pending_delete: bool,
}

const PALETTE_COMMANDS: &[PaletteCommand] = &[
    PaletteCommand {
        label: "Save",
        detail: ":w",
        action: PaletteAction::Write,
    },
    PaletteCommand {
        label: "Quit",
        detail: ":q",
        action: PaletteAction::Quit,
    },
    PaletteCommand {
        label: "Save and Quit",
        detail: ":wq",
        action: PaletteAction::WriteQuit,
    },
];

impl App {
    pub fn new(notes_dir: PathBuf) -> Result<Self> {
        let file_tree = FileTree::load(&notes_dir)
            .with_context(|| format!("failed to scan notes directory: {}", notes_dir.display()))?;
        let buffer = match file_tree.selected_file() {
            Some(path) => DocumentBuffer::from_path(path)?,
            None => DocumentBuffer::empty(),
        };

        Ok(Self {
            notes_dir,
            buffer,
            cursor: Cursor::default(),
            viewport: Viewport::default(),
            mode: Mode::Normal,
            theme: Theme::system(),
            command_line: String::new(),
            status_message: "Glass".to_string(),
            should_quit: false,
            overlay: None,
            visual_line_anchor: None,
            pending_g: false,
            pending_delete: false,
        })
    }

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        if let Event::Key(key) = event {
            self.handle_key_event(key)?;
        }

        self.keep_cursor_visible();
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.kind, KeyEventKind::Release) {
            return Ok(());
        }

        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            self.request_quit(false)?;
            return Ok(());
        }

        if is_command_palette_key(key) {
            self.open_overlay(OverlayKind::CommandPalette);
            return Ok(());
        }

        if self.overlay.is_some() {
            self.handle_overlay_key(key)?;
            return Ok(());
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::Insert => self.handle_insert_key(key),
            Mode::CommandLine => self.handle_command_key(key)?,
            Mode::VisualLine => self.handle_visual_line_key(key)?,
        }

        Ok(())
    }

    pub fn resize_viewport(&mut self, visible_height: usize, visible_width: usize) {
        self.viewport.visible_height = visible_height.max(1);
        self.viewport.visible_width = visible_width.max(1);
        self.keep_cursor_visible();
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.pending_delete {
            self.pending_delete = false;
            self.delete_motion(key);
            return Ok(());
        }

        if self.pending_g {
            self.pending_g = false;
            match key.code {
                KeyCode::Char('g') => motions::document_start(&mut self.cursor),
                KeyCode::Char('e') => motions::word_end_backward(&self.buffer, &mut self.cursor),
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Char(':') => {
                self.mode = Mode::CommandLine;
                self.command_line.clear();
            }
            KeyCode::Char('V') => {
                self.mode = Mode::VisualLine;
                self.visual_line_anchor = Some(self.cursor.line);
            }
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('I') => {
                motions::first_non_blank(&self.buffer, &mut self.cursor);
                self.mode = Mode::Insert;
            }
            KeyCode::Char('a') => {
                motions::right(&self.buffer, &mut self.cursor);
                self.mode = Mode::Insert;
            }
            KeyCode::Char('A') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.mode = Mode::Insert;
            }
            KeyCode::Char('o') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.buffer.insert_char(&mut self.cursor, '\n');
                self.mode = Mode::Insert;
            }
            KeyCode::Char('O') => {
                motions::line_start(&mut self.cursor);
                self.buffer.insert_char(&mut self.cursor, '\n');
                motions::up(&self.buffer, &mut self.cursor);
                self.mode = Mode::Insert;
            }
            KeyCode::Char('h') | KeyCode::Left => motions::left(&mut self.cursor),
            KeyCode::Char('j') | KeyCode::Down => motions::down(&self.buffer, &mut self.cursor),
            KeyCode::Char('k') | KeyCode::Up => motions::up(&self.buffer, &mut self.cursor),
            KeyCode::Char('l') | KeyCode::Right => motions::right(&self.buffer, &mut self.cursor),
            KeyCode::Char('w') => motions::word_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('b') => motions::word_backward(&self.buffer, &mut self.cursor),
            KeyCode::Char('e') => motions::word_end_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('W') => motions::big_word_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('B') => motions::big_word_backward(&self.buffer, &mut self.cursor),
            KeyCode::Char('E') => motions::big_word_end_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('0') | KeyCode::Home => motions::line_start(&mut self.cursor),
            KeyCode::Char('^') => motions::first_non_blank(&self.buffer, &mut self.cursor),
            KeyCode::Char('$') | KeyCode::End => motions::line_end(&self.buffer, &mut self.cursor),
            KeyCode::Char('D') => self.delete_to_line_end(),
            KeyCode::Char('d') => {
                self.pending_delete = true;
                self.status_message = "delete".to_string();
            }
            KeyCode::Char('G') => motions::document_end(&self.buffer, &mut self.cursor),
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('x') | KeyCode::Delete => self.buffer.delete_char(&mut self.cursor),
            _ => {}
        }

        Ok(())
    }

    fn handle_insert_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.buffer.insert_char(&mut self.cursor, '\n'),
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::SUPER) {
                    // Command+Delete: delete to beginning of line
                    let end = self.buffer.char_index(self.cursor);
                    let mut target = self.cursor;
                    motions::line_start(&mut target);
                    let start = self.buffer.char_index(target);
                    self.buffer.delete_range(start, end, &mut self.cursor);
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                    // Option+Delete: delete word backward
                    let end = self.buffer.char_index(self.cursor);
                    let mut target = self.cursor;
                    motions::word_backward(&self.buffer, &mut target);
                    let start = self.buffer.char_index(target);
                    self.buffer.delete_range(start, end, &mut self.cursor);
                } else {
                    self.buffer.delete_previous_char(&mut self.cursor);
                }
            }
            KeyCode::Delete => self.buffer.delete_char(&mut self.cursor),
            KeyCode::Tab => self.buffer.insert_str(&mut self.cursor, "    "),
            KeyCode::Char(ch) => self.buffer.insert_char(&mut self.cursor, ch),
            KeyCode::Left => motions::left(&mut self.cursor),
            KeyCode::Right => motions::right(&self.buffer, &mut self.cursor),
            KeyCode::Up => motions::up(&self.buffer, &mut self.cursor),
            KeyCode::Down => motions::down(&self.buffer, &mut self.cursor),
            _ => {}
        }
    }

    fn handle_visual_line_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.pending_g {
            self.pending_g = false;
            match key.code {
                KeyCode::Char('g') => motions::document_start(&mut self.cursor),
                KeyCode::Char('e') => motions::word_end_backward(&self.buffer, &mut self.cursor),
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.visual_line_anchor = None;
            }
            KeyCode::Char('V') => {
                self.mode = Mode::Normal;
                self.visual_line_anchor = None;
            }
            KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete | KeyCode::Backspace => {
                self.delete_visual_lines();
            }
            KeyCode::Char('h') | KeyCode::Left => motions::left(&mut self.cursor),
            KeyCode::Char('j') | KeyCode::Down => motions::down(&self.buffer, &mut self.cursor),
            KeyCode::Char('k') | KeyCode::Up => motions::up(&self.buffer, &mut self.cursor),
            KeyCode::Char('l') | KeyCode::Right => motions::right(&self.buffer, &mut self.cursor),
            KeyCode::Char('w') => motions::word_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('b') => motions::word_backward(&self.buffer, &mut self.cursor),
            KeyCode::Char('e') => motions::word_end_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('W') => motions::big_word_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('B') => motions::big_word_backward(&self.buffer, &mut self.cursor),
            KeyCode::Char('E') => motions::big_word_end_forward(&self.buffer, &mut self.cursor),
            KeyCode::Char('0') | KeyCode::Home => motions::line_start(&mut self.cursor),
            KeyCode::Char('^') => motions::first_non_blank(&self.buffer, &mut self.cursor),
            KeyCode::Char('$') | KeyCode::End => motions::line_end(&self.buffer, &mut self.cursor),
            KeyCode::Char('G') => motions::document_end(&self.buffer, &mut self.cursor),
            KeyCode::Char('g') => self.pending_g = true,
            _ => {}
        }

        Ok(())
    }

    fn delete_motion(&mut self, key: KeyEvent) {
        let start_cursor = self.cursor;
        let start = self.buffer.char_index(start_cursor);
        let mut target = start_cursor;

        match key.code {
            KeyCode::Char('d') => {
                self.buffer.delete_line_range(
                    start_cursor.line,
                    start_cursor.line,
                    &mut self.cursor,
                );
                return;
            }
            KeyCode::Char('w') => motions::word_forward(&self.buffer, &mut target),
            KeyCode::Char('W') => motions::big_word_forward(&self.buffer, &mut target),
            KeyCode::Char('b') => motions::word_backward(&self.buffer, &mut target),
            KeyCode::Char('B') => motions::big_word_backward(&self.buffer, &mut target),
            KeyCode::Char('e') => {
                motions::word_end_forward(&self.buffer, &mut target);
                motions::right(&self.buffer, &mut target);
            }
            KeyCode::Char('E') => {
                motions::big_word_end_forward(&self.buffer, &mut target);
                motions::right(&self.buffer, &mut target);
            }
            KeyCode::Char('$') | KeyCode::End | KeyCode::Char('D') => {
                target.column = self.buffer.line_len_chars(target.line);
            }
            KeyCode::Char('0') | KeyCode::Home => target.column = 0,
            KeyCode::Char('^') => motions::first_non_blank(&self.buffer, &mut target),
            KeyCode::Char('G') => {
                motions::document_end(&self.buffer, &mut target);
                motions::right(&self.buffer, &mut target);
            }
            _ => return,
        }

        let end = self.buffer.char_index(target);
        self.buffer.delete_range(start, end, &mut self.cursor);
    }

    fn delete_to_line_end(&mut self) {
        let start = self.buffer.char_index(self.cursor);
        let end = self.buffer.char_index(Cursor {
            line: self.cursor.line,
            column: self.buffer.line_len_chars(self.cursor.line),
        });
        self.buffer.delete_range(start, end, &mut self.cursor);
    }

    fn delete_visual_lines(&mut self) {
        let anchor = self.visual_line_anchor.unwrap_or(self.cursor.line);
        let start = anchor.min(self.cursor.line);
        let end = anchor.max(self.cursor.line);
        self.buffer.delete_line_range(start, end, &mut self.cursor);
        self.mode = Mode::Normal;
        self.visual_line_anchor = None;
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.command_line.clear();
            }
            KeyCode::Enter => {
                let command = parse_command(&self.command_line);
                self.command_line.clear();
                self.mode = Mode::Normal;
                self.execute_command(command)?;
            }
            KeyCode::Backspace => {
                self.command_line.pop();
            }
            KeyCode::Char(ch) => self.command_line.push(ch),
            _ => {}
        }

        Ok(())
    }

    fn handle_overlay_key(&mut self, key: KeyEvent) -> Result<()> {
        let max_index = self.overlay_items().len().saturating_sub(1);
        let Some(overlay) = self.overlay.as_mut() else {
            return Ok(());
        };

        match key.code {
            KeyCode::Esc => self.overlay = None,
            KeyCode::Enter => self.execute_overlay_selection()?,
            KeyCode::Backspace => {
                overlay.query.pop();
                overlay.selected = 0;
            }
            KeyCode::Up => overlay.selected = overlay.selected.saturating_sub(1),
            KeyCode::Down => overlay.selected = (overlay.selected + 1).min(max_index),
            KeyCode::Char('k') if key.modifiers.is_empty() => {
                overlay.selected = overlay.selected.saturating_sub(1);
            }
            KeyCode::Char('j') if key.modifiers.is_empty() => {
                overlay.selected = (overlay.selected + 1).min(max_index);
            }
            KeyCode::Char(ch) => {
                overlay.query.push(ch);
                overlay.selected = 0;
            }
            _ => {}
        }

        Ok(())
    }

    fn execute_command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Write => self.save_current_file()?,
            Command::Quit { force } => self.request_quit(force)?,
            Command::WriteQuit => {
                self.save_current_file()?;
                self.should_quit = true;
            }
            Command::Edit(path) => self.open_path(&path)?,
            Command::Unknown(value) => {
                self.status_message = format!("Unknown command: {value}");
            }
        }

        Ok(())
    }

    fn save_current_file(&mut self) -> Result<()> {
        self.buffer.save()?;
        self.status_message = "Saved".to_string();
        Ok(())
    }

    fn request_quit(&mut self, force: bool) -> Result<()> {
        if self.buffer.dirty && !force {
            self.status_message = "Unsaved changes; use :wq or :q!".to_string();
            return Ok(());
        }

        self.should_quit = true;
        Ok(())
    }

    fn open_overlay(&mut self, kind: OverlayKind) {
        self.overlay = Some(OverlayState {
            kind,
            query: String::new(),
            selected: 0,
        });
        self.status_message = match kind {
            OverlayKind::CommandPalette => "Command palette".to_string(),
        };
    }

    fn execute_overlay_selection(&mut self) -> Result<()> {
        let Some(overlay) = self.overlay.clone() else {
            return Ok(());
        };

        match overlay.kind {
            OverlayKind::CommandPalette => {
                let matches = self.command_palette_matches();
                let Some(command) = matches.get(overlay.selected).copied() else {
                    return Ok(());
                };
                self.overlay = None;
                self.execute_palette_action(command.action)?;
            }
        }

        Ok(())
    }

    fn execute_palette_action(&mut self, action: PaletteAction) -> Result<()> {
        match action {
            PaletteAction::Write => self.save_current_file()?,
            PaletteAction::Quit => self.request_quit(false)?,
            PaletteAction::WriteQuit => {
                self.save_current_file()?;
                self.should_quit = true;
            }
        }

        Ok(())
    }

    pub fn overlay_title(&self) -> Option<&'static str> {
        self.overlay.as_ref().map(|overlay| match overlay.kind {
            OverlayKind::CommandPalette => "Command Palette",
        })
    }

    pub fn overlay_items(&self) -> Vec<PickerItem> {
        let Some(overlay) = &self.overlay else {
            return Vec::new();
        };

        match overlay.kind {
            OverlayKind::CommandPalette => self
                .command_palette_matches()
                .into_iter()
                .map(|command| PickerItem {
                    label: command.label.to_string(),
                    detail: command.detail.to_string(),
                })
                .collect(),
        }
    }

    fn command_palette_matches(&self) -> Vec<PaletteCommand> {
        let query = self
            .overlay
            .as_ref()
            .map(|overlay| overlay.query.as_str())
            .unwrap_or_default();

        PALETTE_COMMANDS
            .iter()
            .copied()
            .filter(|command| {
                fuzzy_match(command.label, query) || fuzzy_match(command.detail, query)
            })
            .collect()
    }

    fn open_path(&mut self, path: &Path) -> Result<()> {
        if self.buffer.dirty {
            self.status_message = "Unsaved changes; save before switching files".to_string();
            return Ok(());
        }

        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.notes_dir.join(path)
        };

        self.buffer = DocumentBuffer::from_path(&path)?;
        self.cursor = Cursor::default();
        self.viewport.top_line = 0;
        self.viewport.horizontal_offset = 0;
        self.status_message = format!("Opened {}", path.display());
        Ok(())
    }

    fn keep_cursor_visible(&mut self) {
        self.buffer.clamp_cursor(&mut self.cursor);

        if self.cursor.line < self.viewport.top_line {
            self.viewport.top_line = self.cursor.line;
        }

        let bottom = self
            .viewport
            .top_line
            .saturating_add(self.viewport.visible_height.saturating_sub(1));
        if self.cursor.line > bottom {
            self.viewport.top_line = self
                .cursor
                .line
                .saturating_sub(self.viewport.visible_height.saturating_sub(1));
        }

        if self.cursor.column < self.viewport.horizontal_offset {
            self.viewport.horizontal_offset = self.cursor.column;
        }

        let right = self
            .viewport
            .horizontal_offset
            .saturating_add(self.viewport.visible_width.saturating_sub(1));
        if self.cursor.column > right {
            self.viewport.horizontal_offset = self
                .cursor
                .column
                .saturating_sub(self.viewport.visible_width.saturating_sub(1));
        }
    }
}

fn is_command_palette_key(key: KeyEvent) -> bool {
    key_char_is_p(key) && has_primary_modifier(key.modifiers)
}

fn key_char_is_p(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
}

fn has_primary_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::CONTROL)
}

fn fuzzy_match(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let mut query_chars = query.chars().map(|ch| ch.to_ascii_lowercase());
    let Some(mut needle) = query_chars.next() else {
        return true;
    };

    for ch in candidate.chars().map(|ch| ch.to_ascii_lowercase()) {
        if ch == needle {
            let Some(next) = query_chars.next() else {
                return true;
            };
            needle = next;
        }
    }

    false
}
