use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{config::theme::Theme, editor::buffer::DocumentBuffer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

impl Default for TableAlignment {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub text: String,
    pub source_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct TableBlock {
    pub start_line: usize,
    pub delimiter_line: usize,
    pub end_line: usize,
    pub alignments: Vec<TableAlignment>,
    pub widths: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct TableLayout {
    blocks: Vec<TableBlock>,
}

#[derive(Debug, Clone)]
pub struct RenderedTableLine {
    pub line: Line<'static>,
    pub source_map: Vec<Option<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableBorder {
    Top,
    Bottom,
}

impl TableLayout {
    pub fn new(buffer: &DocumentBuffer) -> Self {
        let mut blocks = Vec::new();
        let mut line = 0;

        while line + 1 < buffer.line_count() {
            let header = trimmed_line(buffer, line);
            let delimiter = trimmed_line(buffer, line + 1);

            let header_cells = parse_table_cells(&header);
            let Some(delimiter_alignments) = parse_delimiter_row(&delimiter) else {
                line += 1;
                continue;
            };

            if header_cells.len() < 2 || delimiter_alignments.len() < 2 {
                line += 1;
                continue;
            }

            let start_line = line;
            let delimiter_line = line + 1;
            let mut end_line = delimiter_line + 1;
            while end_line < buffer.line_count() {
                let row = trimmed_line(buffer, end_line);
                if parse_table_cells(&row).len() < 2 {
                    break;
                }
                end_line += 1;
            }

            let column_count = column_count(buffer, start_line, end_line)
                .max(header_cells.len())
                .max(delimiter_alignments.len());
            let mut alignments = delimiter_alignments;
            alignments.resize(column_count, TableAlignment::Left);
            let widths = column_widths(buffer, start_line, delimiter_line, end_line, column_count);

            blocks.push(TableBlock {
                start_line,
                delimiter_line,
                end_line,
                alignments,
                widths,
            });
            line = end_line;
        }

        Self { blocks }
    }

    pub fn block_for_line(&self, line: usize) -> Option<&TableBlock> {
        self.blocks
            .iter()
            .find(|block| line >= block.start_line && line < block.end_line)
    }

    pub fn is_table_row(&self, line: usize) -> bool {
        self.block_for_line(line).is_some()
    }

    pub fn render_row(
        &self,
        line_number: usize,
        source: &str,
        available_width: usize,
        theme: Theme,
    ) -> Option<RenderedTableLine> {
        let block = self.block_for_line(line_number)?;
        let widths = block.fitted_widths(available_width);
        let is_delimiter = line_number == block.delimiter_line;
        let is_header = line_number == block.start_line;
        let cells = parse_table_cells(source.trim_end_matches(['\r', '\n']));
        let mut spans = Vec::new();
        let mut source_map = Vec::new();

        append_span(
            &mut spans,
            &mut source_map,
            if is_delimiter { "├" } else { "│" }.to_string(),
            Style::default().fg(theme.muted),
            std::iter::once(None),
        );

        for column in 0..block.column_count() {
            if is_delimiter {
                let separator = "─".repeat(widths[column] + 2);
                append_span(
                    &mut spans,
                    &mut source_map,
                    separator.clone(),
                    Style::default().fg(theme.muted),
                    std::iter::repeat_n(None, separator.chars().count()),
                );
                let joint = if column + 1 == block.column_count() {
                    "┤"
                } else {
                    "┼"
                };
                append_span(
                    &mut spans,
                    &mut source_map,
                    joint.to_string(),
                    Style::default().fg(theme.muted),
                    std::iter::once(None),
                );
                continue;
            }

            let alignment = block.alignments[column];
            let style = if is_header {
                theme.heading.add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text)
            };
            let empty_cell = TableCell {
                text: String::new(),
                source_indices: Vec::new(),
            };
            let cell = cells.get(column).unwrap_or(&empty_cell);
            let fitted = fit_cell(cell, widths[column], alignment);

            append_span(
                &mut spans,
                &mut source_map,
                " ".to_string(),
                style,
                std::iter::once(None),
            );
            append_span(
                &mut spans,
                &mut source_map,
                fitted.text,
                style,
                fitted.source_map.into_iter(),
            );
            append_span(
                &mut spans,
                &mut source_map,
                " ".to_string(),
                style,
                std::iter::once(None),
            );
            append_span(
                &mut spans,
                &mut source_map,
                "│".to_string(),
                Style::default().fg(theme.muted),
                std::iter::once(None),
            );
        }

        Some(RenderedTableLine {
            line: Line::from(spans),
            source_map,
        })
    }

    pub fn render_border_for_line(
        &self,
        line_number: usize,
        available_width: usize,
        theme: Theme,
        border: TableBorder,
    ) -> Option<RenderedTableLine> {
        let block = self.block_for_line(line_number)?;
        let is_target_line = match border {
            TableBorder::Top => line_number == block.start_line,
            TableBorder::Bottom => line_number + 1 == block.end_line,
        };
        if !is_target_line {
            return None;
        }

        let widths = block.fitted_widths(available_width);
        let text = match border {
            TableBorder::Top => border_line(&widths, "┌", "┬", "┐"),
            TableBorder::Bottom => border_line(&widths, "└", "┴", "┘"),
        };
        let source_map = std::iter::repeat_n(None, text.chars().count()).collect();
        Some(RenderedTableLine {
            line: Line::from(Span::styled(text, Style::default().fg(theme.muted))),
            source_map,
        })
    }
}

impl TableBlock {
    fn column_count(&self) -> usize {
        self.widths.len()
    }

    fn fitted_widths(&self, available_width: usize) -> Vec<usize> {
        let column_count = self.column_count();
        if column_count == 0 {
            return Vec::new();
        }

        let mut widths = self.widths.clone();
        let fixed_width = column_count * 2 + column_count + 1;

        while fixed_width + widths.iter().sum::<usize>() > available_width
            && widths.iter().any(|width| *width > 1)
        {
            if let Some((index, _)) = widths.iter().enumerate().max_by_key(|(_, width)| **width) {
                widths[index] = widths[index].saturating_sub(1).max(1);
            }
        }

        widths
    }
}

#[derive(Debug, Clone)]
struct FittedCell {
    text: String,
    source_map: Vec<Option<usize>>,
}

pub fn table_wrap_line(text: &str, _width: usize) -> (Vec<(usize, usize)>, usize) {
    (vec![(0, text.chars().count())], 0)
}

fn column_count(buffer: &DocumentBuffer, start: usize, end: usize) -> usize {
    (start..end)
        .map(|line| parse_table_cells(&trimmed_line(buffer, line)).len())
        .max()
        .unwrap_or_default()
}

fn column_widths(
    buffer: &DocumentBuffer,
    start: usize,
    delimiter: usize,
    end: usize,
    column_count: usize,
) -> Vec<usize> {
    let mut widths = vec![3; column_count];
    for line in start..end {
        if line == delimiter {
            continue;
        }

        for (index, cell) in parse_table_cells(&trimmed_line(buffer, line))
            .into_iter()
            .enumerate()
            .take(column_count)
        {
            widths[index] = widths[index].max(cell.text.chars().count());
        }
    }
    widths
}

fn trimmed_line(buffer: &DocumentBuffer, line: usize) -> String {
    buffer.line(line).trim_end_matches(['\r', '\n']).to_string()
}

fn parse_delimiter_row(source: &str) -> Option<Vec<TableAlignment>> {
    let cells = parse_table_cells(source);
    if cells.is_empty() {
        return None;
    }

    cells
        .iter()
        .map(|cell| parse_delimiter_cell(&cell.text))
        .collect()
}

fn parse_delimiter_cell(source: &str) -> Option<TableAlignment> {
    let value = source.trim();
    let left = value.starts_with(':');
    let right = value.ends_with(':');
    let dashes = value.trim_matches(':');

    if dashes.len() < 3 || !dashes.chars().all(|ch| ch == '-') {
        return None;
    }

    Some(match (left, right) {
        (true, true) => TableAlignment::Center,
        (false, true) => TableAlignment::Right,
        _ => TableAlignment::Left,
    })
}

fn parse_table_cells(source: &str) -> Vec<TableCell> {
    let chars = source.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Vec::new();
    }

    let pipe_indices = chars
        .iter()
        .enumerate()
        .filter_map(|(index, ch)| (*ch == '|' && !is_escaped(&chars, index)).then_some(index))
        .collect::<Vec<_>>();
    if pipe_indices.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    for pipe in pipe_indices {
        ranges.push((start, pipe));
        start = pipe + 1;
    }
    ranges.push((start, chars.len()));

    if ranges
        .first()
        .is_some_and(|(start, end)| chars[*start..*end].iter().all(|ch| ch.is_whitespace()))
    {
        ranges.remove(0);
    }
    if ranges
        .last()
        .is_some_and(|(start, end)| chars[*start..*end].iter().all(|ch| ch.is_whitespace()))
    {
        ranges.pop();
    }

    ranges
        .into_iter()
        .map(|(start, end)| parse_cell(&chars, start, end))
        .collect()
}

fn parse_cell(chars: &[char], mut start: usize, mut end: usize) -> TableCell {
    while start < end && chars[start].is_whitespace() {
        start += 1;
    }
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }

    let mut text = String::new();
    let mut source_indices = Vec::new();
    let mut index = start;
    while index < end {
        if chars[index] == '\\' && index + 1 < end && chars[index + 1] == '|' {
            text.push('|');
            source_indices.push(index + 1);
            index += 2;
            continue;
        }

        text.push(chars[index]);
        source_indices.push(index);
        index += 1;
    }

    TableCell {
        text,
        source_indices,
    }
}

fn is_escaped(chars: &[char], index: usize) -> bool {
    let mut backslashes = 0;
    let mut cursor = index;
    while cursor > 0 {
        cursor -= 1;
        if chars[cursor] != '\\' {
            break;
        }
        backslashes += 1;
    }
    backslashes % 2 == 1
}

fn fit_cell(cell: &TableCell, width: usize, alignment: TableAlignment) -> FittedCell {
    let mut chars = cell.text.chars().collect::<Vec<_>>();
    let mut source_map = cell
        .source_indices
        .iter()
        .copied()
        .map(Some)
        .collect::<Vec<_>>();

    if chars.len() > width {
        if width == 1 {
            chars = vec!['…'];
            source_map = vec![None];
        } else {
            chars.truncate(width - 1);
            source_map.truncate(width - 1);
            chars.push('…');
            source_map.push(None);
        }
    }

    let content_width = chars.len();
    let padding = width.saturating_sub(content_width);
    let (left_padding, right_padding) = match alignment {
        TableAlignment::Left => (0, padding),
        TableAlignment::Right => (padding, 0),
        TableAlignment::Center => (padding / 2, padding - padding / 2),
    };

    let mut text = String::new();
    let mut fitted_map = Vec::new();
    text.push_str(&" ".repeat(left_padding));
    fitted_map.extend(std::iter::repeat_n(None, left_padding));
    text.extend(chars);
    fitted_map.extend(source_map);
    text.push_str(&" ".repeat(right_padding));
    fitted_map.extend(std::iter::repeat_n(None, right_padding));

    FittedCell {
        text,
        source_map: fitted_map,
    }
}

fn append_span(
    spans: &mut Vec<Span<'static>>,
    source_map: &mut Vec<Option<usize>>,
    text: String,
    style: Style,
    map: impl IntoIterator<Item = Option<usize>>,
) {
    source_map.extend(map);
    spans.push(Span::styled(text, style));
}

fn border_line(widths: &[usize], left: &str, joint: &str, right: &str) -> String {
    let mut text = String::from(left);
    for (index, width) in widths.iter().enumerate() {
        text.push_str(&"─".repeat(width + 2));
        if index + 1 == widths.len() {
            text.push_str(right);
        } else {
            text.push_str(joint);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::{buffer::DocumentBuffer, cursor::Cursor};

    fn buffer_with(source: &str) -> DocumentBuffer {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, source);
        buffer
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn detects_table_blocks_and_alignment() {
        let buffer =
            buffer_with("| Name | Count | Note |\n| :--- | ---: | :---: |\n| Ada | 12 | ok |\n");
        let layout = TableLayout::new(&buffer);
        let block = layout.block_for_line(0).expect("table block");

        assert_eq!(block.start_line, 0);
        assert_eq!(block.end_line, 3);
        assert_eq!(
            block.alignments,
            vec![
                TableAlignment::Left,
                TableAlignment::Right,
                TableAlignment::Center
            ]
        );
    }

    #[test]
    fn escaped_pipes_stay_inside_cells() {
        let cells = parse_table_cells(r"| Name | A \| B |");

        assert_eq!(cells.len(), 2);
        assert_eq!(cells[1].text, "A | B");
        assert_eq!(cells[1].source_indices, vec![9, 10, 12, 13, 14]);
    }

    #[test]
    fn renders_inactive_rows_as_aligned_table() {
        let buffer = buffer_with("| Name | Count |\n| --- | ---: |\n| Ada | 12 |\n");
        let layout = TableLayout::new(&buffer);
        let rendered = layout
            .render_row(2, "| Ada | 12 |", 80, Theme::monochrome_for_tests())
            .expect("rendered table row");

        assert_eq!(line_text(&rendered.line), "│ Ada  │    12 │");
        assert_eq!(rendered.source_map[2], Some(2));
        assert_eq!(rendered.source_map[12], Some(8));
    }

    #[test]
    fn renders_delimiter_as_box_separator() {
        let buffer = buffer_with("| Name | Count |\n| --- | ---: |\n| Ada | 12 |\n");
        let layout = TableLayout::new(&buffer);
        let rendered = layout
            .render_row(1, "| --- | ---: |", 80, Theme::monochrome_for_tests())
            .expect("rendered table delimiter");

        assert_eq!(line_text(&rendered.line), "├──────┼───────┤");
    }

    #[test]
    fn renders_top_and_bottom_borders() {
        let buffer = buffer_with("| Name | Count |\n| --- | ---: |\n| Ada | 12 |\n");
        let layout = TableLayout::new(&buffer);
        let top = layout
            .render_border_for_line(0, 80, Theme::monochrome_for_tests(), TableBorder::Top)
            .expect("top border");
        let bottom = layout
            .render_border_for_line(2, 80, Theme::monochrome_for_tests(), TableBorder::Bottom)
            .expect("bottom border");

        assert_eq!(line_text(&top.line), "┌──────┬───────┐");
        assert_eq!(line_text(&bottom.line), "└──────┴───────┘");
        assert!(top.source_map.iter().all(Option::is_none));
        assert!(bottom.source_map.iter().all(Option::is_none));
    }

    #[test]
    fn narrows_wide_columns_with_ellipsis() {
        let buffer =
            buffer_with("| Name | Description |\n| --- | --- |\n| Ada | long description |\n");
        let layout = TableLayout::new(&buffer);
        let rendered = layout
            .render_row(
                2,
                "| Ada | long description |",
                18,
                Theme::monochrome_for_tests(),
            )
            .expect("rendered table row");

        assert_eq!(line_text(&rendered.line), "│ Ada  │ long d… │");
    }
}
