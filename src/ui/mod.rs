pub mod editor;
pub mod layout;
pub mod sheet;
pub mod status;

use ratatui::Frame;

use crate::app::{App, Mode};

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let sheet_height = if app.mode == Mode::CommandLine {
        let height = app.sheet_panel_height(frame.area().height);
        (height > 0).then_some(height)
    } else {
        None
    };
    let areas = layout::areas(frame.area(), sheet_height);
    let editor_theme = app.theme;
    let editor_page = editor::page_area(areas.editor);
    app.resize_viewport(editor_page.height as usize, editor_page.width as usize);

    editor::render(frame, areas.editor, app, editor_theme);
    if let Some(sheet_area) = areas.sheet {
        sheet::render(frame, sheet_area, app, app.theme);
    }
    status::render(frame, areas.status, app, app.theme);
}
