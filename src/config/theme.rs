use std::env;

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub text: Color,
    pub muted: Color,
    pub heading: Style,
    pub link: Style,
    pub inline_code: Style,
    pub quote: Style,
    pub list_marker: Style,
    pub code_fence: Style,
    pub selection: Style,
    pub status: Style,
    pub dirty: Color,
}

impl Theme {
    pub fn system() -> Self {
        Self::terminal_monochrome()
    }

    #[cfg(test)]
    pub fn monochrome_for_tests() -> Self {
        Self::terminal_monochrome()
    }

    fn terminal_monochrome() -> Self {
        let background = Color::Reset;
        let text = Color::Reset;
        let muted = Color::DarkGray;
        let link = Color::Rgb(0, 81, 213);
        let selection_bg = Color::Gray;
        let selection_fg = Color::Black;
        let (status_bg, status_fg) = if is_light_terminal() {
            (Color::Black, Color::White)
        } else {
            (Color::White, Color::Black)
        };

        Self {
            background,
            text,
            muted,
            heading: Style::default().fg(text).add_modifier(Modifier::BOLD),
            link: Style::default().fg(link).add_modifier(Modifier::UNDERLINED),
            inline_code: Style::default().fg(text).add_modifier(Modifier::DIM),
            quote: Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            list_marker: Style::default().fg(muted),
            code_fence: Style::default().fg(muted),
            selection: Style::default().bg(selection_bg).fg(selection_fg),
            status: Style::default().bg(status_bg).fg(status_fg),
            dirty: Color::Red,
        }
    }
}

/// Detect whether the terminal appears to have a light background.
/// Uses the `COLORFGBG` environment variable set by many terminals (xterm, rxvt, etc.).
/// Defaults to `false` (dark terminal) when unsure.
fn is_light_terminal() -> bool {
    if let Ok(colorfgbg) = env::var("COLORFGBG") {
        if let Some(bg) = colorfgbg.split(';').nth(1) {
            if let Ok(bg_num) = bg.parse::<u8>() {
                return bg_num >= 7;
            }
        }
    }
    false
}
