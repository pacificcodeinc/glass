use std::path::{Path, PathBuf};

use anyhow::Result;
use ropey::Rope;

use crate::{editor::cursor::Cursor, fs::persistence, markdown::parse::parse_markdown};

#[derive(Debug, Clone)]
pub struct DocumentBuffer {
    pub path: Option<PathBuf>,
    text: Rope,
    pub dirty: bool,
    saved_text: Rope,
}

impl DocumentBuffer {
    pub fn empty() -> Self {
        let text = Rope::new();
        Self {
            path: None,
            text: text.clone(),
            saved_text: text,
            dirty: false,
        }
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = persistence::load_utf8(path)?;
        parse_markdown(&contents)?;
        let text = Rope::from_str(&contents);
        Ok(Self {
            path: Some(path.to_path_buf()),
            saved_text: text.clone(),
            text,
            dirty: false,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        persistence::save_atomic(path, &self.text.to_string())?;
        self.saved_text = self.text.clone();
        self.dirty = false;
        Ok(())
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines().max(1)
    }

    pub fn line(&self, line: usize) -> String {
        if line >= self.text.len_lines() {
            return String::new();
        }

        self.text.line(line).to_string()
    }

    pub fn line_len_chars(&self, line: usize) -> usize {
        trim_line_ending_len(&self.line(line))
    }

    pub fn char_index(&self, cursor: Cursor) -> usize {
        let line = cursor.line.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(line);
        line_start + cursor.column.min(self.line_len_chars(line))
    }

    pub fn insert_char(&mut self, cursor: &mut Cursor, ch: char) {
        let index = self.char_index(*cursor);
        self.text.insert_char(index, ch);
        self.dirty = self.text != self.saved_text;

        if ch == '\n' {
            cursor.line += 1;
            cursor.column = 0;
        } else {
            cursor.column += 1;
        }
    }

    pub fn insert_str(&mut self, cursor: &mut Cursor, value: &str) {
        for ch in value.chars() {
            self.insert_char(cursor, ch);
        }
    }

    pub fn delete_previous_char(&mut self, cursor: &mut Cursor) {
        if cursor.line == 0 && cursor.column == 0 {
            return;
        }

        let end = self.char_index(*cursor);
        if end == 0 {
            return;
        }

        let previous = end - 1;
        let previous_line_len = if cursor.column == 0 {
            Some(self.line_len_chars(cursor.line.saturating_sub(1)))
        } else {
            None
        };
        self.text.remove(previous..end);
        self.dirty = self.text != self.saved_text;

        if cursor.column > 0 {
            cursor.column -= 1;
        } else {
            cursor.line = cursor.line.saturating_sub(1);
            cursor.column = previous_line_len.unwrap_or_default();
        }
    }

    pub fn delete_char(&mut self, cursor: &mut Cursor) {
        let start = self.char_index(*cursor);
        if start >= self.text.len_chars() {
            return;
        }

        self.text.remove(start..start + 1);
        self.dirty = self.text != self.saved_text;
        self.clamp_cursor(cursor);
    }

    pub fn delete_range(&mut self, start: usize, end: usize, cursor: &mut Cursor) {
        let start = start.min(self.text.len_chars());
        let end = end.min(self.text.len_chars());
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        if start == end {
            return;
        }

        self.text.remove(start..end);
        self.dirty = self.text != self.saved_text;
        *cursor = self.cursor_from_char_index(start);
        self.clamp_cursor(cursor);
    }

    pub fn delete_line_range(&mut self, start_line: usize, end_line: usize, cursor: &mut Cursor) {
        let start_line = start_line.min(self.line_count().saturating_sub(1));
        let end_line = end_line.min(self.line_count().saturating_sub(1));
        let (start_line, end_line) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };

        let start = self.line_start_char_index(start_line);
        let end = if end_line + 1 < self.text.len_lines() {
            self.line_start_char_index(end_line + 1)
        } else {
            self.text.len_chars()
        };

        self.delete_range(start, end, cursor);
        cursor.line = start_line.min(self.line_count().saturating_sub(1));
        cursor.column = 0;
        self.clamp_cursor(cursor);
    }

    pub fn line_start_char_index(&self, line: usize) -> usize {
        let line = line.min(self.text.len_lines().saturating_sub(1));
        self.text.line_to_char(line)
    }

    pub fn cursor_from_char_index(&self, index: usize) -> Cursor {
        let index = index.min(self.text.len_chars());
        let line = self.text.char_to_line(index);
        let column = index.saturating_sub(self.text.line_to_char(line));
        let mut cursor = Cursor { line, column };
        self.clamp_cursor(&mut cursor);
        cursor
    }

    pub fn clamp_cursor(&self, cursor: &mut Cursor) {
        cursor.line = cursor.line.min(self.line_count().saturating_sub(1));
        cursor.column = cursor.column.min(self.line_len_chars(cursor.line));
    }

    pub fn as_string(&self) -> String {
        self.text.to_string()
    }
}

fn trim_line_ending_len(line: &str) -> usize {
    line.trim_end_matches(['\r', '\n']).chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_text_tracks_cursor() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();

        buffer.insert_str(&mut cursor, "a\nb");

        assert_eq!(buffer.as_string(), "a\nb");
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.column, 1);
    }

    #[test]
    fn backspace_across_line_join_clamps_cursor() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "ab\ncd");

        cursor.line = 1;
        cursor.column = 0;
        buffer.delete_previous_char(&mut cursor);

        assert_eq!(buffer.as_string(), "abcd");
        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.column, 2);
    }

    #[test]
    fn delete_range_places_cursor_at_start() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "one two");

        buffer.delete_range(4, 7, &mut cursor);

        assert_eq!(buffer.as_string(), "one ");
        assert_eq!(cursor, Cursor { line: 0, column: 4 });
    }

    #[test]
    fn delete_line_range_removes_selected_lines() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "a\nb\nc\n");

        buffer.delete_line_range(1, 2, &mut cursor);

        assert_eq!(buffer.as_string(), "a\n");
        assert_eq!(cursor, Cursor { line: 1, column: 0 });
    }
}
