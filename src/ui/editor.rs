use ratatui::{
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::{
    app::{App, Mode, SearchMatch, TextSelection},
    config::theme::Theme,
    editor::render::{
        column_in_wrap_segment, detect_list_marker, visible_rows, wrap_index_for_column, wrap_line,
    },
    markdown::highlight::{concealed_wrap_line, render_markdown_segment_with_completion},
};

const ARTICLE_WIDTH: u16 = 82;

pub fn page_area(area: Rect) -> Rect {
    let width = ARTICLE_WIDTH.min(area.width.saturating_sub(4)).max(20);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let page = page_area(area);
    let line_count = app.buffer.line_count();
    let gutter_width: u16 = (line_count.to_string().len() + 1) as u16;
    let text_width = page.width.saturating_sub(gutter_width).max(1) as usize;

    frame.render_widget(
        ratatui::widgets::Clear,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        },
    );
    let rows = visible_rows(
        &app.buffer,
        app.viewport.top_line,
        app.viewport.top_wrap_index,
        page.height as usize,
        text_width,
        |line_num, text, w| {
            if line_num == app.cursor.line {
                wrap_line(text, w)
            } else {
                concealed_wrap_line(text, w)
            }
        },
    );
    let visual_range = app.visual_line_anchor.map(|anchor| {
        let start = anchor.min(app.cursor.line);
        let end = anchor.max(app.cursor.line);
        start..=end
    });

    let cursor_line_text = app.buffer.line(app.cursor.line);
    let wrap_index_of_cursor =
        wrap_index_for_column(&cursor_line_text, app.cursor.column, text_width);
    let mut cursor_visual_y: usize = 0;
    let mut cursor_found = false;

    let lines = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let is_cursor_row =
                row.line_number == app.cursor.line && row.wrap_index == wrap_index_of_cursor;
            let active = row.line_number == app.cursor.line;

            let mut line = render_markdown_segment_with_completion(
                &row.full_text,
                row.source_start,
                row.source_end,
                theme,
                active,
                row.wrap_index,
                row.completed && row.wrap_index > 0,
            );

            if !app.search.query.is_empty() {
                let ranges = search_ranges_for_row(
                    &app.search.matches,
                    row.line_number,
                    row.source_start,
                    row.source_end,
                );
                line = highlight_search_ranges(line, &ranges, theme);
            }

            if let Some(selection) = app.text_selection {
                let ranges = selection_ranges_for_row(
                    selection,
                    row.line_number,
                    row.source_start,
                    row.source_end,
                );
                line = highlight_search_ranges(line, &ranges, theme);
            }

            if row.continuation_indent > 0 {
                let indent = Span::raw(" ".repeat(row.continuation_indent));
                line.spans.insert(0, indent);
            }

            if visual_range
                .as_ref()
                .is_some_and(|range| range.contains(&row.line_number))
            {
                line = selected_line(line, theme);
            }
            if is_cursor_row && app.mode != Mode::Visual {
                line.style = line.style.bg(theme.background);
            }

            if gutter_width > 0 {
                let gutter = if row.wrap_index == 0 && app.mode == Mode::Visual {
                    format!(
                        "{:>w$} ",
                        row.line_number + 1,
                        w = gutter_width as usize - 1
                    )
                } else {
                    " ".repeat(gutter_width as usize)
                };
                let mut spans = vec![Span::styled(gutter, Style::default().fg(theme.muted))];
                spans.extend(line.spans);
                line = Line::from(spans);
            }

            if is_cursor_row && !cursor_found {
                cursor_visual_y = i;
                cursor_found = true;
            }

            line
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(theme.background).fg(theme.text));
    frame.render_widget(paragraph, page);

    if app.mode != Mode::CommandLine {
        let cursor_indent = if wrap_index_of_cursor > 0 {
            detect_list_marker(&cursor_line_text)
        } else {
            0
        };
        let x = column_in_wrap_segment(&cursor_line_text, app.cursor.column, text_width) as u16
            + gutter_width
            + cursor_indent as u16;
        let y = cursor_visual_y as u16;
        frame.set_cursor_position(Position::new(page.x + x, page.y + y));
    }
}

