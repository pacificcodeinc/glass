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
        column_in_wrap_segment, detect_list_marker, visible_rows, wrap_index_for_column,
    },
    markdown::highlight::render_markdown_line_with_completion,
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
        page.height as usize,
        text_width,
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
            let active = is_cursor_row;

            // Prepend indentation for continuation lines of list items
            let display_text = if row.continuation_indent > 0 {
                " ".repeat(row.continuation_indent) + &row.text
            } else {
                row.text.clone()
            };
            let mut line = render_markdown_line_with_completion(
                &display_text,
                theme,
                active,
                row.wrap_index,
                row.completed && row.wrap_index > 0,
            );

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

    if app.mode != Mode::CommandLine && app.overlay.is_none() {
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
