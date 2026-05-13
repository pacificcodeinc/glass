use crate::editor::buffer::DocumentBuffer;

#[derive(Debug, Clone)]
pub struct VisibleRow {
    pub line_number: usize,
    pub text: String,
    pub wrap_index: usize,
}

pub fn visible_rows(buffer: &DocumentBuffer, top_line: usize, height: usize, width: usize) -> Vec<VisibleRow> {
    let mut rows = Vec::new();
    let mut line = top_line;
    let wrap_width = width.max(1);

    while rows.len() < height && line < buffer.line_count() {
        let text = buffer.line(line);
        let trimmed = text.trim_end_matches(['\r', '\n']);

        if trimmed.is_empty() {
            rows.push(VisibleRow {
                line_number: line,
                text: String::new(),
                wrap_index: 0,
            });
            line += 1;
            continue;
        }

        let segments = word_wrap_segments(trimmed, wrap_width);
        for (wrap_index, &(start, end)) in segments.iter().enumerate() {
            if rows.len() >= height {
                break;
            }
            let chars: Vec<char> = trimmed.chars().collect();
            let chunk: String = chars[start..end].iter().collect();
            rows.push(VisibleRow {
                line_number: line,
                text: chunk,
                wrap_index,
            });
        }

        line += 1;
    }

    rows
}

/// Returns the wrap segment index (0-based) that contains the given column.
/// Uses the same word-boundary algorithm as `visible_rows`.
pub fn wrap_index_for_column(line_text: &str, column: usize, width: usize) -> usize {
    let trimmed = line_text.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return 0;
    }
    let segments = word_wrap_segments(trimmed, width.max(1));
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
    let segments = word_wrap_segments(trimmed, width.max(1));
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
    let segments = word_wrap_segments(trimmed, width.max(1));
    for &(start, end) in &segments {
        if column >= start && column < end {
            return (start, end);
        }
    }
    segments.last().copied().unwrap_or((0, 0))
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
        if let Some(rel_pos) = slice.iter().rposition(|c| *c == ' ') {
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