fn selected_line(mut line: Line<'static>, theme: Theme) -> Line<'static> {
    line.style = theme.selection;
    for span in &mut line.spans {
        span.style = theme.selection;
    }
    line
}

fn highlight_search_ranges(
    mut line: Line<'static>,
    ranges: &[(usize, usize)],
    theme: Theme,
) -> Line<'static> {
    if ranges.is_empty() {
        return line;
    }

    let ranges = merged_ranges(ranges);
    if ranges.is_empty() {
        return line;
    }

    let original_spans = std::mem::take(&mut line.spans);
    let mut highlighted = Vec::new();
    let mut span_start = 0usize;

    for span in original_spans {
        let text = span.content.into_owned();
        let span_len = text.chars().count();
        let span_end = span_start + span_len;
        let mut local_cursor = 0usize;

        for &(range_start, range_end) in &ranges {
            if range_end <= span_start || range_start >= span_end {
                continue;
            }

            let local_start = range_start.max(span_start) - span_start;
            let local_end = range_end.min(span_end) - span_start;
            if local_cursor < local_start {
                highlighted.push(Span::styled(
                    char_slice(&text, local_cursor, local_start),
                    span.style,
                ));
            }
            highlighted.push(Span::styled(
                char_slice(&text, local_start, local_end),
                theme.selection,
            ));
            local_cursor = local_end;
        }

        if local_cursor < span_len {
            highlighted.push(Span::styled(
                char_slice(&text, local_cursor, span_len),
                span.style,
            ));
        }

        span_start = span_end;
    }

    line.spans = highlighted;
    line
}

fn search_ranges_for_row(
    matches: &[SearchMatch],
    line_number: usize,
    source_start: usize,
    source_end: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    for search_match in matches {
        if search_match.end_line < line_number || search_match.line > line_number {
            continue;
        }

        let start = if search_match.line == line_number {
            search_match.column
        } else {
            source_start
        };
        let end = if search_match.end_line == line_number {
            search_match.end_column
        } else {
            source_end
        };

        let start = start.max(source_start);
        let end = end.min(source_end);
        if start < end {
            ranges.push((start - source_start, end - source_start));
        }
    }

    ranges
}

fn selection_ranges_for_row(
    selection: TextSelection,
    line_number: usize,
    source_start: usize,
    source_end: usize,
) -> Vec<(usize, usize)> {
    let (start_cursor, end_cursor) = selection.ordered();
    if end_cursor.line < line_number || start_cursor.line > line_number {
        return Vec::new();
    }

    let start = if start_cursor.line == line_number {
        start_cursor.column
    } else {
        source_start
    };
    let end = if end_cursor.line == line_number {
        end_cursor.column
    } else {
        source_end
    };

    let start = start.max(source_start);
    let end = end.min(source_end);
    if start < end {
        vec![(start - source_start, end - source_start)]
    } else {
        Vec::new()
    }
}

fn merged_ranges(ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut ranges = ranges
        .iter()
        .copied()
        .filter(|(start, end)| start < end)
        .collect::<Vec<_>>();
    ranges.sort_unstable_by_key(|(start, end)| (*start, *end));

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut() {
            if start <= *last_end {
                *last_end = (*last_end).max(end);
                continue;
            }
        }

        merged.push((start, end));
    }

    merged
}

fn char_slice(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_highlight_splits_requested_ranges() {
        let theme = Theme::monochrome_for_tests();
        let line = Line::from(vec![
            Span::styled("before ", Style::default().fg(theme.text)),
            Span::styled("needle after", Style::default().fg(theme.text)),
        ]);

        let line = highlight_search_ranges(line, &[(7, 13)], theme);

        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[1].content.as_ref(), "needle");
        assert_eq!(line.spans[1].style, theme.selection);
    }

    #[test]
    fn search_ranges_split_across_wrapped_rows() {
        let search_match = SearchMatch {
            line: 0,
            column: 5,
            end_line: 0,
            end_column: 12,
        };

        assert_eq!(
            search_ranges_for_row(&[search_match], 0, 0, 8),
            vec![(5, 8)]
        );
        assert_eq!(
            search_ranges_for_row(&[search_match], 0, 8, 16),
            vec![(0, 4)]
        );
    }

    #[test]
    fn selection_ranges_split_across_wrapped_rows() {
        let selection = TextSelection {
            anchor: crate::editor::cursor::Cursor { line: 0, column: 2 },
            head: crate::editor::cursor::Cursor { line: 0, column: 9 },
        };

        assert_eq!(selection_ranges_for_row(selection, 0, 0, 6), vec![(2, 6)]);
        assert_eq!(selection_ranges_for_row(selection, 0, 6, 12), vec![(0, 3)]);
    }
}
