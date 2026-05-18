use std::{
    io::{self, Stdout},
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event,
    event::{
        DisableMouseCapture, EnableMouseCapture, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    app::{App, Mode},
    ui,
};

pub struct TerminalSession;

impl TerminalSession {
    pub fn run(mut app: App) -> Result<()> {
        let mut terminal = setup_terminal()?;
        let result = run_loop(&mut terminal, &mut app);
        restore_terminal(&mut terminal)?;
        result
    }
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        SetCursorStyle::DefaultUserShape,
        PopKeyboardEnhancementFlags,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;
        execute!(terminal.backend_mut(), cursor_style(app.mode))?;

        if event::poll(Duration::from_millis(16))? {
            app.handle_event(event::read()?)?;
        }
    }

    Ok(())
}

fn cursor_style(mode: Mode) -> SetCursorStyle {
    match mode {
        Mode::Normal => SetCursorStyle::SteadyBlock,
        Mode::Insert => SetCursorStyle::BlinkingBar,
        Mode::CommandLine => SetCursorStyle::BlinkingUnderScore,
        Mode::Visual => SetCursorStyle::SteadyUnderScore,
    }
}
