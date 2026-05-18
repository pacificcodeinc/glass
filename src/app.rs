use std::{
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    time::{Duration, Instant},
};

#[cfg(not(test))]
use std::{io::Write, process::Stdio};

use anyhow::{Context, Result};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::{
    config::theme::Theme,
    editor::{
        buffer::DocumentBuffer,
        commands::{Command, parse_command},
        cursor::Cursor,
        motions,
        render::{visible_rows, visual_line_bounds, wrap_index_for_column, wrap_line},
    },
    fs::tree::FileTree,
    markdown::inline::{LinkKind, link_at_column},
    markdown::{
        highlight::concealed_wrap_line,
        table::{TableLayout, table_wrap_line},
    },
};

const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(3);
const MOUSE_SCROLL_ROWS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    CommandLine,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandPrompt {
    Command,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetItemKind {
    Command,
    File,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetAction {
    Command(String),
    Complete(String),
    File(PathBuf),
    Search {
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
    },
    BeginSearch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetItem {
    pub kind: SheetItemKind,
    pub label: String,
    pub detail: String,
    pub replacement: String,
    pub action: SheetAction,
}

#[derive(Debug, Clone)]
pub struct CommandSheetState {
    pub items: Vec<SheetItem>,
    pub selected: usize,
    pub prompt: CommandPrompt,
    pub return_mode: Mode,
    pub return_visual_line_anchor: Option<usize>,
    pub explicit_selection: bool,
}

impl Default for CommandSheetState {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            prompt: CommandPrompt::Command,
            return_mode: Mode::Normal,
            return_visual_line_anchor: None,
            explicit_selection: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchMatch {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: Cursor,
    pub head: Cursor,
}

impl TextSelection {
    pub fn ordered(self) -> (Cursor, Cursor) {
        if cursor_before_or_equal(self.anchor, self.head) {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: u16,
    pub y: u16,
    pub top_line: usize,
    pub top_wrap_index: usize,
    pub horizontal_offset: usize,
    pub visible_height: usize,
    pub visible_width: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            top_line: 0,
            top_wrap_index: 0,
            horizontal_offset: 0,
            visible_height: 1,
            visible_width: 1,
        }
    }
}

#[derive(Debug)]
pub struct App {
    pub notes_dir: PathBuf,
    pub file_tree: FileTree,
    pub buffer: DocumentBuffer,
    pub cursor: Cursor,
    pub viewport: Viewport,
    pub mode: Mode,
    pub theme: Theme,
    pub command_line: String,
    pub sheet: CommandSheetState,
    pub search: SearchState,
    pub text_selection: Option<TextSelection>,
    pub status_message: String,
    status_message_expires_at: Option<Instant>,
    pub should_quit: bool,
    pub visual_line_anchor: Option<usize>,
    preferred_column: Option<usize>,
    preferred_visual_column: Option<usize>,
    pending_g: bool,
    pending_delete: bool,
    pending_change: bool,
    mouse_anchor: Option<Cursor>,
    last_copied_selection: Option<String>,
}

impl App {
    pub fn new(notes_dir: PathBuf, initial_file: Option<PathBuf>) -> Result<Self> {
        let file_tree = FileTree::load(&notes_dir)
            .with_context(|| format!("failed to scan notes directory: {}", notes_dir.display()))?;
        let buffer = match initial_file {
            Some(path) => DocumentBuffer::from_path_or_empty(&path)?,
            None => match file_tree.selected_file() {
                Some(path) => DocumentBuffer::from_path(path)?,
                None => DocumentBuffer::empty(),
            },
        };

        Ok(Self {
            notes_dir,
            file_tree,
            buffer,
            cursor: Cursor::default(),
            viewport: Viewport::default(),
            mode: Mode::Normal,
            theme: Theme::system(),
            command_line: String::new(),
            sheet: CommandSheetState::default(),
            search: SearchState::default(),
            text_selection: None,
            status_message: "Glass".to_string(),
            status_message_expires_at: None,
            should_quit: false,
            visual_line_anchor: None,
            preferred_column: None,
            preferred_visual_column: None,
            pending_g: false,
            pending_delete: false,
            pending_change: false,
            mouse_anchor: None,
            last_copied_selection: None,
        })
    }

    pub fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::Key(key) => self.handle_key_event(key)?,
            Event::Mouse(mouse) => self.handle_mouse_event(mouse)?,
            _ => {}
        }

        self.keep_cursor_visible();
        Ok(())
    }

    pub fn tick(&mut self) {
        if let Some(expires_at) = self.status_message_expires_at {
            if Instant::now() >= expires_at {
                self.status_message.clear();
                self.status_message_expires_at = None;
            }
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status_message = message.into();
        self.status_message_expires_at = Some(Instant::now() + STATUS_MESSAGE_TTL);
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(key.kind, KeyEventKind::Release) {
            return Ok(());
        }

        self.clear_text_selection();

        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            self.request_quit(false)?;
            return Ok(());
        }

        if self.mode != Mode::CommandLine && is_command_sheet_shortcut(key) {
            self.enter_command_sheet(CommandPrompt::Command);
            return Ok(());
        }

        if self.mode != Mode::CommandLine && self.handle_navigation_modifier(key) {
            return Ok(());
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key)?,
            Mode::Insert => self.handle_insert_key(key),
            Mode::CommandLine => self.handle_command_key(key)?,
            Mode::Visual => self.handle_visual_key(key)?,
        }

        Ok(())
    }

    pub fn resize_viewport(&mut self, visible_height: usize, visible_width: usize) {
        self.viewport.visible_height = visible_height.max(1);
        self.viewport.visible_width = visible_width.max(1);
        self.keep_cursor_visible();
    }

    pub fn move_viewport_to(&mut self, x: u16, y: u16) {
        self.viewport.x = x;
        self.viewport.y = y;
    }

    fn handle_mouse_event(&mut self, mouse: MouseEvent) -> Result<()> {
        if self.mode == Mode::CommandLine {
            return Ok(());
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(cursor) = self.cursor_for_mouse_position(mouse.column, mouse.row) else {
                    return Ok(());
                };

                self.cursor = cursor;
                self.clear_text_selection();
                self.cancel_pending_operators();
                self.reset_preferred_column();
                if mouse.modifiers.contains(KeyModifiers::SUPER) {
                    self.follow_link_under_cursor()?;
                    return Ok(());
                }

                self.mouse_anchor = Some(cursor);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(anchor) = self.mouse_anchor else {
                    return Ok(());
                };
                let Some(cursor) = self.cursor_for_mouse_position(mouse.column, mouse.row) else {
                    return Ok(());
                };

                self.cursor = cursor;
                self.update_text_selection(anchor, cursor);
                self.cancel_pending_operators();
                self.reset_preferred_column();
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let anchor = self.mouse_anchor.take();
                if let (Some(anchor), Some(cursor)) = (
                    anchor,
                    self.cursor_for_mouse_position(mouse.column, mouse.row),
                ) {
                    self.cursor = cursor;
                    self.update_text_selection(anchor, cursor);
                    self.cancel_pending_operators();
                    self.reset_preferred_column();
                }
            }
            MouseEventKind::ScrollDown => {
                if self.viewport_contains(mouse.column, mouse.row) {
                    self.scroll_visual_rows(MOUSE_SCROLL_ROWS as isize);
                }
            }
            MouseEventKind::ScrollUp => {
                if self.viewport_contains(mouse.column, mouse.row) {
                    self.scroll_visual_rows(-(MOUSE_SCROLL_ROWS as isize));
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn cancel_pending_operators(&mut self) {
        self.pending_g = false;
        self.pending_delete = false;
        self.pending_change = false;
    }

    fn clear_text_selection(&mut self) {
        self.mouse_anchor = None;
        self.text_selection = None;
        self.last_copied_selection = None;
    }

    fn update_text_selection(&mut self, anchor: Cursor, head: Cursor) {
        if anchor == head {
            self.text_selection = None;
            self.last_copied_selection = None;
            return;
        }

        self.text_selection = Some(TextSelection { anchor, head });
        let Some(selected_text) = self.selected_text() else {
            return;
        };
        if self.last_copied_selection.as_ref() == Some(&selected_text) {
            return;
        }

        match copy_to_clipboard(&selected_text) {
            Ok(()) => {
                self.last_copied_selection = Some(selected_text);
                self.set_status("Copied selection");
            }
            Err(error) => {
                self.set_status(format!("Copy failed: {error:#}"));
            }
        }
    }

    fn selected_text(&self) -> Option<String> {
        let selection = self.text_selection?;
        let (start, end) = selection.ordered();
        let start = self.buffer.char_index(start);
        let end = self.buffer.char_index(end);
        if start == end {
            return None;
        }

        Some(
            self.buffer
                .as_string()
                .chars()
                .skip(start)
                .take(end.saturating_sub(start))
                .collect(),
        )
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.pending_delete {
            self.pending_delete = false;
            self.delete_motion(key);
            return Ok(());
        }

        if self.pending_change {
            self.pending_change = false;
            self.change_motion(key);
            return Ok(());
        }

        if self.pending_g {
            self.pending_g = false;
            match key.code {
                KeyCode::Char('g') => self.document_start_preserving_column(),
                KeyCode::Char('e') => {
                    motions::word_end_backward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                }
                KeyCode::Char('f') => {
                    self.follow_link_under_cursor()?;
                    self.reset_preferred_column();
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Char(':') => {
                self.enter_command_line();
            }
            KeyCode::Char('/') => {
                self.enter_search_sheet();
            }
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.mode = Mode::Visual;
                self.visual_line_anchor = Some(self.cursor.line);
            }
            KeyCode::Enter => {
                if !self.toggle_checkbox() {
                    self.follow_link_under_cursor()?;
                }
            }
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('I') => {
                motions::first_non_blank(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('a') => {
                motions::right(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('A') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('o') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.buffer.insert_char(&mut self.cursor, '\n');
                self.reset_preferred_column();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('O') => {
                motions::line_start(&mut self.cursor);
                self.buffer.insert_char(&mut self.cursor, '\n');
                motions::up(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
                self.mode = Mode::Insert;
            }
            KeyCode::Char('h') | KeyCode::Left => {
                motions::left(&mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('j') | KeyCode::Down => self.visual_line_down(),
            KeyCode::Char('k') | KeyCode::Up => self.visual_line_up(),
            KeyCode::Char('l') | KeyCode::Right => {
                motions::right(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('w') => {
                motions::word_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('b') => {
                motions::word_backward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('e') => {
                motions::word_end_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('W') => {
                motions::big_word_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('B') => {
                motions::big_word_backward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('E') => {
                motions::big_word_end_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('0') => {
                motions::line_start(&mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Home => {
                self.visual_line_start();
                self.reset_preferred_column();
            }
            KeyCode::Char('^') => {
                motions::first_non_blank(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('$') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::End => {
                self.visual_line_end();
                self.reset_preferred_column();
            }
            KeyCode::Char('C') => self.change_to_line_end(),
            KeyCode::Char('D') => self.delete_to_line_end(),
            KeyCode::Char('c') => {
                self.pending_change = true;
                self.set_status("change");
            }
            KeyCode::Char('d') => {
                self.pending_delete = true;
                self.set_status("delete");
            }
            KeyCode::Char('G') => self.document_end_preserving_column(),
            KeyCode::Char('g') => self.pending_g = true,
            KeyCode::Char('n') => self.jump_search_match(1),
            KeyCode::Char('N') => self.jump_search_match(-1),
            KeyCode::Char('u') => self.undo(),
            KeyCode::Char('x') | KeyCode::Delete => {
                self.buffer.delete_char(&mut self.cursor);
                self.reset_preferred_column();
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_insert_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                insert_newline_with_list_continuation(&mut self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::SUPER) {
                    self.delete_to_line_start();
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                    self.delete_word_backward();
                } else {
                    self.buffer.delete_previous_char(&mut self.cursor);
                }
                self.reset_preferred_column();
            }
            KeyCode::Delete => {
                if key.modifiers.contains(KeyModifiers::SUPER) {
                    self.delete_to_line_end();
                } else {
                    self.buffer.delete_char(&mut self.cursor);
                }
                self.reset_preferred_column();
            }
            KeyCode::Tab => {
                self.buffer.insert_str(&mut self.cursor, "    ");
                self.reset_preferred_column();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_to_line_start();
                self.reset_preferred_column();
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.delete_to_line_end();
                self.reset_preferred_column();
            }
            KeyCode::Char(ch) if is_text_input_key(key) => {
                self.buffer.insert_char(&mut self.cursor, ch);
                self.reset_preferred_column();
            }
            KeyCode::Left => {
                motions::left(&mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Right => {
                motions::right(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Up => self.move_line_up_preserving_column(),
            KeyCode::Down => self.move_line_down_preserving_column(),
            KeyCode::Home => {
                self.visual_line_start();
                self.reset_preferred_column();
            }
            KeyCode::End => {
                self.visual_line_end();
                self.reset_preferred_column();
            }
            _ => {}
        }
    }

    fn handle_visual_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.pending_g {
            self.pending_g = false;
            match key.code {
                KeyCode::Char('g') => self.document_start_preserving_column(),
                KeyCode::Char('e') => {
                    motions::word_end_backward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                }
                KeyCode::Char('f') => {
                    self.follow_link_under_cursor()?;
                    self.reset_preferred_column();
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Char(':') => {
                self.enter_command_line();
            }
            KeyCode::Char('/') => {
                self.enter_search_sheet();
            }
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
            KeyCode::Char('h') | KeyCode::Left => {
                motions::left(&mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('j') | KeyCode::Down => self.visual_line_down(),
            KeyCode::Char('k') | KeyCode::Up => self.visual_line_up(),
            KeyCode::Char('l') | KeyCode::Right => {
                motions::right(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('w') => {
                motions::word_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('b') => {
                motions::word_backward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('e') => {
                motions::word_end_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('W') => {
                motions::big_word_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('B') => {
                motions::big_word_backward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('E') => {
                motions::big_word_end_forward(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('0') => {
                motions::line_start(&mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Home => {
                self.visual_line_start();
                self.reset_preferred_column();
            }
            KeyCode::Char('^') => {
                motions::first_non_blank(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::Char('$') => {
                motions::line_end(&self.buffer, &mut self.cursor);
                self.reset_preferred_column();
            }
            KeyCode::End => {
                self.visual_line_end();
                self.reset_preferred_column();
            }
            KeyCode::Char('G') => self.document_end_preserving_column(),
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
                self.reset_preferred_column();
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
        self.reset_preferred_column();
    }

    fn delete_to_line_end(&mut self) {
        let start = self.buffer.char_index(self.cursor);
        let end = self.buffer.char_index(Cursor {
            line: self.cursor.line,
            column: self.buffer.line_len_chars(self.cursor.line),
        });
        self.buffer.delete_range(start, end, &mut self.cursor);
        self.reset_preferred_column();
    }

    fn delete_to_line_start(&mut self) {
        let start = self.buffer.char_index(Cursor {
            line: self.cursor.line,
            column: 0,
        });
        let end = self.buffer.char_index(self.cursor);
        self.buffer.delete_range(start, end, &mut self.cursor);
        self.reset_preferred_column();
    }

    fn delete_word_backward(&mut self) {
        let end = self.buffer.char_index(self.cursor);
        let mut target = self.cursor;
        motions::word_backward(&self.buffer, &mut target);
        let start = self.buffer.char_index(target);
        self.buffer.delete_range(start, end, &mut self.cursor);
        self.reset_preferred_column();
    }

    fn undo(&mut self) {
        if self.buffer.undo(&mut self.cursor) {
            self.set_status("Undid change");
        } else {
            self.set_status("Already at oldest change");
        }
    }

    fn change_motion(&mut self, key: KeyEvent) {
        let start_cursor = self.cursor;
        let start = self.buffer.char_index(start_cursor);
        let mut target = start_cursor;

        match key.code {
            KeyCode::Char('c') => {
                self.buffer.delete_line_range(
                    start_cursor.line,
                    start_cursor.line,
                    &mut self.cursor,
                );
                self.mode = Mode::Insert;
                self.reset_preferred_column();
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
        self.mode = Mode::Insert;
        self.reset_preferred_column();
    }

    fn change_to_line_end(&mut self) {
        let start = self.buffer.char_index(self.cursor);
        let end = self.buffer.char_index(Cursor {
            line: self.cursor.line,
            column: self.buffer.line_len_chars(self.cursor.line),
        });
        self.buffer.delete_range(start, end, &mut self.cursor);
        self.mode = Mode::Insert;
        self.reset_preferred_column();
    }

    fn delete_visual_lines(&mut self) {
        let anchor = self.visual_line_anchor.unwrap_or(self.cursor.line);
        let start = anchor.min(self.cursor.line);
        let end = anchor.max(self.cursor.line);
        self.buffer.delete_line_range(start, end, &mut self.cursor);
        self.mode = Mode::Normal;
        self.visual_line_anchor = None;
        self.reset_preferred_column();
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.close_command_sheet();
            }
            KeyCode::Enter => {
                self.accept_command_sheet()?;
            }
            KeyCode::Backspace => {
                self.command_line.pop();
                self.sheet.explicit_selection = false;
                self.refresh_sheet_items();
            }
            KeyCode::Tab | KeyCode::Down => self.move_sheet_selection(1),
            KeyCode::BackTab | KeyCode::Up => self.move_sheet_selection(-1),
            KeyCode::Right => self.apply_sheet_completion(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.command_line.clear();
                self.sheet.explicit_selection = false;
                self.refresh_sheet_items();
            }
            KeyCode::Char(ch) if is_text_input_key(key) => {
                self.command_line.push(ch);
                self.sheet.explicit_selection = false;
                self.refresh_sheet_items();
            }
            _ => {}
        }

        Ok(())
    }

    fn enter_command_line(&mut self) {
        self.enter_command_sheet(CommandPrompt::Command);
    }

    fn enter_search_sheet(&mut self) {
        self.enter_command_sheet(CommandPrompt::Search);
    }

    fn enter_command_sheet(&mut self, prompt: CommandPrompt) {
        self.sheet.return_mode = self.mode;
        self.sheet.return_visual_line_anchor = self.visual_line_anchor;
        self.sheet.prompt = prompt;
        self.sheet.selected = 0;
        self.sheet.explicit_selection = false;
        self.mode = Mode::CommandLine;
        self.visual_line_anchor = None;
        self.command_line.clear();
        self.refresh_sheet_items();
    }

    fn close_command_sheet(&mut self) {
        if matches!(self.sheet.prompt, CommandPrompt::Search)
            || self.command_line.trim().starts_with('/')
        {
            self.clear_search();
        }
        self.mode = self.sheet.return_mode;
        self.visual_line_anchor = self.sheet.return_visual_line_anchor.take();
        self.command_line.clear();
        self.sheet.items.clear();
        self.sheet.selected = 0;
        self.sheet.explicit_selection = false;
    }

    fn finish_command_sheet(&mut self) {
        self.mode = Mode::Normal;
        self.visual_line_anchor = None;
        self.command_line.clear();
        self.sheet.items.clear();
        self.sheet.selected = 0;
        self.sheet.explicit_selection = false;
    }

    fn move_sheet_selection(&mut self, delta: isize) {
        let len = self.sheet.items.len();
        if len == 0 {
            self.sheet.selected = 0;
            return;
        }

        let current = self.sheet.selected.min(len - 1) as isize;
        let next = (current + delta).rem_euclid(len as isize) as usize;
        self.sheet.selected = next;
        self.sheet.explicit_selection = true;
    }

    fn apply_sheet_completion(&mut self) {
        let Some(item) = self.sheet.items.get(self.sheet.selected) else {
            return;
        };

        match item.action {
            SheetAction::BeginSearch => {
                self.sheet.prompt = CommandPrompt::Search;
                self.command_line.clear();
            }
            _ => {
                self.command_line = item.replacement.clone();
            }
        }
        self.sheet.selected = 0;
        self.sheet.explicit_selection = false;
        self.refresh_sheet_items();
    }

    fn accept_command_sheet(&mut self) -> Result<()> {
        let input = self.command_line.trim().to_string();
        if input.is_empty() {
            self.finish_command_sheet();
            return Ok(());
        }

        let selected = self.sheet.items.get(self.sheet.selected).cloned();
        if self.sheet.explicit_selection {
            if let Some(item) = selected {
                return self.execute_sheet_item(item);
            }
        }

        if matches!(self.sheet.prompt, CommandPrompt::Search) {
            return self.execute_search_query(&input, selected);
        }

        if let Some(search_query) = input.strip_prefix('/') {
            return self.execute_search_query(search_query.trim(), selected);
        }

        match parse_command(&input) {
            Command::Unknown(_) => {
                if let Some(item) =
                    selected.filter(|item| !matches!(item.kind, SheetItemKind::Command))
                {
                    self.execute_sheet_item(item)
                } else if let Some(path) = resolve_command_path_input(&input, &self.notes_dir) {
                    self.finish_command_sheet();
                    self.open_path(&path)
                } else {
                    self.finish_command_sheet();
                    self.execute_command(Command::Unknown(input))
                }
            }
            command => {
                self.finish_command_sheet();
                self.execute_command(command)
            }
        }
    }

    fn execute_sheet_item(&mut self, item: SheetItem) -> Result<()> {
        match item.action {
            SheetAction::Command(command) => {
                self.finish_command_sheet();
                self.execute_command(parse_command(&command))
            }
            SheetAction::Complete(value) => {
                self.command_line = value;
                self.sheet.selected = 0;
                self.sheet.explicit_selection = false;
                self.refresh_sheet_items();
                Ok(())
            }
            SheetAction::File(path) => {
                self.finish_command_sheet();
                self.open_path(&path)
            }
            SheetAction::Search {
                line,
                column,
                end_line,
                end_column,
            } => {
                self.activate_search(
                    &item.replacement,
                    SearchMatch {
                        line,
                        column,
                        end_line,
                        end_column,
                    },
                );
                self.finish_command_sheet();
                self.cursor = Cursor { line, column };
                self.reset_preferred_column();
                self.set_status(format!("Found on line {}", line + 1));
                Ok(())
            }
            SheetAction::BeginSearch => {
                self.sheet.prompt = CommandPrompt::Search;
                self.command_line.clear();
                self.clear_search();
                self.sheet.selected = 0;
                self.sheet.explicit_selection = false;
                self.refresh_sheet_items();
                Ok(())
            }
        }
    }

    fn execute_search_query(&mut self, query: &str, selected: Option<SheetItem>) -> Result<()> {
        if query.is_empty() {
            self.clear_search();
            self.finish_command_sheet();
            self.set_status("Search needs text");
            return Ok(());
        }

        if let Some(item) = selected.filter(|item| matches!(item.kind, SheetItemKind::Search)) {
            return self.execute_sheet_item(item);
        }

        if let Some(search_match) = first_search_match(&self.buffer, query) {
            self.activate_search(query, search_match);
            self.finish_command_sheet();
            self.cursor = Cursor {
                line: search_match.line,
                column: search_match.column,
            };
            self.reset_preferred_column();
            self.set_status(format!("Found on line {}", search_match.line + 1));
        } else {
            self.clear_search();
            self.finish_command_sheet();
            self.set_status(format!("No match: {query}"));
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
                self.set_status(format!("Unknown command: {value}"));
            }
        }

        Ok(())
    }

    fn clear_search(&mut self) {
        self.search = SearchState::default();
    }

    fn preview_search(&mut self, query: &str) {
        let query = query.trim();
        if query.is_empty() {
            self.clear_search();
            return;
        }

        self.search.query = query.to_string();
        self.search.matches = search_matches_for_query(&self.buffer, query);
        self.search.selected = 0;
    }

    fn activate_search(&mut self, query: &str, selected_match: SearchMatch) {
        self.search.query = query.trim().to_string();
        self.search.matches = search_matches_for_query(&self.buffer, query);
        if self.search.matches.is_empty() {
            self.search.matches.push(selected_match);
        }
        self.search.selected = self
            .search
            .matches
            .iter()
            .position(|candidate| *candidate == selected_match)
            .unwrap_or_default();
    }

    fn jump_search_match(&mut self, delta: isize) {
        if self.search.query.is_empty() {
            self.set_status("No active search");
            return;
        }

        self.search.matches = search_matches_for_query(&self.buffer, &self.search.query);
        if self.search.matches.is_empty() {
            self.set_status(format!("No match: {}", self.search.query));
            return;
        }

        let len = self.search.matches.len();
        let current = self.search.selected.min(len - 1) as isize;
        self.search.selected = (current + delta).rem_euclid(len as isize) as usize;
        let search_match = self.search.matches[self.search.selected];
        self.cursor = Cursor {
            line: search_match.line,
            column: search_match.column,
        };
        self.reset_preferred_column();
        self.set_status(format!("Found on line {}", search_match.line + 1));
    }

    pub fn search_result_indicator(&self) -> Option<(usize, usize)> {
        if self.search.query.is_empty() || self.search.matches.is_empty() {
            return None;
        }

        Some((
            self.search.selected.min(self.search.matches.len() - 1) + 1,
            self.search.matches.len(),
        ))
    }

    fn save_current_file(&mut self) -> Result<()> {
        self.buffer.save()?;
        self.refresh_file_tree()?;
        self.set_status("Saved");
        Ok(())
    }

    fn request_quit(&mut self, force: bool) -> Result<()> {
        if self.buffer.dirty && !force {
            self.set_status("Unsaved changes; use :wq or :q!");
            return Ok(());
        }

        self.should_quit = true;
        Ok(())
    }

    fn wrap_width(&self) -> usize {
        let gutter = (self.buffer.line_count().to_string().len() + 1) as usize;
        self.viewport.visible_width.saturating_sub(gutter).max(1)
    }

    fn cursor_for_mouse_position(&self, column: u16, row: u16) -> Option<Cursor> {
        if !self.viewport_contains(column, row) {
            return None;
        }

        let local_x = column.saturating_sub(self.viewport.x) as usize;
        let local_y = row.saturating_sub(self.viewport.y) as usize;
        self.cursor_for_viewport_position(local_x, local_y)
    }

    fn viewport_contains(&self, column: u16, row: u16) -> bool {
        if column < self.viewport.x || row < self.viewport.y {
            return false;
        }

        let local_x = column.saturating_sub(self.viewport.x) as usize;
        let local_y = row.saturating_sub(self.viewport.y) as usize;
        local_x < self.viewport.visible_width && local_y < self.viewport.visible_height
    }

    fn cursor_for_viewport_position(&self, local_x: usize, local_y: usize) -> Option<Cursor> {
        let gutter = (self.buffer.line_count().to_string().len() + 1) as usize;
        let text_x = local_x.saturating_sub(gutter);
        let width = self.wrap_width();
        let table_layout = TableLayout::new(&self.buffer);
        let rows = visible_rows(
            &self.buffer,
            self.viewport.top_line,
            self.viewport.top_wrap_index,
            self.viewport.visible_height,
            width,
            |line_num, text, width| {
                if line_num == self.cursor.line {
                    wrap_line(text, width)
                } else if table_layout.is_table_row(line_num) {
                    table_wrap_line(text, width)
                } else {
                    concealed_wrap_line(text, width)
                }
            },
        );

        let Some(row) = rows.get(local_y) else {
            let line = self.buffer.line_count().saturating_sub(1);
            return Some(Cursor {
                line,
                column: self.buffer.line_len_chars(line),
            });
        };

        let text_x = text_x.saturating_sub(row.continuation_indent);
        let segment_len = row.source_end.saturating_sub(row.source_start);
        let column = row.source_start + text_x.min(segment_len);
        Some(Cursor {
            line: row.line_number,
            column: column.min(self.buffer.line_len_chars(row.line_number)),
        })
    }

    fn scroll_visual_rows(&mut self, delta: isize) {
        if delta == 0 {
            return;
        }

        let width = self.wrap_width();
        self.normalize_viewport(width);
        let steps = delta.unsigned_abs();

        for _ in 0..steps {
            let next = if delta > 0 {
                self.next_visual_position(
                    self.viewport.top_line,
                    self.viewport.top_wrap_index,
                    width,
                )
            } else {
                self.previous_visual_position(
                    self.viewport.top_line,
                    self.viewport.top_wrap_index,
                    width,
                )
            };

            if next == (self.viewport.top_line, self.viewport.top_wrap_index) {
                break;
            }
            (self.viewport.top_line, self.viewport.top_wrap_index) = next;
        }

        self.normalize_viewport(width);
        self.keep_cursor_in_scrolled_viewport(width);
        self.cancel_pending_operators();
        self.reset_preferred_column();
    }

    fn keep_cursor_in_scrolled_viewport(&mut self, width: usize) {
        let cursor_wrap = wrap_index_for_column(
            &self.buffer.line(self.cursor.line),
            self.cursor.column,
            width,
        );
        let preferred_column = self.current_visual_column(width);

        if visual_position_before(
            self.cursor.line,
            cursor_wrap,
            self.viewport.top_line,
            self.viewport.top_wrap_index,
        ) {
            self.cursor = self.cursor_for_visual_position(
                self.viewport.top_line,
                self.viewport.top_wrap_index,
                width,
                preferred_column,
            );
            return;
        }

        let offset = self
            .visual_offset_from_viewport(self.cursor.line, cursor_wrap, width)
            .unwrap_or(usize::MAX);
        if offset >= self.viewport.visible_height {
            let (line, wrap_index) =
                self.visual_position_at_viewport_offset(self.viewport.visible_height - 1, width);
            self.cursor =
                self.cursor_for_visual_position(line, wrap_index, width, preferred_column);
        }
    }

    fn current_visual_column(&self, width: usize) -> usize {
        let line_text = self.buffer.line(self.cursor.line);
        let (segment_start, _) = visual_line_bounds(&line_text, self.cursor.column, width);
        self.cursor.column.saturating_sub(segment_start)
    }

    fn visual_position_at_viewport_offset(&self, offset: usize, width: usize) -> (usize, usize) {
        let mut line = self.viewport.top_line;
        let mut wrap = self.viewport.top_wrap_index;
        for _ in 0..offset {
            let next = self.next_visual_position(line, wrap, width);
            if next == (line, wrap) {
                break;
            }
            (line, wrap) = next;
        }
        (line, wrap)
    }

    fn cursor_for_visual_position(
        &self,
        line: usize,
        wrap_index: usize,
        width: usize,
        preferred_column: usize,
    ) -> Cursor {
        let line = line.min(self.buffer.line_count().saturating_sub(1));
        let line_text = self.buffer.line(line);
        let trimmed = line_text.trim_end_matches(['\r', '\n']);
        let (segments, _) = wrap_line(trimmed, width);
        let segment_index = wrap_index.min(segments.len().saturating_sub(1));
        let Some(&(start, end)) = segments.get(segment_index) else {
            return Cursor { line, column: 0 };
        };

        let max_col = visual_segment_max_column(
            &segments,
            segment_index,
            self.buffer.line_len_chars(line),
            end,
        );
        Cursor {
            line,
            column: (start + preferred_column).min(max_col),
        }
    }

    fn visual_line_start(&mut self) {
        let line_text = self.buffer.line(self.cursor.line);
        let width = self.wrap_width();
        let (seg_start, _) = visual_line_bounds(&line_text, self.cursor.column, width);
        self.cursor.column = seg_start;
    }

    fn visual_line_end(&mut self) {
        let line_text = self.buffer.line(self.cursor.line);
        let width = self.wrap_width();
        let (_, seg_end) = visual_line_bounds(&line_text, self.cursor.column, width);
        self.cursor.column = seg_end.min(self.buffer.line_len_chars(self.cursor.line));
    }

    fn handle_navigation_modifier(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::SUPER) {
            match key.code {
                KeyCode::Left => {
                    motions::line_start(&mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Right => {
                    motions::line_end(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Up => {
                    self.document_start_preserving_column();
                    true
                }
                KeyCode::Down => {
                    self.document_end_preserving_column();
                    true
                }
                KeyCode::Home => {
                    self.document_start_preserving_column();
                    true
                }
                KeyCode::End => {
                    self.document_end_preserving_column();
                    true
                }
                _ => false,
            }
        } else if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => {
                    motions::word_backward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Right => {
                    motions::word_forward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    motions::word_backward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Char('f') | KeyCode::Char('F') => {
                    motions::word_forward(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                _ => false,
            }
        } else if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('a') | KeyCode::Char('A') => {
                    motions::line_start(&mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Char('e') | KeyCode::Char('E') => {
                    motions::line_end(&self.buffer, &mut self.cursor);
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Char('u') | KeyCode::Char('U') => {
                    self.delete_to_line_start();
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Char('k') | KeyCode::Char('K') => {
                    self.delete_to_line_end();
                    self.reset_preferred_column();
                    true
                }
                KeyCode::Home => {
                    self.document_start_preserving_column();
                    true
                }
                KeyCode::End => {
                    self.document_end_preserving_column();
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    fn preferred_column(&mut self) -> usize {
        match self.preferred_column {
            Some(column) => column,
            None => {
                self.preferred_column = Some(self.cursor.column);
                self.cursor.column
            }
        }
    }

    fn reset_preferred_column(&mut self) {
        self.preferred_column = None;
        self.preferred_visual_column = None;
    }

    fn move_to_line_preserving_column(&mut self, line: usize) {
        let column = self.preferred_column();
        self.cursor.line = line.min(self.buffer.line_count().saturating_sub(1));
        self.cursor.column = column.min(self.buffer.line_len_chars(self.cursor.line));
    }

    fn move_line_down_preserving_column(&mut self) {
        let target = (self.cursor.line + 1).min(self.buffer.line_count().saturating_sub(1));
        self.move_to_line_preserving_column(target);
    }

    fn move_line_up_preserving_column(&mut self) {
        let target = self.cursor.line.saturating_sub(1);
        self.move_to_line_preserving_column(target);
    }

    fn document_start_preserving_column(&mut self) {
        self.move_to_line_preserving_column(0);
    }

    fn document_end_preserving_column(&mut self) {
        self.move_to_line_preserving_column(self.buffer.line_count().saturating_sub(1));
    }

    fn preferred_visual_column(&mut self, segment_start: usize) -> usize {
        match self.preferred_visual_column {
            Some(column) => column,
            None => {
                let column = self.cursor.column.saturating_sub(segment_start);
                self.preferred_visual_column = Some(column);
                column
            }
        }
    }

    fn move_to_visual_segment_preserving_column(
        &mut self,
        line: usize,
        segment_index: usize,
        width: usize,
    ) {
        let line = line.min(self.buffer.line_count().saturating_sub(1));
        let line_text = self.buffer.line(line);
        let (segments, _) = wrap_line(line_text.trim_end_matches(['\r', '\n']), width);
        let Some(&(start, end)) = segments.get(segment_index) else {
            self.cursor.line = line;
            self.cursor.column = self.buffer.line_len_chars(line);
            return;
        };

        let column = self.preferred_visual_column(start);
        let max_col = visual_segment_max_column(
            &segments,
            segment_index,
            self.buffer.line_len_chars(line),
            end,
        );
        self.cursor.line = line;
        self.cursor.column = (start + column).min(max_col);
    }

    fn visual_line_down(&mut self) {
        let line_text = self.buffer.line(self.cursor.line);
        let width = self.wrap_width();
        let (segments, _) = wrap_line(line_text.trim_end_matches(['\r', '\n']), width);
        let current_seg = wrap_index_for_column(&line_text, self.cursor.column, width);
        let segment_start = segments
            .get(current_seg)
            .map(|(start, _)| *start)
            .unwrap_or_default();
        let rel = self.preferred_visual_column(segment_start);
        if current_seg + 1 < segments.len() {
            let (next_start, next_end) = segments[current_seg + 1];
            let max_col = visual_segment_max_column(
                &segments,
                current_seg + 1,
                self.buffer.line_len_chars(self.cursor.line),
                next_end,
            );
            self.cursor.column = if next_start + rel > max_col {
                max_col
            } else {
                next_start + rel
            };
        } else if self.cursor.line + 1 < self.buffer.line_count() {
            self.move_to_visual_segment_preserving_column(self.cursor.line + 1, 0, width);
        }
    }

    fn visual_line_up(&mut self) {
        let line_text = self.buffer.line(self.cursor.line);
        let width = self.wrap_width();
        let (segments, _) = wrap_line(line_text.trim_end_matches(['\r', '\n']), width);
        let current_seg = wrap_index_for_column(&line_text, self.cursor.column, width);
        let segment_start = segments
            .get(current_seg)
            .map(|(start, _)| *start)
            .unwrap_or_default();
        let rel = self.preferred_visual_column(segment_start);
        if current_seg > 0 {
            let (prev_start, prev_end) = segments[current_seg - 1];
            let max_col = visual_segment_max_column(
                &segments,
                current_seg - 1,
                self.buffer.line_len_chars(self.cursor.line),
                prev_end,
            );
            self.cursor.column = if prev_start + rel > max_col {
                max_col
            } else {
                prev_start + rel
            };
        } else if self.cursor.line > 0 {
            let previous_line = self.cursor.line - 1;
            let previous_text = self.buffer.line(previous_line);
            let (previous_segments, _) =
                wrap_line(previous_text.trim_end_matches(['\r', '\n']), width);
            self.move_to_visual_segment_preserving_column(
                previous_line,
                previous_segments.len().saturating_sub(1),
                width,
            );
        }
    }

    fn open_path(&mut self, path: &Path) -> Result<()> {
        if self.buffer.dirty {
            self.set_status("Unsaved changes; save before switching files");
            return Ok(());
        }

        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.notes_dir.join(path)
        };

        self.buffer = DocumentBuffer::from_path_or_empty(&path)?;
        self.cursor = Cursor::default();
        self.reset_preferred_column();
        self.viewport.top_line = 0;
        self.viewport.top_wrap_index = 0;
        self.viewport.horizontal_offset = 0;
        self.set_status(format!("Opened {}", path.display()));
        Ok(())
    }

    fn refresh_file_tree(&mut self) -> Result<()> {
        self.file_tree = FileTree::load(&self.notes_dir).with_context(|| {
            format!(
                "failed to scan notes directory: {}",
                self.notes_dir.display()
            )
        })?;
        self.refresh_sheet_items();
        Ok(())
    }

    fn refresh_sheet_items(&mut self) {
        let input = self.command_line.trim().to_string();
        let mut items = if matches!(self.sheet.prompt, CommandPrompt::Search) {
            self.preview_search(&input);
            search_sheet_items(&self.buffer, &input)
        } else if let Some(search_query) = input.strip_prefix('/') {
            let query = search_query.trim();
            self.preview_search(query);
            search_sheet_items(&self.buffer, query)
        } else {
            self.clear_search();
            command_sheet_items(&input, &self.notes_dir, &self.file_tree)
        };

        items.truncate(128);
        self.sheet.items = items.into_iter().map(|(_, item)| item).collect();
        if self.sheet.selected >= self.sheet.items.len() {
            self.sheet.selected = 0;
            self.sheet.explicit_selection = false;
        }
    }

    pub fn sheet_panel_height(&self, max_height: u16) -> u16 {
        let content_rows = self.sheet.items.len() as u16;
        let desired = content_rows.min(9);
        desired.min(max_height.saturating_sub(1))
    }

    fn follow_link_under_cursor(&mut self) -> Result<()> {
        let line = self.buffer.line(self.cursor.line);
        let source = line.trim_end_matches(['\r', '\n']);
        let Some(link) = link_at_column(source, self.cursor.column) else {
            self.set_status("No link under cursor");
            return Ok(());
        };

        let target = link.target.trim();
        if target.is_empty() {
            self.set_status("No link under cursor");
            return Ok(());
        }

        if is_external_link(target) {
            let url = normalized_external_url(target);
            open_external_url(&url)?;
            self.set_status(format!("Opened {url}"));
            return Ok(());
        }

        let path = self.resolve_link_path(target, link.kind);
        self.open_path(&path)
    }

    fn resolve_link_path(&self, target: &str, kind: LinkKind) -> PathBuf {
        let target = target
            .split_once('#')
            .map(|(path, _)| path)
            .unwrap_or(target);
        let mut path = PathBuf::from(target);
        if matches!(kind, LinkKind::Wiki) && path.extension().is_none() {
            path.set_extension("md");
        }

        if path.is_absolute() {
            return path;
        }

        let base_dir = self
            .buffer
            .path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(&self.notes_dir);
        base_dir.join(path)
    }

    fn keep_cursor_visible(&mut self) {
        self.buffer.clamp_cursor(&mut self.cursor);
        let width = self.wrap_width();
        self.normalize_viewport(width);

        let cursor_wrap = wrap_index_for_column(
            &self.buffer.line(self.cursor.line),
            self.cursor.column,
            width,
        );
        if visual_position_before(
            self.cursor.line,
            cursor_wrap,
            self.viewport.top_line,
            self.viewport.top_wrap_index,
        ) {
            self.viewport.top_line = self.cursor.line;
            self.viewport.top_wrap_index = cursor_wrap;
            return;
        }

        let offset = self
            .visual_offset_from_viewport(self.cursor.line, cursor_wrap, width)
            .unwrap_or(usize::MAX);
        if offset >= self.viewport.visible_height {
            let (line, wrap_index) = self.visual_position_for_cursor_bottom(cursor_wrap, width);
            self.viewport.top_line = line;
            self.viewport.top_wrap_index = wrap_index;
        }

        self.normalize_viewport(width);
    }

    fn normalize_viewport(&mut self, width: usize) {
        self.viewport.top_line = self
            .viewport
            .top_line
            .min(self.buffer.line_count().saturating_sub(1));
        let wraps = self.line_wrap_count(self.viewport.top_line, width);
        self.viewport.top_wrap_index = self.viewport.top_wrap_index.min(wraps.saturating_sub(1));
    }

    fn visual_offset_from_viewport(
        &self,
        target_line: usize,
        target_wrap: usize,
        width: usize,
    ) -> Option<usize> {
        let mut line = self.viewport.top_line;
        let mut wrap = self.viewport.top_wrap_index;
        let mut offset = 0;

        loop {
            if line == target_line && wrap == target_wrap {
                return Some(offset);
            }
            if !visual_position_before(line, wrap, target_line, target_wrap) {
                return None;
            }

            let next = self.next_visual_position(line, wrap, width);
            if next == (line, wrap) {
                return None;
            }
            (line, wrap) = next;
            offset += 1;
        }
    }

    fn visual_position_for_cursor_bottom(
        &self,
        cursor_wrap: usize,
        width: usize,
    ) -> (usize, usize) {
        let mut line = self.cursor.line;
        let mut wrap = cursor_wrap;
        for _ in 1..self.viewport.visible_height {
            let previous = self.previous_visual_position(line, wrap, width);
            if previous == (line, wrap) {
                break;
            }
            (line, wrap) = previous;
        }

        (line, wrap)
    }

    fn next_visual_position(&self, line: usize, wrap: usize, width: usize) -> (usize, usize) {
        let wrap_count = self.line_wrap_count(line, width);
        if wrap + 1 < wrap_count {
            return (line, wrap + 1);
        }

        if line + 1 < self.buffer.line_count() {
            return (line + 1, 0);
        }

        (line, wrap)
    }

    fn previous_visual_position(&self, line: usize, wrap: usize, width: usize) -> (usize, usize) {
        if wrap > 0 {
            return (line, wrap - 1);
        }

        if line > 0 {
            let previous_line = line - 1;
            return (
                previous_line,
                self.line_wrap_count(previous_line, width).saturating_sub(1),
            );
        }

        (line, wrap)
    }

    fn line_wrap_count(&self, line: usize, width: usize) -> usize {
        let line_text = self.buffer.line(line);
        let trimmed = line_text.trim_end_matches(['\r', '\n']);
        let table_layout = TableLayout::new(&self.buffer);
        let (segments, _) = if line == self.cursor.line {
            wrap_line(trimmed, width)
        } else if table_layout.is_table_row(line) {
            table_wrap_line(trimmed, width)
        } else {
            concealed_wrap_line(trimmed, width)
        };
        segments.len().max(1)
    }

    fn toggle_checkbox(&mut self) -> bool {
        let original_cursor = self.cursor;
        let line = self.buffer.line(original_cursor.line);
        let trimmed = line.trim_end_matches(['\r', '\n']);
        let leading_ws_len = trimmed.len() - trimmed.trim_start().len();
        let content = &trimmed[leading_ws_len..];

        let col = leading_ws_len + 3;

        if content.starts_with("- [ ] ") || content.starts_with("- [x] ") {
            let unchecked = content.starts_with("- [ ] ");
            let start = self.buffer.char_index(Cursor {
                line: original_cursor.line,
                column: col,
            });
            let replacement = if unchecked { "x" } else { " " };
            self.buffer
                .replace_range(start, start + 1, replacement, &mut self.cursor);
            self.cursor = original_cursor;
            return true;
        }

        false
    }
}

fn visual_position_before(
    line: usize,
    wrap_index: usize,
    other_line: usize,
    other_wrap_index: usize,
) -> bool {
    line < other_line || (line == other_line && wrap_index < other_wrap_index)
}

fn cursor_before_or_equal(left: Cursor, right: Cursor) -> bool {
    left.line < right.line || (left.line == right.line && left.column <= right.column)
}

fn visual_segment_max_column(
    segments: &[(usize, usize)],
    segment_index: usize,
    line_len: usize,
    segment_end: usize,
) -> usize {
    if segments.len() == 1 && segment_index == 0 {
        line_len
    } else {
        segment_end.saturating_sub(1).min(line_len)
    }
}

fn is_text_input_key(key: KeyEvent) -> bool {
    !key.modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
}

fn is_command_sheet_shortcut(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
        && key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER)
}

#[derive(Debug, Clone, Copy)]
struct CommandCandidate {
    replacement: &'static str,
    label: &'static str,
    detail: &'static str,
    aliases: &'static [&'static str],
    action: CommandCandidateAction,
}

#[derive(Debug, Clone, Copy)]
enum CommandCandidateAction {
    Command(&'static str),
    Complete(&'static str),
    BeginSearch,
}

const COMMAND_CANDIDATES: &[CommandCandidate] = &[
    CommandCandidate {
        replacement: "w",
        label: "write",
        detail: "Save current file",
        aliases: &["w", "write", "save"],
        action: CommandCandidateAction::Command("w"),
    },
    CommandCandidate {
        replacement: "q",
        label: "quit",
        detail: "Quit if there are no unsaved changes",
        aliases: &["q", "quit", "close"],
        action: CommandCandidateAction::Command("q"),
    },
    CommandCandidate {
        replacement: "q!",
        label: "quit!",
        detail: "Quit and discard unsaved changes",
        aliases: &["q!", "quit!", "force quit"],
        action: CommandCandidateAction::Command("q!"),
    },
    CommandCandidate {
        replacement: "wq",
        label: "write quit",
        detail: "Save current file and quit",
        aliases: &["wq", "x", "write quit", "save quit"],
        action: CommandCandidateAction::Command("wq"),
    },
    CommandCandidate {
        replacement: "e ",
        label: "edit",
        detail: "Open a file path",
        aliases: &["e", "edit", "open", "file"],
        action: CommandCandidateAction::Complete("e "),
    },
    CommandCandidate {
        replacement: "/",
        label: "search",
        detail: "Find text in the current document",
        aliases: &["/", "search", "find"],
        action: CommandCandidateAction::BeginSearch,
    },
];

fn command_sheet_items(
    input: &str,
    notes_dir: &Path,
    file_tree: &FileTree,
) -> Vec<(usize, SheetItem)> {
    let mut items = Vec::new();

    for candidate in COMMAND_CANDIDATES {
        if let Some(score) = score_command_candidate(input, candidate) {
            items.push((1_000 + score, command_sheet_item(candidate)));
        }
    }

    let file_query = file_query_for_command_input(input);
    for entry in file_tree.entries.iter().filter(|entry| !entry.is_dir) {
        if let Some(score) = score_file_entry(file_query, notes_dir, entry) {
            let relative = relative_path_label(notes_dir, &entry.path);
            items.push((
                score,
                SheetItem {
                    kind: SheetItemKind::File,
                    label: entry.display_name.clone(),
                    detail: relative.clone(),
                    replacement: if input.starts_with("e ") || input.starts_with("edit ") {
                        format!("e {relative}")
                    } else {
                        relative
                    },
                    action: SheetAction::File(entry.path.clone()),
                },
            ));
        }
    }

    items.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.label.cmp(&right.1.label))
            .then_with(|| left.1.detail.cmp(&right.1.detail))
    });
    items
}

fn command_sheet_item(candidate: &CommandCandidate) -> SheetItem {
    let action = match candidate.action {
        CommandCandidateAction::Command(command) => SheetAction::Command(command.to_string()),
        CommandCandidateAction::Complete(value) => SheetAction::Complete(value.to_string()),
        CommandCandidateAction::BeginSearch => SheetAction::BeginSearch,
    };

    SheetItem {
        kind: SheetItemKind::Command,
        label: candidate.label.to_string(),
        detail: candidate.detail.to_string(),
        replacement: candidate.replacement.to_string(),
        action,
    }
}

fn score_command_candidate(input: &str, candidate: &CommandCandidate) -> Option<usize> {
    let input = input.trim();
    if input.is_empty() {
        return Some(0);
    }

    candidate
        .aliases
        .iter()
        .filter_map(|alias| score_fuzzy_text(input, alias))
        .min()
}

fn file_query_for_command_input(input: &str) -> &str {
    let input = input.trim();
    if let Some((command, rest)) = input.split_once(' ') {
        if matches!(command, "e" | "edit") {
            return rest.trim();
        }
    }
    input
}

fn resolve_command_path_input(input: &str, notes_dir: &Path) -> Option<PathBuf> {
    let input = input.trim();
    if input.is_empty() || input.contains(' ') {
        return None;
    }
    if !(input.ends_with(".md") || input.contains('/') || input.contains('\\')) {
        return None;
    }

    Some(if Path::new(input).is_absolute() {
        PathBuf::from(input)
    } else {
        notes_dir.join(input)
    })
}

fn search_sheet_items(buffer: &DocumentBuffer, query: &str) -> Vec<(usize, SheetItem)> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let exact_matches = exact_search_matches(buffer, query);
    if !exact_matches.is_empty() {
        return exact_matches
            .into_iter()
            .enumerate()
            .map(|(index, search_match)| {
                (
                    index,
                    SheetItem {
                        kind: SheetItemKind::Search,
                        label: format!("Line {}", search_match.line + 1),
                        detail: search_match_preview(buffer, search_match),
                        replacement: query.to_string(),
                        action: SheetAction::Search {
                            line: search_match.line,
                            column: search_match.column,
                            end_line: search_match.end_line,
                            end_column: search_match.end_column,
                        },
                    },
                )
            })
            .collect();
    }

    let mut items = Vec::new();
    for line in 0..buffer.line_count() {
        let raw = buffer.line(line);
        let preview = raw.trim_end_matches(['\r', '\n']).trim().to_string();
        if preview.is_empty() {
            continue;
        }

        if let Some(score) = score_fuzzy_text(query, &preview) {
            let column = search_match_column(query, &raw).unwrap_or_default();
            let search_match = line_search_match(buffer, line, column, query);
            items.push((
                score + line,
                SheetItem {
                    kind: SheetItemKind::Search,
                    label: format!("Line {}", line + 1),
                    detail: preview,
                    replacement: query.to_string(),
                    action: SheetAction::Search {
                        line,
                        column,
                        end_line: search_match.end_line,
                        end_column: search_match.end_column,
                    },
                },
            ));
        }
    }

    items.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.label.cmp(&right.1.label))
    });
    items
}

fn first_search_match(buffer: &DocumentBuffer, query: &str) -> Option<SearchMatch> {
    if let Some(search_match) = search_matches_for_query(buffer, query).into_iter().next() {
        return Some(search_match);
    }

    search_sheet_items(buffer, query)
        .into_iter()
        .find_map(|(_, item)| match item.action {
            SheetAction::Search {
                line,
                column,
                end_line,
                end_column,
            } => Some(SearchMatch {
                line,
                column,
                end_line,
                end_column,
            }),
            _ => None,
        })
}

fn search_matches_for_query(buffer: &DocumentBuffer, query: &str) -> Vec<SearchMatch> {
    let exact_matches = exact_search_matches(buffer, query);
    if !exact_matches.is_empty() {
        return exact_matches;
    }

    search_sheet_items(buffer, query)
        .into_iter()
        .filter_map(|(_, item)| match item.action {
            SheetAction::Search {
                line,
                column,
                end_line,
                end_column,
            } => Some(SearchMatch {
                line,
                column,
                end_line,
                end_column,
            }),
            _ => None,
        })
        .collect()
}

fn line_search_match(
    buffer: &DocumentBuffer,
    line: usize,
    column: usize,
    query: &str,
) -> SearchMatch {
    let line_len = buffer.line_len_chars(line);
    let end_column = column
        .saturating_add(query.chars().count().max(1))
        .min(line_len)
        .max(column.min(line_len));

    SearchMatch {
        line,
        column,
        end_line: line,
        end_column,
    }
}

fn exact_search_matches(buffer: &DocumentBuffer, query: &str) -> Vec<SearchMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let needle = normalize_search_text(query);
    if needle.is_empty() {
        return Vec::new();
    }

    let source = buffer.as_string();
    let haystack = SearchIndex::new(&source);
    let mut matches = Vec::new();
    let mut byte_offset = 0usize;

    while let Some(relative_start) = haystack.text[byte_offset..].find(&needle) {
        let start_byte = byte_offset + relative_start;
        let end_byte = start_byte + needle.len();
        let normalized_start = haystack.text[..start_byte].chars().count();
        let normalized_end = haystack.text[..end_byte].chars().count();
        if let Some(source_index) = haystack.source_indices.get(normalized_start) {
            let start_cursor = buffer.cursor_from_char_index(*source_index);
            let end_source_index = haystack
                .source_indices
                .get(normalized_end)
                .copied()
                .unwrap_or_else(|| source.chars().count());
            let end_cursor = buffer.cursor_from_char_index(end_source_index);
            matches.push(SearchMatch {
                line: start_cursor.line,
                column: start_cursor.column,
                end_line: end_cursor.line,
                end_column: end_cursor.column,
            });
        }
        byte_offset = end_byte.max(start_byte + 1);
    }

    matches
}

struct SearchIndex {
    text: String,
    source_indices: Vec<usize>,
}

impl SearchIndex {
    fn new(source: &str) -> Self {
        let mut text = String::new();
        let mut source_indices = Vec::new();
        let mut previous_was_whitespace = false;

        for (source_index, ch) in source.chars().enumerate() {
            if ch.is_whitespace() {
                if !previous_was_whitespace {
                    text.push(' ');
                    source_indices.push(source_index);
                    previous_was_whitespace = true;
                }
                continue;
            }

            text.push(ch.to_ascii_lowercase());
            source_indices.push(source_index);
            previous_was_whitespace = false;
        }

        Self {
            text,
            source_indices,
        }
    }
}

fn normalize_search_text(source: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_whitespace = false;

    for ch in source.trim().chars() {
        if ch.is_whitespace() {
            if !previous_was_whitespace {
                normalized.push(' ');
                previous_was_whitespace = true;
            }
            continue;
        }

        normalized.push(ch.to_ascii_lowercase());
        previous_was_whitespace = false;
    }

    normalized
}

fn search_match_preview(buffer: &DocumentBuffer, search_match: SearchMatch) -> String {
    let start = buffer.char_index(Cursor {
        line: search_match.line,
        column: search_match.column,
    });
    let preview = buffer
        .as_string()
        .chars()
        .skip(start)
        .take(96)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    if preview.is_empty() {
        buffer
            .line(search_match.line)
            .trim_end_matches(['\r', '\n'])
            .trim()
            .to_string()
    } else {
        preview
    }
}

fn score_file_entry(
    query: &str,
    notes_dir: &Path,
    entry: &crate::fs::tree::TreeEntry,
) -> Option<usize> {
    if query.trim().is_empty() {
        return Some(0);
    }

    let relative = relative_path_label(notes_dir, &entry.path);

    score_fuzzy_text(query, &entry.display_name)
        .or_else(|| score_fuzzy_text(query, &relative).map(|score| score + 200))
}

fn relative_path_label(notes_dir: &Path, path: &Path) -> String {
    path.strip_prefix(notes_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn score_fuzzy_text(query: &str, candidate: &str) -> Option<usize> {
    let query = normalize_fuzzy_text(query);
    let candidate = normalize_fuzzy_text(candidate);
    score_normalized_fuzzy_text(&query, &candidate)
}

fn score_normalized_fuzzy_text(query: &str, candidate: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }

    if candidate == query {
        return Some(0);
    }

    if candidate.starts_with(query) {
        return Some(1 + candidate.len().saturating_sub(query.len()));
    }

    if let Some(index) = candidate.find(query) {
        return Some(100 + index);
    }

    subsequence_score(query, candidate)
}

fn subsequence_score(query: &str, candidate: &str) -> Option<usize> {
    let mut query_chars = query.chars();
    let mut next_query = query_chars.next()?;
    let mut first_match = None;
    let mut last_match = 0usize;
    let mut gaps = 0usize;

    for (index, ch) in candidate.chars().enumerate() {
        if ch != next_query {
            continue;
        }

        if first_match.is_none() {
            first_match = Some(index);
        } else {
            gaps += index.saturating_sub(last_match + 1);
        }
        last_match = index;

        if let Some(next) = query_chars.next() {
            next_query = next;
        } else {
            return Some(400 + first_match.unwrap_or_default() + gaps);
        }
    }

    None
}

fn search_match_column(query: &str, candidate: &str) -> Option<usize> {
    let query = normalize_fuzzy_text(query);
    let candidate = normalize_fuzzy_text(candidate);
    if let Some(byte_index) = candidate.find(&query) {
        return Some(candidate[..byte_index].chars().count());
    }

    let mut query_chars = query.chars();
    let next_query = query_chars.next()?;
    candidate.chars().position(|ch| ch == next_query)
}

fn normalize_fuzzy_text(text: &str) -> String {
    text.to_lowercase()
}

fn parse_numbered_list(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && text.get(i..i + 2) == Some(". ") {
        Some((&text[..i], &text[i + 2..]))
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListContinuation {
    None,
    EndList { delete_to_column: usize },
    Continue(String),
}

fn insert_newline_with_list_continuation(buffer: &mut DocumentBuffer, cursor: &mut Cursor) {
    let current_line = buffer.line(cursor.line);
    match list_continuation_after_enter(&current_line, cursor.column) {
        ListContinuation::None => buffer.insert_char(cursor, '\n'),
        ListContinuation::Continue(prefix) => {
            buffer.insert_str(cursor, &format!("\n{prefix}"));
        }
        ListContinuation::EndList { delete_to_column } => {
            let line = cursor.line;
            let had_line_ending = current_line.ends_with('\n');
            let start = buffer.char_index(Cursor { line, column: 0 });
            let end = buffer.char_index(Cursor {
                line,
                column: delete_to_column,
            });
            let replacement = if had_line_ending { "" } else { "\n" };
            buffer.replace_range(start, end, replacement, cursor);
            *cursor = Cursor { line, column: 0 };
        }
    }
}

fn list_continuation_after_enter(line: &str, column: usize) -> ListContinuation {
    let line = line.trim_end_matches(['\r', '\n']);
    let leading_ws_len = line.len() - line.trim_start().len();
    if column < leading_ws_len {
        return ListContinuation::None;
    }

    let leading_ws = &line[..leading_ws_len];
    let content = &line[leading_ws_len..];
    let Some(item) = parse_list_item(content) else {
        return ListContinuation::None;
    };

    if column < leading_ws_len + item.marker_len {
        return ListContinuation::None;
    }

    if item.content.is_empty() {
        return ListContinuation::EndList {
            delete_to_column: leading_ws_len + item.marker_len,
        };
    }

    let prefix = match item.kind {
        ListItemKind::Checkbox => format!("{leading_ws}- [ ] "),
        ListItemKind::Bullet(marker) => format!("{leading_ws}{marker} "),
        ListItemKind::Numbered(number) => format!("{leading_ws}{}. ", number + 1),
    };
    ListContinuation::Continue(prefix)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListItemKind {
    Checkbox,
    Bullet(char),
    Numbered(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ListItem<'a> {
    kind: ListItemKind,
    marker_len: usize,
    content: &'a str,
}

fn parse_list_item(content: &str) -> Option<ListItem<'_>> {
    if let Some(rest) = content
        .strip_prefix("- [ ]")
        .or_else(|| content.strip_prefix("- [x]"))
    {
        if let Some(item_content) = rest
            .strip_prefix(' ')
            .or_else(|| rest.is_empty().then_some(""))
        {
            return Some(ListItem {
                kind: ListItemKind::Checkbox,
                marker_len: content.len() - item_content.len(),
                content: item_content,
            });
        }
    }

    if let Some((number, item_content)) = parse_numbered_list(content) {
        return Some(ListItem {
            kind: ListItemKind::Numbered(number.parse().unwrap_or(1)),
            marker_len: content.len() - item_content.len(),
            content: item_content,
        });
    }

    for marker in ['-', '*', '+'] {
        let prefix = [marker, ' '].iter().collect::<String>();
        if let Some(item_content) = content.strip_prefix(&prefix) {
            return Some(ListItem {
                kind: ListItemKind::Bullet(marker),
                marker_len: prefix.len(),
                content: item_content,
            });
        }
    }

    None
}

fn is_external_link(target: &str) -> bool {
    target.starts_with("https://") || target.starts_with("http://") || target.starts_with("www.")
}

fn normalized_external_url(target: &str) -> String {
    if target.starts_with("www.") {
        format!("https://{target}")
    } else {
        target.to_string()
    }
}

fn open_external_url(url: &str) -> Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        let mut command = ProcessCommand::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = ProcessCommand::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .with_context(|| format!("failed to open URL: {url}"))?;
    Ok(())
}

#[cfg(test)]
fn copy_to_clipboard(_text: &str) -> Result<()> {
    Ok(())
}

#[cfg(all(not(test), target_os = "macos"))]
fn copy_to_clipboard(text: &str) -> Result<()> {
    copy_with_command("pbcopy", &[], text)
}

#[cfg(all(not(test), target_os = "windows"))]
fn copy_to_clipboard(text: &str) -> Result<()> {
    copy_with_command("clip", &[], text)
}

#[cfg(all(not(test), unix, not(target_os = "macos")))]
fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut last_error = None;
    for (program, args) in [
        ("wl-copy", &[][..]),
        ("xclip", &["-selection", "clipboard"][..]),
        ("xsel", &["--clipboard", "--input"][..]),
    ] {
        match copy_with_command(program, args, text) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("no clipboard command configured")))
}

#[cfg(all(not(test), not(any(unix, target_os = "windows"))))]
fn copy_to_clipboard(_text: &str) -> Result<()> {
    anyhow::bail!("clipboard copy is not supported on this platform")
}

#[cfg(not(test))]
fn copy_with_command(program: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = ProcessCommand::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start clipboard command `{program}`"))?;

    let mut stdin = child
        .stdin
        .take()
        .with_context(|| format!("failed to open stdin for `{program}`"))?;
    stdin
        .write_all(text.as_bytes())
        .with_context(|| format!("failed to write selection to `{program}`"))?;
    drop(stdin);

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for `{program}`"))?;
    if !status.success() {
        anyhow::bail!("clipboard command `{program}` exited with {status}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::tree::{FileTree, TreeEntry};

    fn test_app(text: &str) -> App {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, text);
        buffer.dirty = false;

        App {
            notes_dir: PathBuf::new(),
            file_tree: FileTree {
                entries: Vec::new(),
                selected: 0,
            },
            buffer,
            cursor: Cursor::default(),
            viewport: Viewport::default(),
            mode: Mode::Normal,
            theme: Theme::monochrome_for_tests(),
            command_line: String::new(),
            sheet: CommandSheetState::default(),
            search: SearchState::default(),
            text_selection: None,
            status_message: String::new(),
            status_message_expires_at: None,
            should_quit: false,
            visual_line_anchor: None,
            preferred_column: None,
            preferred_visual_column: None,
            pending_g: false,
            pending_delete: false,
            pending_change: false,
            mouse_anchor: None,
            last_copied_selection: None,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_event(Event::Key(key(code))).unwrap();
    }

    fn press_modified(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
        app.handle_event(Event::Key(modified_key(code, modifiers)))
            .unwrap();
    }

    fn click(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    fn super_click(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::SUPER,
        }))
        .unwrap();
    }

    fn drag(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    fn release(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    fn scroll_down(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    fn scroll_up(app: &mut App, column: u16, row: u16) {
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }))
        .unwrap();
    }

    #[test]
    fn visual_mode_can_enter_command_line_and_quit() {
        let mut app = test_app("text");
        app.mode = Mode::Visual;
        app.visual_line_anchor = Some(0);

        press(&mut app, KeyCode::Char(':'));
        assert_eq!(app.mode, Mode::CommandLine);
        assert_eq!(app.visual_line_anchor, None);

        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Enter);

        assert!(app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn normal_mode_a_enters_insert_at_line_end() {
        let mut app = test_app("abc");
        app.cursor = Cursor { line: 0, column: 1 };

        press(&mut app, KeyCode::Char('A'));

        assert_eq!(app.mode, Mode::Insert);
        assert_eq!(app.cursor, Cursor { line: 0, column: 3 });
    }

    #[test]
    fn dd_preserves_cursor_column_when_next_line_is_long_enough() {
        let mut app = test_app("abcd\nwxyz");
        app.cursor = Cursor { line: 0, column: 2 };

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('d'));

        assert_eq!(app.buffer.as_string(), "wxyz");
        assert_eq!(app.cursor, Cursor { line: 0, column: 2 });
    }

    #[test]
    fn j_scrolls_viewport_through_wrapped_rows() {
        let mut app = test_app("abcdefghijklmnopqrstuvwxyz");
        app.resize_viewport(2, 10);

        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));

        assert_eq!(app.viewport.top_line, 0);
        assert!(app.viewport.top_wrap_index > 0);
    }

    #[test]
    fn mouse_click_moves_cursor_to_visible_position() {
        let mut app = test_app("alpha\nbravo charlie\nomega");
        app.resize_viewport(5, 20);
        app.move_viewport_to(2, 1);

        click(&mut app, 7, 2);

        assert_eq!(app.cursor, Cursor { line: 1, column: 3 });
    }

    #[test]
    fn mouse_click_maps_wrapped_rows_to_source_columns() {
        let mut app = test_app("abcdefghij");
        app.resize_viewport(5, 8);

        click(&mut app, 4, 1);

        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });
    }

    #[test]
    fn mouse_wheel_scrolls_view_without_moving_visible_cursor() {
        let mut app = test_app("l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
        app.resize_viewport(6, 20);
        app.viewport.top_line = 0;
        app.cursor = Cursor { line: 4, column: 1 };

        scroll_down(&mut app, 0, 0);

        assert_eq!(app.viewport.top_line, 3);
        assert_eq!(app.cursor, Cursor { line: 4, column: 1 });
    }

    #[test]
    fn mouse_wheel_moves_cursor_when_scroll_hides_it() {
        let mut app = test_app("l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
        app.resize_viewport(4, 20);
        app.viewport.top_line = 0;
        app.cursor = Cursor { line: 0, column: 1 };

        scroll_down(&mut app, 0, 0);

        assert_eq!(app.viewport.top_line, 3);
        assert_eq!(app.cursor, Cursor { line: 3, column: 1 });
    }

    #[test]
    fn mouse_wheel_scrolls_wrapped_visual_rows() {
        let mut app = test_app("abcdefghij");
        app.resize_viewport(2, 8);
        app.cursor = Cursor { line: 0, column: 8 };

        scroll_down(&mut app, 0, 0);

        assert_eq!(app.viewport.top_line, 0);
        assert_eq!(app.viewport.top_wrap_index, 1);
        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });
    }

    #[test]
    fn mouse_wheel_scrolls_up() {
        let mut app = test_app("l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8");
        app.resize_viewport(6, 20);
        app.viewport.top_line = 5;
        app.cursor = Cursor { line: 6, column: 1 };

        scroll_up(&mut app, 0, 0);

        assert_eq!(app.viewport.top_line, 2);
        assert_eq!(app.cursor, Cursor { line: 6, column: 1 });
    }

    #[test]
    fn super_click_opens_link_under_mouse() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("glass-super-click-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let linked = dir.join("next.md");
        std::fs::write(&linked, "opened")?;

        let mut app = test_app("[Next](next.md)");
        app.notes_dir = dir.clone();
        app.resize_viewport(5, 30);

        super_click(&mut app, 3, 0);

        assert_eq!(app.buffer.path.as_deref(), Some(linked.as_path()));
        assert_eq!(app.buffer.as_string(), "opened");
        assert!(app.status_message.starts_with("Opened "));
        assert!(app.status_message_expires_at.is_some());

        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    #[test]
    fn mouse_drag_selects_text_and_copies_immediately() {
        let mut app = test_app("bravo");
        app.resize_viewport(5, 20);

        click(&mut app, 3, 0);
        drag(&mut app, 6, 0);

        assert_eq!(app.cursor, Cursor { line: 0, column: 4 });
        assert_eq!(app.selected_text().as_deref(), Some("rav"));
        assert_eq!(app.status_message, "Copied selection");
        assert_eq!(app.last_copied_selection.as_deref(), Some("rav"));
    }

    #[test]
    fn mouse_drag_selection_can_cross_wrapped_rows() {
        let mut app = test_app("abcdefghij");
        app.resize_viewport(5, 8);

        click(&mut app, 4, 0);
        drag(&mut app, 3, 1);

        assert_eq!(app.selected_text().as_deref(), Some("cdefg"));
    }

    #[test]
    fn mouse_release_keeps_selection_but_stops_dragging() {
        let mut app = test_app("bravo");
        app.resize_viewport(5, 20);

        click(&mut app, 3, 0);
        drag(&mut app, 6, 0);
        release(&mut app, 6, 0);

        assert_eq!(app.selected_text().as_deref(), Some("rav"));
        assert_eq!(app.mouse_anchor, None);
    }

    #[test]
    fn temporary_status_messages_expire() {
        let mut app = test_app("text");
        app.set_status("Opened note.md");
        assert_eq!(app.status_message, "Opened note.md");
        assert!(app.status_message_expires_at.is_some());

        app.status_message_expires_at = Some(Instant::now() - Duration::from_millis(1));
        app.tick();

        assert_eq!(app.status_message, "");
        assert_eq!(app.status_message_expires_at, None);
    }

    #[test]
    fn vertical_movement_restores_preferred_column_after_short_line() {
        let mut app = test_app("abcdef\nx\nabcdef");
        app.resize_viewport(5, 80);
        app.cursor = Cursor { line: 0, column: 5 };

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.cursor, Cursor { line: 1, column: 1 });

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.cursor, Cursor { line: 2, column: 5 });
    }

    #[test]
    fn wrapped_visual_movement_restores_preferred_column_after_short_segment() {
        let mut app = test_app("abcdef gh ijklmnop");
        app.resize_viewport(5, 10);
        app.cursor = Cursor { line: 0, column: 5 };

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(
            app.cursor,
            Cursor {
                line: 0,
                column: 15
            }
        );
    }

    #[test]
    fn wrapped_visual_movement_preserves_column_across_physical_lines() {
        let mut app = test_app("abcdef gh\nijklmnop");
        app.resize_viewport(5, 10);
        app.cursor = Cursor { line: 0, column: 5 };

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });

        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.cursor, Cursor { line: 1, column: 5 });
    }

    #[test]
    fn horizontal_movement_resets_preferred_column() {
        let mut app = test_app("abcdef\nx\nabcdef");
        app.cursor = Cursor { line: 0, column: 5 };

        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('h'));
        press(&mut app, KeyCode::Char('j'));

        assert_eq!(app.cursor, Cursor { line: 2, column: 0 });
    }

    #[test]
    fn document_jumps_preserve_cursor_column() {
        let mut app = test_app("abcdef\nxy\nabcdef");
        app.cursor = Cursor { line: 0, column: 4 };

        press(&mut app, KeyCode::Char('G'));
        assert_eq!(app.cursor, Cursor { line: 2, column: 4 });

        let mut app = test_app("abcdef\nxy\nabcdef");
        app.cursor = Cursor { line: 2, column: 5 };
        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('g'));
        assert_eq!(app.cursor, Cursor { line: 0, column: 5 });
    }

    #[test]
    fn command_arrows_navigate_line_and_document_bounds() {
        let mut app = test_app("first line\nsecond line");
        app.cursor = Cursor { line: 1, column: 3 };

        press_modified(&mut app, KeyCode::Left, KeyModifiers::SUPER);
        assert_eq!(app.cursor, Cursor { line: 1, column: 0 });

        press_modified(&mut app, KeyCode::Right, KeyModifiers::SUPER);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 1,
                column: "second line".chars().count(),
            }
        );

        press_modified(&mut app, KeyCode::Up, KeyModifiers::SUPER);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 0,
                column: "first line".chars().count(),
            }
        );

        press_modified(&mut app, KeyCode::Down, KeyModifiers::SUPER);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 1,
                column: "second line".chars().count(),
            }
        );
    }

    #[test]
    fn terminal_translated_command_left_and_right_navigate_lines() {
        let mut app = test_app("first line\nsecond line");
        app.cursor = Cursor { line: 1, column: 3 };

        press_modified(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(app.cursor, Cursor { line: 1, column: 0 });

        press_modified(&mut app, KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 1,
                column: "second line".chars().count(),
            }
        );
    }

    #[test]
    fn option_left_and_right_move_by_words_in_insert_mode() {
        let mut app = test_app("one two three");
        app.mode = Mode::Insert;
        app.cursor = Cursor {
            line: 0,
            column: "one two three".chars().count(),
        };

        press_modified(&mut app, KeyCode::Left, KeyModifiers::ALT);
        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });

        press_modified(&mut app, KeyCode::Right, KeyModifiers::ALT);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 0,
                column: "one two three".chars().count(),
            }
        );
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn terminal_translated_option_left_and_right_move_by_words() {
        let mut app = test_app("one two three");
        app.mode = Mode::Insert;
        app.cursor = Cursor {
            line: 0,
            column: "one two three".chars().count(),
        };

        press_modified(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);
        assert_eq!(app.cursor, Cursor { line: 0, column: 8 });

        press_modified(&mut app, KeyCode::Char('f'), KeyModifiers::ALT);
        assert_eq!(
            app.cursor,
            Cursor {
                line: 0,
                column: "one two three".chars().count(),
            }
        );
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn command_delete_removes_to_logical_line_start_in_insert_mode() {
        let mut app = test_app("prefix content");
        app.mode = Mode::Insert;
        app.cursor = Cursor { line: 0, column: 7 };

        press_modified(&mut app, KeyCode::Backspace, KeyModifiers::SUPER);

        assert_eq!(app.buffer.as_string(), "content");
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn terminal_translated_command_delete_removes_to_line_start() {
        let mut app = test_app("prefix content");
        app.mode = Mode::Insert;
        app.cursor = Cursor { line: 0, column: 7 };

        press_modified(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);

        assert_eq!(app.buffer.as_string(), "content");
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn terminal_translated_command_delete_wins_over_normal_mode_undo() {
        let mut app = test_app("prefix content");
        app.cursor = Cursor { line: 0, column: 7 };

        press_modified(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);

        assert_eq!(app.buffer.as_string(), "content");
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn command_line_ignores_translated_command_navigation_chars() {
        let mut app = test_app("text");
        app.mode = Mode::CommandLine;
        app.command_line = "q".to_string();

        press_modified(&mut app, KeyCode::Char('e'), KeyModifiers::CONTROL);
        press_modified(&mut app, KeyCode::Char('b'), KeyModifiers::ALT);

        assert_eq!(app.command_line, "q");
    }

    #[test]
    fn primary_p_opens_command_sheet_and_esc_closes_it() {
        let mut app = test_app("text");
        app.status_message = "ready".to_string();

        press_modified(&mut app, KeyCode::Char('p'), KeyModifiers::SUPER);

        assert_eq!(app.mode, Mode::CommandLine);
        assert_eq!(app.sheet.prompt, CommandPrompt::Command);
        assert_eq!(app.command_line, "");
        assert!(!app.sheet.items.is_empty());

        press(&mut app, KeyCode::Esc);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_message, "ready");
        assert_eq!(app.command_line, "");
    }

    #[test]
    fn command_sheet_filters_matches_and_opens_selected_file() {
        let mut app = test_app("text");
        app.notes_dir = PathBuf::from("/notes");
        app.file_tree.entries = vec![
            TreeEntry {
                path: PathBuf::from("/notes/alpha.md"),
                display_name: "alpha.md".to_string(),
                is_dir: false,
            },
            TreeEntry {
                path: PathBuf::from("/notes/beta.md"),
                display_name: "beta.md".to_string(),
                is_dir: false,
            },
        ];

        press_modified(&mut app, KeyCode::Char('p'), KeyModifiers::SUPER);
        press(&mut app, KeyCode::Char('b'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(
            app.buffer.path.as_deref(),
            Some(Path::new("/notes/beta.md"))
        );
        assert_eq!(app.buffer.as_string(), "");
    }

    #[test]
    fn command_sheet_lists_files_before_commands() {
        let mut app = test_app("text");
        app.notes_dir = PathBuf::from("/notes");
        app.file_tree.entries = vec![TreeEntry {
            path: PathBuf::from("/notes/CHANGELOG.md"),
            display_name: "CHANGELOG.md".to_string(),
            is_dir: false,
        }];

        press(&mut app, KeyCode::Char(':'));

        assert_eq!(app.sheet.items[0].kind, SheetItemKind::File);
        assert_eq!(app.sheet.items[0].label, "CHANGELOG.md");
    }

    #[test]
    fn command_sheet_accepts_new_markdown_paths_that_do_not_exist() {
        let mut app = test_app("text");
        app.notes_dir = PathBuf::from("/notes");

        press_modified(&mut app, KeyCode::Char('p'), KeyModifiers::SUPER);
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('w'));
        press(&mut app, KeyCode::Char('-'));
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('o'));
        press(&mut app, KeyCode::Char('t'));
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Char('.'));
        press(&mut app, KeyCode::Char('m'));
        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(
            app.buffer.path.as_deref(),
            Some(Path::new("/notes/new-note.md"))
        );
        assert_eq!(app.buffer.as_string(), "");
    }

    #[test]
    fn command_sheet_keeps_typed_commands_fast() {
        let mut app = test_app("text");

        press(&mut app, KeyCode::Char(':'));
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, KeyCode::Enter);

        assert!(app.should_quit);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn command_sheet_does_not_execute_partial_command_suggestions_by_accident() {
        let mut app = test_app("text");
        app.notes_dir = PathBuf::from("/notes");

        press(&mut app, KeyCode::Char(':'));
        press(&mut app, KeyCode::Char('e'));
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.status_message, "Unknown command: e");
        assert_eq!(app.buffer.path, None);
    }

    #[test]
    fn command_sheet_can_complete_selected_files_into_the_input() {
        let mut app = test_app("text");
        app.notes_dir = PathBuf::from("/notes");
        app.file_tree.entries = vec![TreeEntry {
            path: PathBuf::from("/notes/projects/glass.md"),
            display_name: "glass.md".to_string(),
            is_dir: false,
        }];

        press(&mut app, KeyCode::Char(':'));
        for ch in "gla".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Right);

        assert_eq!(app.command_line, "projects/glass.md");
    }

    #[test]
    fn slash_opens_same_sheet_for_search_results() {
        let mut app = test_app("alpha\nneedle here\nomega");

        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.mode, Mode::CommandLine);
        assert_eq!(app.sheet.prompt, CommandPrompt::Search);

        for ch in "needle".chars() {
            press(&mut app, KeyCode::Char(ch));
        }

        assert_eq!(app.sheet.items[0].kind, SheetItemKind::Search);
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.cursor.line, 1);
        assert_eq!(app.cursor.column, 0);
        assert_eq!(app.search_result_indicator(), Some((1, 1)));
    }

    #[test]
    fn search_finds_queries_across_line_breaks() {
        let mut app = test_app("alpha foo\nbar omega");

        press(&mut app, KeyCode::Char('/'));
        for ch in "foo bar".chars() {
            press(&mut app, KeyCode::Char(ch));
        }

        assert_eq!(app.sheet.items[0].kind, SheetItemKind::Search);
        assert_eq!(app.sheet.items[0].label, "Line 1");
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.cursor, Cursor { line: 0, column: 6 });
        assert_eq!(
            app.search.matches[0],
            SearchMatch {
                line: 0,
                column: 6,
                end_line: 1,
                end_column: 3,
            }
        );
        assert_eq!(app.search_result_indicator(), Some((1, 1)));
    }

    #[test]
    fn command_mode_slash_search_uses_the_shared_sheet() {
        let mut app = test_app("alpha\nneedle here\nomega");

        press(&mut app, KeyCode::Char(':'));
        press(&mut app, KeyCode::Char('/'));
        for ch in "needle".chars() {
            press(&mut app, KeyCode::Char(ch));
        }

        assert_eq!(app.sheet.items[0].kind, SheetItemKind::Search);
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.cursor.line, 1);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn normal_mode_n_and_shift_n_navigate_search_results() {
        let mut app = test_app("needle\nmiddle needle\nneedle end");

        press(&mut app, KeyCode::Char('/'));
        for ch in "needle".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.search_result_indicator(), Some((1, 3)));
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });

        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.search_result_indicator(), Some((2, 3)));
        assert_eq!(app.cursor, Cursor { line: 1, column: 7 });

        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.search_result_indicator(), Some((1, 3)));
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn normal_mode_n_navigates_multiline_search_results() {
        let mut app = test_app("foo\nbar\nmiddle\nfoo bar");

        press(&mut app, KeyCode::Char('/'));
        for ch in "foo bar".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.search_result_indicator(), Some((1, 2)));
        assert_eq!(app.cursor, Cursor { line: 0, column: 0 });

        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.search_result_indicator(), Some((2, 2)));
        assert_eq!(app.cursor, Cursor { line: 3, column: 0 });
    }

    #[test]
    fn command_sheet_has_no_extra_height_without_results() {
        let mut app = test_app("text");

        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.sheet_panel_height(20), 0);

        for ch in "missing".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        assert_eq!(app.sheet_panel_height(20), 0);
    }

    #[test]
    fn normal_mode_u_undoes_last_insert() {
        let mut app = test_app("");
        app.mode = Mode::Insert;

        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('u'));

        assert_eq!(app.buffer.as_string(), "");
        assert_eq!(app.cursor, Cursor::default());
        assert_eq!(app.status_message, "Undid change");
    }

    #[test]
    fn normal_mode_u_undoes_checkbox_toggle_atomically() {
        let mut app = test_app("- [ ] todo");
        app.cursor = Cursor { line: 0, column: 0 };

        press(&mut app, KeyCode::Enter);
        assert_eq!(app.buffer.as_string(), "- [x] todo");

        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.buffer.as_string(), "- [ ] todo");
    }

    #[test]
    fn gf_without_link_sets_status_message() {
        let mut app = test_app("plain text");

        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('f'));

        assert_eq!(app.status_message, "No link under cursor");
    }

    #[test]
    fn gf_opens_relative_markdown_link_under_cursor() {
        let root = std::env::temp_dir().join(format!(
            "glass-link-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let notes = root.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        let index = notes.join("index.md");
        let target = notes.join("target.md");
        std::fs::write(&target, "target").unwrap();

        let mut app = test_app("[Target](target.md)");
        app.notes_dir = notes;
        app.buffer.path = Some(index);
        app.cursor = Cursor { line: 0, column: 2 };

        press(&mut app, KeyCode::Char('g'));
        press(&mut app, KeyCode::Char('f'));

        assert_eq!(app.buffer.path.as_deref(), Some(target.as_path()));
        assert_eq!(app.buffer.as_string(), "target");
    }

    #[test]
    fn wiki_link_paths_default_to_markdown_files() {
        let mut app = test_app("[[Daily Note]]");
        app.notes_dir = PathBuf::from("/notes");

        let path = app.resolve_link_path("Daily Note", LinkKind::Wiki);

        assert_eq!(path, PathBuf::from("/notes/Daily Note.md"));
    }

    #[test]
    fn command_forward_delete_removes_to_logical_line_end_in_insert_mode() {
        let mut app = test_app("prefix content");
        app.mode = Mode::Insert;
        app.cursor = Cursor { line: 0, column: 6 };

        press_modified(&mut app, KeyCode::Delete, KeyModifiers::SUPER);

        assert_eq!(app.buffer.as_string(), "prefix");
        assert_eq!(app.cursor, Cursor { line: 0, column: 6 });
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn enter_continues_checkbox_items_at_end_and_middle() {
        assert_eq!(
            list_continuation_after_enter("- [ ] todo", 10),
            ListContinuation::Continue("- [ ] ".to_string())
        );
        assert_eq!(
            list_continuation_after_enter("- [x] todo", 6),
            ListContinuation::Continue("- [ ] ".to_string())
        );
    }

    #[test]
    fn enter_exits_empty_checkbox_item() {
        assert_eq!(
            list_continuation_after_enter("- [ ] ", 6),
            ListContinuation::EndList {
                delete_to_column: 6
            }
        );
        assert_eq!(
            list_continuation_after_enter("- [ ]", 5),
            ListContinuation::EndList {
                delete_to_column: 5
            }
        );
    }

    #[test]
    fn double_enter_exits_checkbox_list_at_document_end() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "- [ ] todo");

        insert_newline_with_list_continuation(&mut buffer, &mut cursor);
        assert_eq!(buffer.as_string(), "- [ ] todo\n- [ ] ");
        assert_eq!(cursor, Cursor { line: 1, column: 6 });

        insert_newline_with_list_continuation(&mut buffer, &mut cursor);
        buffer.clamp_cursor(&mut cursor);

        assert_eq!(buffer.as_string(), "- [ ] todo\n\n");
        assert_eq!(cursor, Cursor { line: 1, column: 0 });
    }

    #[test]
    fn enter_exits_empty_checkbox_list_without_adding_extra_middle_blank() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "- [ ] todo\n- [ ] \nafter");
        cursor = Cursor { line: 1, column: 6 };

        insert_newline_with_list_continuation(&mut buffer, &mut cursor);

        assert_eq!(buffer.as_string(), "- [ ] todo\n\nafter");
        assert_eq!(cursor, Cursor { line: 1, column: 0 });
    }

    #[test]
    fn checkbox_marker_requires_separator_or_end() {
        assert_eq!(
            list_continuation_after_enter("- [ ]not a checkbox", 6),
            ListContinuation::Continue("- ".to_string())
        );
    }

    #[test]
    fn enter_continues_numbered_items_with_next_number() {
        assert_eq!(
            list_continuation_after_enter("9. todo", 7),
            ListContinuation::Continue("10. ".to_string())
        );
    }

    #[test]
    fn enter_continues_bullet_items_with_existing_marker() {
        assert_eq!(
            list_continuation_after_enter("  * todo", 8),
            ListContinuation::Continue("  * ".to_string())
        );
    }
}
