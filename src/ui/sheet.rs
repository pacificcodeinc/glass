use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
    Frame,
};

use crate::{
    app::{App, SheetItemKind},
    config::theme::Theme,
};

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let visible_height = area.height.max(1) as usize;
    let (start, end) = sheet_window(app, visible_height);
    let items = sheet_items(app, theme, start, end);
    let mut state = ListState::default();

    if !app.sheet.items.is_empty() {
        state.select(Some(app.sheet.selected.saturating_sub(start)));
    }

    let list = List::new(items)
        .block(Block::default().style(theme.status))
        .style(theme.status)
        .highlight_style(theme.selection)
        .highlight_symbol(" ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn sheet_items(app: &App, theme: Theme, start: usize, end: usize) -> Vec<ListItem<'static>> {
    if app.sheet.items.is_empty() {
        return vec![ListItem::new(Line::from(Span::styled(
            empty_message(app),
            theme.status,
        )))];
    }

    app.sheet.items[start..end]
        .iter()
        .map(|item| {
            let kind = match item.kind {
                SheetItemKind::Command => "CMD",
                SheetItemKind::File => "FILE",
                SheetItemKind::Search => "FIND",
            };
            let action = match item.kind {
                SheetItemKind::Command => item.label.clone(),
                SheetItemKind::File => "navigate".to_string(),
                SheetItemKind::Search => item.label.clone(),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{kind:<4} "), theme.status),
                Span::styled(action, theme.status),
                Span::styled(format!("  {}", item.detail), theme.status),
            ]))
        })
        .collect()
}

fn sheet_window(app: &App, visible_height: usize) -> (usize, usize) {
    let len = app.sheet.items.len();
    if len == 0 {
        return (0, 1);
    }

    let height = visible_height.min(len).max(1);
    let selected = app.sheet.selected.min(len - 1);
    let mut start = selected.saturating_sub(height.saturating_sub(1));
    if start + height > len {
        start = len - height;
    }

    (start, start + height)
}

fn empty_message(app: &App) -> &'static str {
    if app.command_line.trim().is_empty() {
        "Type a command, file, or search query"
    } else {
        "No matches"
    }
}
