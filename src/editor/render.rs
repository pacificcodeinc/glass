use crate::editor::buffer::DocumentBuffer;

#[derive(Debug, Clone)]
pub struct VisibleRow {
    pub line_number: usize,
    pub full_text: String,
    pub source_start: usize,
    pub source_end: usize,
    pub wrap_index: usize,
    pub continuation_indent: usize,
    pub completed: bool,
}

pub fn visible_rows(
    buffer: &DocumentBuffer,
    top_line: usize,
    top_wrap_index: usize,
    height: usize,
    width: usize,
    wrap_fn: impl Fn(usize, &str, usize) -> (Vec<(usize, usize)>, usize),
) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    let mut line = top_line;
    let wrap_width = width.max(1);

    while rows.len() < height && line < buffer.line_count() {
        let text = buffer.line(line);
        let trimmed = text.trim_end_matches(['\r', '\n']);

        if trimmed.is_empty() {
            rows.push(VisibleRow {
                line_number: line,
                full_text: String::new(),
                source_start: 0,
                source_end: 0,
                wrap_index: 0,
                continuation_indent: 0,
                completed: false,
            });
            line += 1;
            continue;
        }

        let (segments, marker_len) = wrap_fn(line, trimmed, wrap_width);
        let completed = is_checked_checkbox(trimmed);
        for (wrap_index, &(start, end)) in segments.iter().enumerate() {
            if line == top_line && wrap_index < top_wrap_index {
                continue;
            }
            if rows.len() >= height {
                break;
            }
            rows.push(VisibleRow {
                line_number: line,
                full_text: trimmed.to_string(),
                source_start: start,
                source_end: end,
                wrap_index,
                continuation_indent: if wrap_index > 0 { marker_len } else { 0 },
                completed,
            });
        }

        line += 1;
    }

    rows
}

fn is_checked_checkbox(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("- [x] ") || text.starts_with("[x] ")
}

/// Returns the wrap segment index (0-based) that contains the given column.
/// Uses the same word-boundary algorithm as `visible_rows`.
pub fn wrap_index_for_column(line_text: &str, column: usize, width: usize) -> usize {
    let trimmed = line_text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return 0;
    }
    let (segments, _) = wrap_line(trimmed, width.max(1));
    for (i, &(start, end)) in segments.iter().enumerate() {
        if column >= start && column < end {
            return i;
        }
    }
    // Column is at or past the end — place in the last segment
    segments.len().saturating_sub(1)
}

/// Returns the column position within the wrap segment.
pub fn column_in_wrap_segment(line_text: &str, column: usize, width: usize) -> usize {
    let trimmed = line_text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return column;
    }
    let (segments, _) = wrap_line(trimmed, width.max(1));
    for &(start, end) in &segments {
        if column >= start && column < end {
            return column.saturating_sub(start);
        }
    }
    // Column is past the end — place at the end of the last segment
    if let Some(&(start, end)) = segments.last() {
        return (end - start).min(column.saturating_sub(start));
    }
    0
}

/// Returns (start, end) character bounds of the wrap segment that contains `column`.
pub fn visual_line_bounds(line_text: &str, column: usize, width: usize) -> (usize, usize) {
    let trimmed = line_text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return (0, 0);
    }
    let (segments, _) = wrap_line(trimmed, width.max(1));
    for &(start, end) in &segments {
        if column >= start && column < end {
            return (start, end);
        }
    }
    segments.last().copied().unwrap_or((0, 0))
}

/// Detect list-item marker length in characters (including leading whitespace).
/// Returns 0 if the line does not start with a recognised marker.
pub fn detect_list_marker(text: &str) -> usize {
    let trimmed = text.trim_start();
    let ws = text.len() - trimmed.len();

    if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") {
        return ws + 6;
    }
    if trimmed.starts_with("[ ] ") || trimmed.starts_with("[x] ") {
        return ws + 4;
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return ws + 2;
    }

    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && trimmed.get(i..i + 2) == Some(". ") {
        return ws + i + 2;
    }

    0
}

/// Wrap a line into visual segments.
/// For list items the first segment includes the marker and subsequent
/// segments are wrapped with a reduced width so they can be indented.
/// Returns the segments and the marker length (0 for non-list lines).
pub fn wrap_line(text: &str, width: usize) -> (Vec<(usize, usize)>, usize) {
    let marker_len = detect_list_marker(text);

    if marker_len == 0 || marker_len >= width {
        return (word_wrap_segments(text, width), 0);
    }

    let content = &text[marker_len..];
    if content.is_empty() {
        return (vec![(0, text.chars().count())], marker_len);
    }

    let content_width = width - marker_len;
    let content_segments = word_wrap_segments(content, content_width);

    let mut segments = Vec::new();
    for (i, (start, end)) in content_segments.into_iter().enumerate() {
        if i == 0 {
            segments.push((0, marker_len + end));
        } else {
            segments.push((marker_len + start, marker_len + end));
        }
    }

    (segments, marker_len)
}

pub fn word_wrap_segments(text: &str, width: usize) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut segments = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        let end = (pos + width).min(chars.len());
        if end >= chars.len() {
            segments.push((pos, chars.len()));
            break;
        }

        let slice = &chars[pos..end];
        if let Some(rel_pos) = slice.iter().rposition(|c| *c == ' ')
            && rel_pos > 0
        {
            let break_at = pos + rel_pos;
            segments.push((pos, break_at));
            pos = break_at + 1;
        } else {
            segments.push((pos, end));
            pos = end;
        }
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{editor::buffer::DocumentBuffer, editor::cursor::Cursor};

    fn buffer_with(text: &str) -> DocumentBuffer {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, text);
        buffer
    }

    #[test]
    fn marker_only_checkbox_renders_as_visible_row() {
        let buffer = buffer_with("before\n- [ ] ");

        let rows = visible_rows(&buffer, 0, 0, 10, 80, |_, text, w| wrap_line(text, w));

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].line_number, 1);
        assert_eq!(rows[1].full_text, "[ ] ");
        assert_eq!(rows[1].wrap_index, 0);
        assert!(!rows[1].completed);
    }

    #[test]
    fn marker_only_checkbox_has_wrap_segment_for_end_cursor() {
        assert_eq!(wrap_index_for_column("- [ ] ", 6, 80), 0);
        assert_eq!(column_in_wrap_segment("- [ ] ", 6, 80), 6);
        assert_eq!(visual_line_bounds("- [ ] ", 6, 80), (0, 6));
    }

    #[test]
    fn word_wrap_does_not_emit_empty_segment_when_break_starts_with_space() {
        let segments = word_wrap_segments("one two", 4);

        assert_eq!(segments, vec![(0, 3), (4, 7)]);
    }

    #[test]
    fn checked_checkbox_completion_state_survives_wrapping() {
        let buffer = buffer_with("- [x] one two three four five");

        let rows = visible_rows(&buffer, 0, 0, 10, 14, |_, text, w| wrap_line(text, w));

        assert!(rows.len() > 1);
        assert!(rows.iter().all(|row| row.completed));
    }

    #[test]
    fn visible_rows_can_start_inside_wrapped_line() {
        let buffer = buffer_with("one two three four");

        let rows = visible_rows(&buffer, 0, 1, 2, 8, |_, text, w| wrap_line(text, w));

        assert_eq!(rows[0].line_number, 0);
        assert_eq!(rows[0].wrap_index, 1);
        assert_eq!(
            &rows[0].full_text[rows[0].source_start..rows[0].source_end],
            "three"
        );
    }
}
