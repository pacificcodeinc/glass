pub mod editor;
pub mod layout;
pub mod status;

use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let theme = app.theme;
    let areas = layout::areas(frame.area());
    let editor_page = editor::page_area(areas.editor);
    app.resize_viewport(editor_page.height as usize, editor_page.width as usize);

    editor::render(frame, areas.editor, app, theme);
    status::render(frame, areas.status, app, theme);
}
