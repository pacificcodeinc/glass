use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy)]
pub struct AppAreas {
    pub editor: Rect,
    pub sheet: Option<Rect>,
    pub status: Rect,
}

pub fn areas(area: Rect, sheet_height: Option<u16>) -> AppAreas {
    let constraints = if let Some(sheet_height) = sheet_height {
        vec![
            Constraint::Min(1),
            Constraint::Length(sheet_height),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Min(1), Constraint::Length(1)]
    };

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    if sheet_height.is_some() {
        AppAreas {
            editor: vertical[0],
            sheet: Some(vertical[1]),
            status: vertical[2],
        }
    } else {
        AppAreas {
            editor: vertical[0],
            sheet: None,
            status: vertical[1],
        }
    }
}
