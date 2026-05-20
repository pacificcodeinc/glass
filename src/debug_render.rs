use std::path::PathBuf;

use anyhow::Result;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

use crate::{
    app::App,
    editor::render::{visible_rows, wrap_line},
    markdown::{highlight::concealed_wrap_line, table::TableLayout},
    ui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellStyle {
    fg: Color,
    bg: Color,
    modifier: Modifier,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::empty(),
        }
    }
}

pub fn render_path_to_ansi(
    notes_dir: PathBuf,
    initial_file: Option<PathBuf>,
    width: u16,
    height: Option<u16>,
) -> Result<String> {
    let mut app = App::new(notes_dir, initial_file)?;
    let width = width.max(1);
    let height = height.unwrap_or_else(|| full_render_height(&app, width));
    let backend = TestBackend::new(width, height.max(2));
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, &mut app))?;
    Ok(buffer_to_ansi(terminal.backend().buffer()))
}

fn full_render_height(app: &App, width: u16) -> u16 {
    let editor_area = Rect {
        x: 0,
        y: 0,
        width,
        height: 1,
    };
    let page = ui::editor::page_area(editor_area);
    let gutter_width = (app.buffer.line_count().to_string().len() + 1).max(1);
    let text_width = page.width.saturating_sub(gutter_width as u16).max(1) as usize;
    let table_layout = TableLayout::new(&app.buffer);
    let rows = visible_rows(
        &app.buffer,
        0,
        0,
        usize::MAX,
        text_width,
        |line_num, text, w| {
            if line_num == app.cursor.line {
                wrap_line(text, w)
            } else if table_layout.is_table_row(line_num) {
                table_layout.wrap_line(line_num, text, w)
            } else {
                concealed_wrap_line(text, w)
            }
        },
    );
    rows.len().saturating_add(1).min(u16::MAX as usize) as u16
}

fn buffer_to_ansi(buffer: &ratatui::buffer::Buffer) -> String {
    let mut out = String::new();
    let area = buffer.area;
    for y in area.top()..area.bottom() {
        let mut current = CellStyle::default();
        for x in area.left()..area.right() {
            let Some(cell) = buffer.cell((x, y)) else {
                continue;
            };
            let next = CellStyle {
                fg: cell.fg,
                bg: cell.bg,
                modifier: cell.modifier,
            };
            if next != current {
                out.push_str(&style_ansi(next));
                current = next;
            }
            out.push_str(cell.symbol());
        }
        out.push_str("\x1b[0m");
        if y + 1 < area.bottom() {
            out.push('\n');
        }
    }
    out
}

fn style_ansi(style: CellStyle) -> String {
    let mut sequence = String::from("\x1b[0m");
    sequence.push_str(&fg_ansi(style.fg));
    sequence.push_str(&bg_ansi(style.bg));
    if style.modifier.contains(Modifier::BOLD) {
        sequence.push_str("\x1b[1m");
    }
    if style.modifier.contains(Modifier::DIM) {
        sequence.push_str("\x1b[2m");
    }
    if style.modifier.contains(Modifier::ITALIC) {
        sequence.push_str("\x1b[3m");
    }
    if style.modifier.contains(Modifier::UNDERLINED) {
        sequence.push_str("\x1b[4m");
    }
    if style.modifier.contains(Modifier::REVERSED) {
        sequence.push_str("\x1b[7m");
    }
    if style.modifier.contains(Modifier::CROSSED_OUT) {
        sequence.push_str("\x1b[9m");
    }
    sequence
}

fn fg_ansi(color: Color) -> String {
    color_ansi(color, true)
}

fn bg_ansi(color: Color) -> String {
    color_ansi(color, false)
}

fn color_ansi(color: Color, foreground: bool) -> String {
    let target = if foreground { 38 } else { 48 };
    match color {
        Color::Reset => format!("\x1b[{}m", if foreground { 39 } else { 49 }),
        Color::Black => indexed_color_ansi(target, 0),
        Color::Red => indexed_color_ansi(target, 1),
        Color::Green => indexed_color_ansi(target, 2),
        Color::Yellow => indexed_color_ansi(target, 3),
        Color::Blue => indexed_color_ansi(target, 4),
        Color::Magenta => indexed_color_ansi(target, 5),
        Color::Cyan => indexed_color_ansi(target, 6),
        Color::Gray => indexed_color_ansi(target, 7),
        Color::DarkGray => indexed_color_ansi(target, 8),
        Color::LightRed => indexed_color_ansi(target, 9),
        Color::LightGreen => indexed_color_ansi(target, 10),
        Color::LightYellow => indexed_color_ansi(target, 11),
        Color::LightBlue => indexed_color_ansi(target, 12),
        Color::LightMagenta => indexed_color_ansi(target, 13),
        Color::LightCyan => indexed_color_ansi(target, 14),
        Color::White => indexed_color_ansi(target, 15),
        Color::Rgb(r, g, b) => format!("\x1b[{target};2;{r};{g};{b}m"),
        Color::Indexed(index) => {
            format!("\x1b[{target};5;{index}m")
        }
    }
}

fn indexed_color_ansi(target: u8, index: u8) -> String {
    format!("\x1b[{target};5;{index}m")
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use std::fs;

    #[test]
    fn render_full_height_includes_entire_file_and_status_bar() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("glass-render-test-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let file = dir.join("note.md");
        fs::write(&file, "one\ntwo\nthree\n")?;

        let ansi = render_path_to_ansi(dir.clone(), Some(file), 80, None)?;

        assert!(ansi.contains("one"));
        assert!(ansi.contains("two"));
        assert!(ansi.contains("three"));
        assert!(ansi.contains(" NORMAL  note.md"));

        fs::remove_dir_all(dir).context("failed to clean render test directory")?;
        Ok(())
    }

    #[test]
    fn render_height_can_clip_document_but_keeps_status_bar() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("glass-render-clip-test-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let file = dir.join("note.md");
        fs::write(&file, "one\ntwo\nthree\n")?;

        let ansi = render_path_to_ansi(dir.clone(), Some(file), 80, Some(2))?;

        assert!(ansi.contains("one"));
        assert!(!ansi.contains("three"));
        assert!(ansi.contains(" NORMAL  note.md"));

        fs::remove_dir_all(dir).context("failed to clean render test directory")?;
        Ok(())
    }

    #[test]
    fn render_height_has_status_bar_even_when_too_small() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("glass-render-small-test-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let file = dir.join("note.md");
        fs::write(&file, "one\n")?;

        let ansi = render_path_to_ansi(dir.clone(), Some(file), 80, Some(1))?;

        assert!(ansi.contains(" NORMAL  note.md"));

        fs::remove_dir_all(dir).context("failed to clean render test directory")?;
        Ok(())
    }

    #[test]
    fn style_ansi_includes_rgb_and_modifiers() {
        let ansi = style_ansi(CellStyle {
            fg: Color::Rgb(1, 2, 3),
            bg: Color::Reset,
            modifier: Modifier::BOLD | Modifier::UNDERLINED,
        });

        assert!(ansi.contains("\x1b[38;2;1;2;3m"));
        assert!(ansi.contains("\x1b[1m"));
        assert!(ansi.contains("\x1b[4m"));
    }

    #[test]
    fn named_colors_match_crossterm_palette_indices() {
        let ansi = style_ansi(CellStyle {
            fg: Color::Black,
            bg: Color::White,
            modifier: Modifier::empty(),
        });

        assert!(ansi.contains("\x1b[38;5;0m"));
        assert!(ansi.contains("\x1b[48;5;15m"));
    }
}
