use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyModifiers};
use futures::{FutureExt, StreamExt};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod platform;
mod tui;
mod ui;

use actions::Action;
use app::AppState;
use collector::Collector;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let mut col = Collector::new(backend);

    // Initial collection before entering the TUI so the first frame is not blank.
    match col.collect().await {
        Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
        Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
    }

    let mut terminal = tui::init()?;

    // CancellationToken bridges OS signals (SIGINT/Ctrl-C from the shell) with
    // the same graceful-shutdown path used by the keyboard 'q' handler.
    let shutdown = CancellationToken::new();
    {
        let token = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                token.cancel();
            }
        });
    }

    let result = run(&mut terminal, state, &mut col, shutdown).await;
    tui::restore()?;
    result
}

async fn run(
    terminal: &mut tui::Tui,
    state: Arc<Mutex<AppState>>,
    col: &mut Collector,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut tick       = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33)); // ~30 fps
    let mut events     = EventStream::new();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            // OS signal or token.cancel() called
            _ = shutdown.cancelled() => break,

            // Data tick — collect a MemSnapshot and push it into AppState
            _ = tick.tick() => {
                match col.collect().await {
                    Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
                    Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
                }
            }

            // Frame tick — redraw at ~30 fps
            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            // Keyboard input — map to Actions
            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    let action = match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
                        KeyCode::Char('r') => Some(Action::Refresh),
                        KeyCode::Tab       => Some(Action::NextTab),
                        KeyCode::BackTab   => Some(Action::PrevTab),
                        KeyCode::Char('1') => Some(Action::SelectTab(1)),
                        KeyCode::Char('2') => Some(Action::SelectTab(2)),
                        KeyCode::Char('3') => Some(Action::SelectTab(3)),
                        KeyCode::Char('4') => Some(Action::SelectTab(4)),
                        _ => None,
                    };
                    if let Some(a) = action {
                        state.lock().expect("state mutex poisoned").handle_action(a);
                    }
                }
            }
        }
    }

    Ok(())
}
