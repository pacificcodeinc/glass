use std::path::PathBuf;

use anyhow::Result;
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier},
};

use crate::{app::App, ui};

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
    height: u16,
) -> Result<String> {
    let mut app = App::new(notes_dir, initial_file)?;
    let backend = TestBackend::new(width.max(1), height.max(1));
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| ui::draw(frame, &mut app))?;
    Ok(buffer_to_ansi(terminal.backend().buffer()))
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
    let base = if foreground { 30 } else { 40 };
    let bright_base = if foreground { 90 } else { 100 };
    match color {
        Color::Reset => format!("\x1b[{}m", if foreground { 39 } else { 49 }),
        Color::Black => format!("\x1b[{base}m"),
        Color::Red => format!("\x1b[{}m", base + 1),
        Color::Green => format!("\x1b[{}m", base + 2),
        Color::Yellow => format!("\x1b[{}m", base + 3),
        Color::Blue => format!("\x1b[{}m", base + 4),
        Color::Magenta => format!("\x1b[{}m", base + 5),
        Color::Cyan => format!("\x1b[{}m", base + 6),
        Color::Gray | Color::White => format!("\x1b[{}m", base + 7),
        Color::DarkGray => format!("\x1b[{bright_base}m"),
        Color::LightRed => format!("\x1b[{}m", bright_base + 1),
        Color::LightGreen => format!("\x1b[{}m", bright_base + 2),
        Color::LightYellow => format!("\x1b[{}m", bright_base + 3),
        Color::LightBlue => format!("\x1b[{}m", bright_base + 4),
        Color::LightMagenta => format!("\x1b[{}m", bright_base + 5),
        Color::LightCyan => format!("\x1b[{}m", bright_base + 6),
        Color::Rgb(r, g, b) => {
            format!("\x1b[{};2;{r};{g};{b}m", if foreground { 38 } else { 48 })
        }
        Color::Indexed(index) => {
            format!("\x1b[{};5;{index}m", if foreground { 38 } else { 48 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
