use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
};

use crate::{app::App, config::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(overlay) = &app.overlay else {
        return;
    };

    let popup = centered_rect(64, 14, area);
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .margin(1)
        .split(popup);

    frame.render_widget(Clear, popup);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().fg(theme.text).bg(theme.background)),
        popup,
    );

    let title = app.overlay_title().unwrap_or("Palette");
    let prompt = Paragraph::new(Line::from(vec![
        Span::styled(title, theme.heading),
        Span::styled("  ", Style::default().fg(theme.text)),
        Span::styled(&overlay.query, Style::default().fg(theme.text)),
    ]))
    .style(Style::default().bg(theme.background));
    frame.render_widget(prompt, inner[0]);

    let items = app.overlay_items();
    let list_items = items
        .iter()
        .enumerate()
        .take(inner[1].height as usize)
        .map(|(index, item)| {
            let style = if index == overlay.selected {
                theme.selection.add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text).bg(theme.background)
            };
            ListItem::new(Line::from(vec![
                Span::styled(item.label.clone(), style),
                Span::styled("  ", style),
                Span::styled(item.detail.clone(), style.fg(theme.muted)),
            ]))
        })
        .collect::<Vec<_>>();

    frame.render_widget(
        List::new(list_items).style(Style::default().bg(theme.background)),
        inner[1],
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(4)).max(20);
    let height = height.min(area.height.saturating_sub(2)).max(6);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    }
}
