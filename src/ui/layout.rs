use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy)]
pub struct AppAreas {
    pub editor: Rect,
    pub status: Rect,
}

pub fn areas(area: Rect) -> AppAreas {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    AppAreas {
        editor: vertical[0],
        status: vertical[1],
    }
}
