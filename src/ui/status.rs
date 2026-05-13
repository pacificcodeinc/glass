use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{App, Mode},
    config::theme::Theme,
};

const RAINBOW: &[(char, Color)] = &[
    ('█', Color::Rgb(255, 50, 50)),
    ('█', Color::Rgb(255, 140, 0)),
    ('█', Color::Rgb(255, 220, 0)),
    ('█', Color::Rgb(50, 220, 80)),
    ('█', Color::Rgb(30, 120, 255)),
];

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let mode = match app.mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
        Mode::CommandLine => "COMMAND",
        Mode::Visual => "VISUAL",
    };
    let file = app
        .buffer
        .path
        .as_ref()
        .and_then(|path| path.strip_prefix(&app.notes_dir).ok())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "[No note]".to_string());
    let dirty = if app.buffer.dirty { " +" } else { "" };
    let left = if app.mode == Mode::CommandLine {
        format!(":{}", app.command_line)
    } else {
        format!(
            " {mode}  {file}{dirty}  {}:{}",
            app.cursor.line + 1,
            app.cursor.column + 1
        )
    };

    let dirty_style = Style::default()
        .fg(theme.dirty)
        .bg(theme.status.bg.unwrap_or(theme.background));
    let show_message = app.mode != Mode::CommandLine;
    let mut spans = vec![Span::styled(left, theme.status)];
    if show_message {
        spans.push(Span::styled("  ", theme.status));
        spans.push(Span::styled(
            app.status_message.clone(),
            if app.buffer.dirty {
                dirty_style
            } else {
                theme.status
            },
        ));
    }
    let line = Line::from(spans);

    // Split status bar: left for status text, right for rainbow logo
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(RAINBOW.len() as u16)])
            .areas(area);

    frame.render_widget(
        Paragraph::new(line).bg(theme.status.bg.unwrap_or(theme.background)),
        left_area,
    );

    let rainbow_spans: Vec<Span> = RAINBOW
        .iter()
        .map(|(ch, color)| Span::styled(ch.to_string(), Style::default().fg(*color)))
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(rainbow_spans)).bg(theme.status.bg.unwrap_or(theme.background)),
        right_area,
    );
}
