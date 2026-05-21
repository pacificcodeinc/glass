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
    pub insertion_index: usize,
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

    pub fn render_row_segment(
        &self,
        line_number: usize,
        source: &str,
        available_width: usize,
        theme: Theme,
        wrap_index: usize,
    ) -> Option<RenderedTableLine> {
        self.render_row_lines(line_number, source, available_width, theme)?
            .into_iter()
            .nth(wrap_index)
    }

    pub fn wrap_line(
        &self,
        line_number: usize,
        source: &str,
        available_width: usize,
    ) -> (Vec<(usize, usize)>, usize) {
        let line_len = source.chars().count();
        let Some(block) = self.block_for_line(line_number) else {
            return (vec![(0, line_len)], 0);
        };
        if line_number == block.delimiter_line {
            return (vec![(0, line_len)], 0);
        }

        let widths = block.fitted_widths(available_width);
        let cells = parse_table_cells(source.trim_end_matches(['\r', '\n']));
        let empty_cell = TableCell {
            text: String::new(),
            source_indices: Vec::new(),
            insertion_index: source.chars().count(),
        };
        let row_height = (0..block.column_count())
            .map(|column| {
                let cell = cells.get(column).unwrap_or(&empty_cell);
                wrap_cell(cell, widths[column], block.alignments[column]).len()
            })
            .max()
            .unwrap_or(1);
        let separator_height = usize::from(block.has_body_row_after(line_number));

        (
            std::iter::repeat_n((0, line_len), row_height)
                .chain(std::iter::repeat_n((line_len, line_len), separator_height))
                .collect(),
            0,
        )
    }

    fn render_row_lines(
        &self,
        line_number: usize,
        source: &str,
        available_width: usize,
        theme: Theme,
    ) -> Option<Vec<RenderedTableLine>> {
        let block = self.block_for_line(line_number)?;
        let widths = block.fitted_widths(available_width);
        let is_delimiter = line_number == block.delimiter_line;
        let is_header = line_number == block.start_line;
        let cells = parse_table_cells(source.trim_end_matches(['\r', '\n']));

        if is_delimiter {
            return Some(vec![render_delimiter_row(block, &widths, theme)]);
        }

        let empty_cell = TableCell {
            text: String::new(),
            source_indices: Vec::new(),
            insertion_index: source.chars().count(),
        };
        let wrapped_cells = (0..block.column_count())
            .map(|column| {
                let cell = cells.get(column).unwrap_or(&empty_cell);
                wrap_cell(cell, widths[column], block.alignments[column])
            })
            .collect::<Vec<_>>();
        let row_height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let style = if is_header {
            theme.heading.add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.text)
        };

        let mut rows = Vec::new();
        for visual_row in 0..row_height {
            rows.push(render_content_row(
                block,
                &widths,
                &wrapped_cells,
                visual_row,
                style,
                theme,
            ));
        }
        if block.has_body_row_after(line_number) {
            rows.push(render_delimiter_row(block, &widths, theme));
        }

        Some(rows)
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

    fn has_body_row_after(&self, line_number: usize) -> bool {
        line_number > self.delimiter_line && line_number + 1 < self.end_line
    }
}

fn render_delimiter_row(block: &TableBlock, widths: &[usize], theme: Theme) -> RenderedTableLine {
    let mut spans = Vec::new();
    let mut source_map = Vec::new();

    append_span(
        &mut spans,
        &mut source_map,
        "├".to_string(),
        Style::default().fg(theme.muted),
        std::iter::once(None),
    );

    for (column, width) in widths.iter().enumerate().take(block.column_count()) {
        let separator = "─".repeat(width + 2);
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
    }

    RenderedTableLine {
        line: Line::from(spans),
        source_map,
    }
}

