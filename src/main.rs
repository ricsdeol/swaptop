use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
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

use actions::{Action, DeviceOp, DeviceOpKind, OpStatus, SortColumn};
use app::{AppState, Tab};
use collector::Collector;
use platform::linux::LinuxBackend;
use platform::SwapBackend;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend          = platform::factory::detect();
    let caps             = backend.capabilities();
    let state            = Arc::new(Mutex::new(AppState::new(caps)));
    let processes_active = Arc::new(AtomicBool::new(false));
    let mut col          = Collector::new(backend, Arc::clone(&processes_active));

    // Initial collection before entering the TUI so the first frame is not blank.
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

    let result = run(&mut terminal, state, &mut col, processes_active, shutdown).await;
    tui::restore()?;
    result
}

async fn run(
    terminal:         &mut tui::Tui,
    state:            Arc<Mutex<AppState>>,
    col:              &mut Collector,
    processes_active: Arc<AtomicBool>,
    shutdown:         CancellationToken,
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
                    let (active_tab, confirm_action, selected_dev, has_devices, filter_mode,
                         sort_col, filter_active) = {
                        let s = state.lock().expect("state mutex poisoned");
                        (
                            s.active_tab.clone(),
                            s.confirm_action.clone(),
                            s.selected_dev,
                            !s.devices.is_empty(),
                            s.filter_mode,
                            s.sort_col,
                            s.filter_mode,
                        )
                    };

                    let action: Option<Action> = resolve_key_with_context(
                        key,
                        &active_tab,
                        confirm_action.as_ref(),
                        selected_dev,
                        has_devices,
                        filter_mode,
                        &sort_col,
                        filter_active,
                        &state,
                    );

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
                        let mut s = state.lock().expect("state mutex poisoned");
                        s.handle_action(a);
                        processes_active.store(
                            s.active_tab == Tab::Processes,
                            Ordering::Relaxed,
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_key_with_context(
    key:            crossterm::event::KeyEvent,
    active_tab:     &Tab,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    filter_mode:    bool,
    sort_col:       &SortColumn,
    _filter_active: bool,
    state:          &Arc<Mutex<AppState>>,
) -> Option<Action> {
    // Priority 1: filter input captures almost all keys
    if filter_mode {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::ExitFilterMode),
            KeyCode::Backspace            => Some(Action::FilterBackspace),
            KeyCode::Char(c)              => Some(Action::FilterChar(c)),
            _                             => None,
        };
    }

    // Global keys (always active)
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Action::Quit),
        KeyCode::Tab     => return Some(Action::NextTab),
        KeyCode::BackTab => return Some(Action::PrevTab),
        KeyCode::Char('1') => return Some(Action::SelectTab(1)),
        KeyCode::Char('2') => return Some(Action::SelectTab(2)),
        KeyCode::Char('3') => return Some(Action::SelectTab(3)),
        KeyCode::Char('4') => return Some(Action::SelectTab(4)),
        _ => {}
    }

    // Tab-specific keys
    match active_tab {
        Tab::Processes => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down  => return Some(Action::NavigateDown),
                KeyCode::Char('k') | KeyCode::Up    => return Some(Action::NavigateUp),
                KeyCode::Char('s')                  => {
                    return Some(Action::SortBy(next_sort_column(sort_col)));
                }
                KeyCode::Char('/')                  => return Some(Action::EnterFilterMode),
                KeyCode::Char('r')                  => return Some(Action::Refresh),
                _ => {}
            }
        }
        Tab::Devices => {
            return handle_devices_key(
                key.code,
                confirm_action,
                selected_dev,
                has_devices,
                state,
            );
        }
        _ => {
            if let KeyCode::Char('r') = key.code {
                return Some(Action::Refresh);
            }
        }
    }

    None
}

fn handle_devices_key(
    code:           KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    state:          &Arc<Mutex<AppState>>,
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

fn next_sort_column(current: &SortColumn) -> SortColumn {
    match current {
        SortColumn::Swap => SortColumn::Cpu,
        SortColumn::Cpu  => SortColumn::Rss,
        SortColumn::Rss  => SortColumn::Pid,
        SortColumn::Pid  => SortColumn::Name,
        SortColumn::Name | SortColumn::User => SortColumn::Swap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_column_cycles_through_all_columns() {
        assert_eq!(next_sort_column(&SortColumn::Swap), SortColumn::Cpu);
        assert_eq!(next_sort_column(&SortColumn::Cpu),  SortColumn::Rss);
        assert_eq!(next_sort_column(&SortColumn::Rss),  SortColumn::Pid);
        assert_eq!(next_sort_column(&SortColumn::Pid),  SortColumn::Name);
        assert_eq!(next_sort_column(&SortColumn::Name), SortColumn::Swap);
    }

    #[test]
    fn user_column_falls_back_to_swap() {
        assert_eq!(next_sort_column(&SortColumn::User), SortColumn::Swap);
    }
}
