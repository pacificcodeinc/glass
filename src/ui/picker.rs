use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph},
};

use crate::{app::App, config::theme::Theme};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let Some(overlay) = &app.overlay else {
        return;
    };

    // Dimmed background overlay
    let dim_bg = Color::Rgb(25, 25, 30);
    frame.render_widget(Paragraph::new("").style(Style::default().bg(dim_bg)), area);

    let title = app.overlay_title().unwrap_or("Palette");
    let query = overlay.query.as_str();
    let item_count = app.overlay_items().len();
    let list_height = (item_count as u16 + 1).min(12).max(3);
    let popup_height = list_height + 5;
    let popup_width = 56u16;

    let popup = centered_rect(popup_width, popup_height, area);
    frame.render_widget(Clear, popup);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .margin(1)
        .split(popup);

    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::default().fg(theme.muted))
            .style(Style::default().bg(theme.background)),
        popup,
    );

    let prompt = Paragraph::new(Line::from(vec![
        Span::styled(title, theme.heading),
        Span::styled("  ", Style::default().fg(theme.text)),
        Span::styled(query, Style::default().fg(theme.text)),
    ]))
    .style(Style::default().bg(theme.background));
    frame.render_widget(prompt, inner[0]);

    let items = app.overlay_items();
    let has_items = !items.is_empty();
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

    if has_items {
        frame.render_widget(
            List::new(list_items).style(Style::default().bg(theme.background)),
            inner[1],
        );
    } else {
        let empty = Paragraph::new(Span::styled(
            "No matches",
            Style::default().fg(theme.muted).bg(theme.background),
        ))
        .style(Style::default().bg(theme.background));
        frame.render_widget(empty, inner[1]);
    }
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
