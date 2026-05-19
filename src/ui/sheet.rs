use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState},
};

use crate::{
    app::{App, CommandPrompt, SheetItemKind},
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
            let line = match app.sheet.prompt {
                CommandPrompt::File => file_item_line(item, theme),
                CommandPrompt::Palette => action_item_line(item, theme),
                CommandPrompt::Search => search_item_line(item, theme),
                CommandPrompt::Command => command_item_line(item, theme),
            };
            ListItem::new(line)
        })
        .collect()
}

fn file_item_line(item: &crate::app::SheetItem, theme: Theme) -> Line<'static> {
    if item.detail.is_empty() || item.detail == item.label {
        return Line::from(Span::styled(item.label.clone(), theme.status));
    }

    Line::from(vec![
        Span::styled(item.label.clone(), theme.status),
        Span::styled(format!("  {}", item.detail), theme.status),
    ])
}

fn action_item_line(item: &crate::app::SheetItem, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(item.label.clone(), theme.status),
        Span::styled(format!("  {}", item.detail), theme.status),
    ])
}

fn search_item_line(item: &crate::app::SheetItem, theme: Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(item.label.clone(), theme.status),
        Span::styled(format!("  {}", item.detail), theme.status),
    ])
}

fn command_item_line(item: &crate::app::SheetItem, theme: Theme) -> Line<'static> {
    match item.kind {
        SheetItemKind::Command => Line::from(vec![
            Span::styled(item.label.clone(), theme.status),
            Span::styled(format!("  {}", item.detail), theme.status),
        ]),
        SheetItemKind::File => file_item_line(item, theme),
        SheetItemKind::Search => search_item_line(item, theme),
    }
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
    if !app.command_line.trim().is_empty() {
        return "No matches";
    }

    match app.sheet.prompt {
        CommandPrompt::Command => "Type a command",
        CommandPrompt::File => "Type a file name",
        CommandPrompt::Palette => "Type an action",
        CommandPrompt::Search => "Type a search query",
    }
}