fn render_content_row(
    block: &TableBlock,
    widths: &[usize],
    wrapped_cells: &[Vec<FittedCell>],
    visual_row: usize,
    style: Style,
    theme: Theme,
) -> RenderedTableLine {
    let mut spans = Vec::new();
    let mut source_map = Vec::new();

    append_span(
        &mut spans,
        &mut source_map,
        "│".to_string(),
        Style::default().fg(theme.muted),
        std::iter::once(None),
    );

    for column in 0..block.column_count() {
        let fitted = wrapped_cells
            .get(column)
            .and_then(|cell_lines| cell_lines.get(visual_row))
            .cloned()
            .unwrap_or_else(|| blank_cell(widths[column]));

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

    RenderedTableLine {
        line: Line::from(spans),
        source_map,
    }
}

#[derive(Debug, Clone)]
struct FittedCell {
    text: String,
    source_map: Vec<Option<usize>>,
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
        insertion_index: start,
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

fn wrap_cell(cell: &TableCell, width: usize, alignment: TableAlignment) -> Vec<FittedCell> {
    if cell.text.is_empty() {
        return vec![empty_fitted_cell(width.max(1), cell.insertion_index)];
    }

    cell_wrap_segments(cell, width.max(1))
        .into_iter()
        .map(|(start, end)| {
            let chars = cell.text.chars().skip(start).take(end - start);
            let source_map = cell.source_indices[start..end]
                .iter()
                .copied()
                .map(Some)
                .collect::<Vec<_>>();
            fit_cell_segment(chars, source_map, width, alignment)
        })
        .collect()
}

fn cell_wrap_segments(cell: &TableCell, width: usize) -> Vec<(usize, usize)> {
    let chars = cell.text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return vec![(0, 0)];
    }

    let mut segments = Vec::new();
    let mut pos = 0;
    while pos < chars.len() {
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= chars.len() {
            break;
        }

        let end = (pos + width).min(chars.len());
        if end >= chars.len() {
            segments.push((pos, chars.len()));
            break;
        }

        let slice = &chars[pos..end];
        if let Some(rel_pos) = slice.iter().rposition(|ch| ch.is_whitespace())
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

    if segments.is_empty() {
        vec![(0, 0)]
    } else {
        segments
    }
}

fn fit_cell_segment(
    chars: impl IntoIterator<Item = char>,
    source_map: Vec<Option<usize>>,
    width: usize,
    alignment: TableAlignment,
) -> FittedCell {
    let chars = chars.into_iter().collect::<Vec<_>>();
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

fn blank_cell(width: usize) -> FittedCell {
    FittedCell {
        text: " ".repeat(width),
        source_map: std::iter::repeat_n(None, width).collect(),
    }
}

fn empty_fitted_cell(width: usize, insertion_index: usize) -> FittedCell {
    let mut source_map = std::iter::repeat_n(None, width).collect::<Vec<_>>();
    if let Some(first) = source_map.first_mut() {
        *first = Some(insertion_index);
    }
    FittedCell {
        text: " ".repeat(width),
        source_map,
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
            .render_row_segment(2, "| Ada | 12 |", 80, Theme::monochrome_for_tests(), 0)
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
            .render_row_segment(1, "| --- | ---: |", 80, Theme::monochrome_for_tests(), 0)
            .expect("rendered table delimiter");

        assert_eq!(line_text(&rendered.line), "├──────┼───────┤");
    }

    #[test]
    fn renders_internal_separators_between_body_rows() {
        let buffer = buffer_with("| A | B |\n| --- | --- |\n| x | y |\n| z | q |\n");
        let layout = TableLayout::new(&buffer);
        let (first_row_segments, _) = layout.wrap_line(2, "| x | y |", 80);
        let (last_row_segments, _) = layout.wrap_line(3, "| z | q |", 80);
        let separator = layout
            .render_row_segment(2, "| x | y |", 80, Theme::monochrome_for_tests(), 1)
            .expect("rendered table row separator");

        assert_eq!(first_row_segments.len(), 2);
        assert_eq!(last_row_segments.len(), 1);
        assert_eq!(line_text(&separator.line), "├─────┼─────┤");
        assert!(separator.source_map.iter().all(Option::is_none));
    }

    #[test]
    fn wraps_wide_cells_across_row_segments() {
        let buffer =
            buffer_with("| Name | Description |\n| --- | --- |\n| Ada | long description |\n");
        let layout = TableLayout::new(&buffer);
        let (segments, _) = layout.wrap_line(2, "| Ada | long description |", 18);
        let rendered = (0..segments.len())
            .map(|wrap_index| {
                layout
                    .render_row_segment(
                        2,
                        "| Ada | long description |",
                        18,
                        Theme::monochrome_for_tests(),
                        wrap_index,
                    )
                    .expect("rendered table row segment")
            })
            .collect::<Vec<_>>();

        assert_eq!(segments.len(), 3);
        assert_eq!(line_text(&rendered[0].line), "│ Ada  │ long    │");
        assert_eq!(line_text(&rendered[1].line), "│      │ descrip │");
        assert_eq!(line_text(&rendered[2].line), "│      │ tion    │");
    }

    #[test]
    fn wraps_wide_cells_inside_single_character_columns() {
        let buffer = buffer_with("| A | B |\n| --- | --- |\n| x | abc |\n");
        let layout = TableLayout::new(&buffer);
        let (segments, _) = layout.wrap_line(2, "| x | abc |", 7);
        let rendered = (0..segments.len())
            .map(|wrap_index| {
                layout
                    .render_row_segment(
                        2,
                        "| x | abc |",
                        7,
                        Theme::monochrome_for_tests(),
                        wrap_index,
                    )
                    .expect("rendered narrow table row segment")
            })
            .collect::<Vec<_>>();

        assert_eq!(segments.len(), 3);
        assert_eq!(line_text(&rendered[0].line), "│ x │ a │");
        assert_eq!(line_text(&rendered[1].line), "│   │ b │");
        assert_eq!(line_text(&rendered[2].line), "│   │ c │");
    }

    #[test]
    fn keeps_source_maps_on_wrapped_cell_segments() {
        let buffer =
            buffer_with("| Name | Description |\n| --- | --- |\n| Ada | long description |\n");
        let layout = TableLayout::new(&buffer);
        let rendered = layout
            .render_row_segment(
                2,
                "| Ada | long description |",
                18,
                Theme::monochrome_for_tests(),
                1,
            )
            .expect("rendered table row segment");

        assert_eq!(line_text(&rendered.line), "│      │ descrip │");
        assert!(rendered.source_map.contains(&Some(13)));
        assert!(!line_text(&rendered.line).contains('…'));
    }
}
