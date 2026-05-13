use crate::editor::buffer::DocumentBuffer;

#[derive(Debug, Clone)]
pub struct VisibleRow {
    pub line_number: usize,
    pub text: String,
}

pub fn visible_rows(buffer: &DocumentBuffer, top_line: usize, height: usize) -> Vec<VisibleRow> {
    let bottom = top_line.saturating_add(height);
    (top_line..bottom)
        .filter(|line| *line < buffer.line_count())
        .map(|line| VisibleRow {
            line_number: line,
            text: buffer.line(line),
        })
        .collect()
}
