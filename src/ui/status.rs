use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    app::{App, CommandPrompt, Mode},
    config::theme::Theme,
};

const RAINBOW: &[(char, Color)] = &[
    ('█', Color::Rgb(255, 50, 50)),
    ('█', Color::Rgb(255, 140, 0)),
    ('█', Color::Rgb(255, 220, 0)),
    ('█', Color::Rgb(50, 220, 80)),
    ('█', Color::Rgb(30, 120, 255)),
];
const DIRTY_ICON: &str = " ●";

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
    if app.mode == Mode::CommandLine {
        let (prompt, cursor_offset) = command_prompt(app);
        let line = Line::from(Span::styled(
            format!(" {prompt}{}", app.command_line),
            theme.status,
        ));
        frame.render_widget(Paragraph::new(line).style(theme.status), area);
        frame.set_cursor_position(Position::new(
            area.x + 1 + cursor_offset + app.command_line.chars().count() as u16,
            area.y,
        ));
        return;
    }
    let line = status_line(app, theme, mode, &file);

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

fn command_prompt(app: &App) -> (&'static str, u16) {
    match app.sheet.prompt {
        CommandPrompt::Command => (":", 1),
        CommandPrompt::Search => ("/", 1),
    }
}

fn status_line(app: &App, theme: Theme, mode: &str, file: &str) -> Line<'static> {
    let mut spans = vec![Span::styled(format!(" {mode}  {file}"), theme.status)];
    if app.buffer.dirty {
        spans.push(Span::styled(
            DIRTY_ICON,
            Style::default()
                .fg(theme.dirty)
                .bg(theme.status.bg.unwrap_or(theme.background)),
        ));
    }
    spans.push(Span::styled(
        format!("  {}:{}", app.cursor.line + 1, app.cursor.column + 1),
        theme.status,
    ));
    if let Some((current, total)) = app.search_result_indicator() {
        spans.push(Span::styled(
            format!("  FIND {current}/{total}"),
            theme.status,
        ));
    }
    spans.push(Span::styled("  ", theme.status));
    spans.push(Span::styled(app.status_message.clone(), theme.status));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn dirty_indicator_is_separate_from_status_message() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("glass-status-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let mut app = App::new(dir.clone(), None)?;
        app.buffer.insert_str(&mut app.cursor, "dirty");
        app.status_message = "Glass".to_string();

        let theme = Theme::monochrome_for_tests();
        let line = status_line(&app, theme, "NORMAL", "[No note]");

        assert_eq!(line.spans[1].content.as_ref(), DIRTY_ICON);
        assert_eq!(line.spans[1].style.fg, Some(theme.dirty));
        assert_eq!(line.spans.last().unwrap().content.as_ref(), "Glass");
        assert_eq!(line.spans.last().unwrap().style, theme.status);

        std::fs::remove_dir(dir)?;
        Ok(())
    }

    #[test]
    fn status_line_shows_active_search_result_index() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("glass-status-search-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let mut app = App::new(dir.clone(), None)?;
        app.search.query = "needle".to_string();
        app.search.matches = vec![
            crate::app::SearchMatch {
                line: 0,
                column: 0,
                end_line: 0,
                end_column: 6,
            },
            crate::app::SearchMatch {
                line: 2,
                column: 4,
                end_line: 2,
                end_column: 10,
            },
        ];
        app.search.selected = 1;

        let line = status_line(&app, Theme::monochrome_for_tests(), "NORMAL", "[No note]");
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("FIND 2/2"));

        std::fs::remove_dir(dir)?;
        Ok(())
    }
}
