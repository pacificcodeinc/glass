use crate::editor::{buffer::DocumentBuffer, cursor::Cursor};

pub fn left(cursor: &mut Cursor) {
    cursor.column = cursor.column.saturating_sub(1);
}

pub fn right(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    cursor.column = (cursor.column + 1).min(buffer.line_len_chars(cursor.line));
}

pub fn up(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    cursor.line = cursor.line.saturating_sub(1);
    cursor.column = cursor.column.min(buffer.line_len_chars(cursor.line));
}

pub fn line_start(cursor: &mut Cursor) {
    cursor.column = 0;
}

pub fn first_non_blank(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    let line = buffer.line(cursor.line);
    cursor.column = line
        .trim_end_matches(['\r', '\n'])
        .chars()
        .position(|ch| !ch.is_whitespace())
        .unwrap_or(0);
}

pub fn line_end(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    cursor.column = buffer.line_len_chars(cursor.line);
}

pub fn document_end(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    cursor.line = buffer.line_count().saturating_sub(1);
    cursor.column = buffer.line_len_chars(cursor.line);
}

pub fn word_forward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    let text = buffer.as_string();
    let mut index = buffer.char_index(*cursor);
    let chars: Vec<char> = text.chars().collect();

    while index < chars.len() && is_word(chars[index]) {
        index += 1;
    }
    while index < chars.len() && !is_word(chars[index]) {
        index += 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

pub fn word_backward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    let mut index = buffer.char_index(*cursor).saturating_sub(1);

    while index > 0 && !is_word(chars[index]) {
        index -= 1;
    }
    while index > 0 && is_word(chars[index - 1]) {
        index -= 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

pub fn word_end_forward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    let mut index = buffer.char_index(*cursor);

    if index < chars.len() && is_word(chars[index]) {
        index += 1;
    }
    while index < chars.len() && !is_word(chars[index]) {
        index += 1;
    }
    while index + 1 < chars.len() && is_word(chars[index + 1]) {
        index += 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

pub fn word_end_backward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return;
    }

    let mut index = buffer.char_index(*cursor).saturating_sub(1);

    if index < chars.len() && is_word(chars[index]) {
        while index > 0 && is_word(chars[index - 1]) {
            index -= 1;
        }
        index = index.saturating_sub(1);
    }

    while index > 0 && !is_word(chars[index]) {
        index -= 1;
    }

    while index + 1 < chars.len() && is_word(chars[index + 1]) {
        index += 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

pub fn big_word_forward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    word_motion_forward(buffer, cursor, |ch| !ch.is_whitespace());
}

pub fn big_word_backward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    word_motion_backward(buffer, cursor, |ch| !ch.is_whitespace());
}

pub fn big_word_end_forward(buffer: &DocumentBuffer, cursor: &mut Cursor) {
    word_end_motion_forward(buffer, cursor, |ch| !ch.is_whitespace());
}

fn word_motion_forward(
    buffer: &DocumentBuffer,
    cursor: &mut Cursor,
    is_unit: impl Fn(char) -> bool,
) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    let mut index = buffer.char_index(*cursor);

    while index < chars.len() && is_unit(chars[index]) {
        index += 1;
    }
    while index < chars.len() && !is_unit(chars[index]) {
        index += 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

fn word_motion_backward(
    buffer: &DocumentBuffer,
    cursor: &mut Cursor,
    is_unit: impl Fn(char) -> bool,
) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    let mut index = buffer.char_index(*cursor).saturating_sub(1);

    while index > 0 && !is_unit(chars[index]) {
        index -= 1;
    }
    while index > 0 && is_unit(chars[index - 1]) {
        index -= 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

fn word_end_motion_forward(
    buffer: &DocumentBuffer,
    cursor: &mut Cursor,
    is_unit: impl Fn(char) -> bool,
) {
    let text = buffer.as_string();
    let chars: Vec<char> = text.chars().collect();
    let mut index = buffer.char_index(*cursor);

    if index < chars.len() && is_unit(chars[index]) {
        index += 1;
    }
    while index < chars.len() && !is_unit(chars[index]) {
        index += 1;
    }
    while index + 1 < chars.len() && is_unit(chars[index + 1]) {
        index += 1;
    }

    set_cursor_from_char_index(buffer, cursor, index);
}

fn set_cursor_from_char_index(buffer: &DocumentBuffer, cursor: &mut Cursor, mut target: usize) {
    target = target.min(buffer.as_string().chars().count());

    let mut remaining = target;
    for line in 0..buffer.line_count() {
        let len = buffer.line(line).chars().count();
        if remaining <= len {
            cursor.line = line;
            cursor.column = remaining.min(buffer.line_len_chars(line));
            return;
        }
        remaining -= len;
    }

    document_end(buffer, cursor);
}

fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer_with(text: &str) -> DocumentBuffer {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, text);
        buffer
    }

    #[test]
    fn word_end_forward_moves_to_current_or_next_word_end() {
        let buffer = buffer_with("one two three");
        let mut cursor = Cursor { line: 0, column: 0 };

        word_end_forward(&buffer, &mut cursor);
        assert_eq!(cursor.column, 2);

        word_end_forward(&buffer, &mut cursor);
        assert_eq!(cursor.column, 6);
    }

    #[test]
    fn word_end_backward_moves_to_previous_word_end() {
        let buffer = buffer_with("one two three");
        let mut cursor = Cursor { line: 0, column: 8 };

        word_end_backward(&buffer, &mut cursor);
        assert_eq!(cursor.column, 6);

        word_end_backward(&buffer, &mut cursor);
        assert_eq!(cursor.column, 2);
    }

    #[test]
    fn first_non_blank_skips_indent() {
        let buffer = buffer_with("    heading");
        let mut cursor = Cursor { line: 0, column: 0 };

        first_non_blank(&buffer, &mut cursor);
        assert_eq!(cursor.column, 4);
    }
}
