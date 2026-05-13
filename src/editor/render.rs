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

        let chars: Vec<char> = trimmed.chars().collect();
        for (wrap_index, chunk) in chars.chunks(wrap_width).enumerate() {
            if rows.len() >= height {
                break;
            }
            rows.push(VisibleRow {
                line_number: line,
                text: chunk.iter().collect(),
                wrap_index,
            });
        }

        line += 1;
    }

    rows
}
