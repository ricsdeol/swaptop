use color_eyre::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::Stdout;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> Result<Tui> {
    // Ensure terminal is always restored even if a panic occurs mid-render.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_raw();
        original_hook(info);
    }));

    enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(std::io::stdout())).map_err(Into::into)
}

pub fn restore() -> Result<()> {
    restore_raw().map_err(Into::into)
}

fn restore_raw() -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)
}
