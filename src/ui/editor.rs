use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::Style,
    text::{Line, Text},
    widgets::Paragraph,
};

use crate::{
    app::{App, Mode},
    config::theme::Theme,
    editor::render::visible_rows,
    markdown::highlight::render_markdown_line,
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
    frame.render_widget(
        ratatui::widgets::Clear,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        },
    );
    let rows = visible_rows(&app.buffer, app.viewport.top_line, page.height as usize);
    let visual_range = app.visual_line_anchor.map(|anchor| {
        let start = anchor.min(app.cursor.line);
        let end = anchor.max(app.cursor.line);
        start..=end
    });
    let lines = rows
        .into_iter()
        .map(|row| {
            let active = row.line_number == app.cursor.line || app.mode == Mode::VisualLine;
            let mut line = render_markdown_line(&row.text, theme, active);
            if visual_range
                .as_ref()
                .is_some_and(|range| range.contains(&row.line_number))
            {
                line = selected_line(line, theme);
            }
            if row.line_number == app.cursor.line && app.mode != Mode::VisualLine {
                line.style = line.style.bg(theme.background);
            }
            line
        })
        .collect::<Vec<_>>();

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(theme.background).fg(theme.text));
    frame.render_widget(paragraph, page);

    if app.mode != Mode::CommandLine && app.overlay.is_none() {
        let x = app
            .cursor
            .column
            .saturating_sub(app.viewport.horizontal_offset)
            .min(page.width.saturating_sub(1) as usize) as u16;
        let y = app
            .cursor
            .line
            .saturating_sub(app.viewport.top_line)
            .min(page.height.saturating_sub(1) as usize) as u16;

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
