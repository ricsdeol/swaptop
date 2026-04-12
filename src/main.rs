use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyModifiers};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod platform;
mod tui;
mod ui;

use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use app::{AppState, Tab};
use collector::Collector;
use platform::linux::LinuxBackend;
use platform::SwapBackend;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let mut col = Collector::new(backend);

    match col.collect().await {
        Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
        Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
    }

    let mut terminal = tui::init()?;

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
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events     = EventStream::new();

    // Channel for background tasks (spawn_blocking) to send actions back.
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            // Background task result (e.g. DeviceOpUpdate from swapon/swapoff)
            Some(action) = action_rx.recv() => {
                state.lock().expect("state mutex poisoned").handle_action(action);
            }

            _ = tick.tick() => {
                match col.collect().await {
                    Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
                    Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
                }
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    // Read tab-relevant state before dropping the lock
                    let (active_tab, confirm_action, selected_dev, has_devices) = {
                        let s = state.lock().expect("state mutex poisoned");
                        (
                            s.active_tab.clone(),
                            s.confirm_action.clone(),
                            s.selected_dev,
                            !s.devices.is_empty(),
                        )
                    };

                    let action: Option<Action> = match key.code {
                        // Global keys (always active, except 'r' which is overridden in Devices tab)
                        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
                        KeyCode::Tab     => Some(Action::NextTab),
                        KeyCode::BackTab => Some(Action::PrevTab),
                        KeyCode::Char('1') => Some(Action::SelectTab(1)),
                        KeyCode::Char('2') => Some(Action::SelectTab(2)),
                        KeyCode::Char('3') => Some(Action::SelectTab(3)),
                        KeyCode::Char('4') => Some(Action::SelectTab(4)),

                        // Tab-specific keys
                        _ => match active_tab {
                            Tab::Devices => handle_devices_key(
                                key.code,
                                confirm_action.as_ref(),
                                selected_dev,
                                has_devices,
                                &state,
                            ),
                            _ => match key.code {
                                KeyCode::Char('r') => Some(Action::Refresh),
                                _ => None,
                            },
                        },
                    };

                    // Spawn background task before dispatching ExecuteDeviceOp to AppState
                    if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        let tx   = action_tx.clone();
                        let path = path.clone();
                        let kind = kind.clone();
                        tokio::task::spawn_blocking(move || {
                            let backend = LinuxBackend::new();
                            let result = match kind {
                                DeviceOpKind::On    => backend.swap_on(&path),
                                DeviceOpKind::Off   => backend.swap_off(&path),
                                DeviceOpKind::Reset => backend.swap_reset(&path),
                            };
                            let status = match result {
                                Ok(_)  => OpStatus::Done,
                                Err(e) => OpStatus::Error(e.to_string()),
                            };
                            let _ = tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
                        });
                    }

                    if let Some(a) = action {
                        state.lock().expect("state mutex poisoned").handle_action(a);
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_devices_key(
    code: KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev: usize,
    has_devices: bool,
    state: &Arc<Mutex<AppState>>,
) -> Option<Action> {
    if let Some(kind) = confirm_action {
        // Modal is open — only 's'/Enter and Esc are active
        return match code {
            KeyCode::Char('s') | KeyCode::Enter => {
                let path = state
                    .lock()
                    .expect("state mutex poisoned")
                    .devices
                    .get(selected_dev)?
                    .path
                    .clone();
                Some(Action::ExecuteDeviceOp { path, kind: kind.clone() })
            }
            KeyCode::Esc => Some(Action::CancelConfirm),
            _ => None,
        };
    }

    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::DeviceDown),
        KeyCode::Char('k') | KeyCode::Up   => Some(Action::DeviceUp),
        KeyCode::Char('r') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Reset))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('o') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::On))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('f') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Off))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        _ => None,
    }
}
