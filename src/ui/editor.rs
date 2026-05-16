use ratatui::{
    layout::{Position, Rect},
    style::Style,
    text::{Line, Span, Text},
    widgets::Paragraph,
    Frame,
};

use crate::{
    app::{App, Mode},
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

            if row.continuation_indent > 0 {
                let indent = Span::raw(" ".repeat(row.continuation_indent));
                line.spans.insert(0, indent);
            }

            if !app.search.query.is_empty() {
                let query = app.search.query.as_str();
                line = highlight_search_matches(line, query, theme);
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

fn highlight_search_matches(mut line: Line<'static>, query: &str, theme: Theme) -> Line<'static> {
    let query = query.trim();
    if query.is_empty() {
        return line;
    }

    let visible_text = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    let ranges = search_ranges(&visible_text, query);
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

fn search_ranges(text: &str, query: &str) -> Vec<(usize, usize)> {
    let haystack = text.to_ascii_lowercase();
    let needle = query.to_ascii_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut byte_offset = 0usize;
    while let Some(relative_start) = haystack[byte_offset..].find(&needle) {
        let start_byte = byte_offset + relative_start;
        let end_byte = start_byte + needle.len();
        let start = haystack[..start_byte].chars().count();
        let end = haystack[..end_byte].chars().count();
        ranges.push((start, end));
        byte_offset = end_byte;
    }

    ranges
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
    fn search_highlight_splits_visible_text_matches() {
        let theme = Theme::monochrome_for_tests();
        let line = Line::from(vec![
            Span::styled("before ", Style::default().fg(theme.text)),
            Span::styled("needle after", Style::default().fg(theme.text)),
        ]);

        let line = highlight_search_matches(line, "needle", theme);

        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[1].content.as_ref(), "needle");
        assert_eq!(line.spans[1].style, theme.selection);
    }
}
