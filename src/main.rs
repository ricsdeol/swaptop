use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod create_swap;
mod input;
mod platform;
mod tui;
mod ui;

use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use app::{AppState, Tab};
use collector::Collector;
use create_swap::run_create_swap_steps;
use platform::SwapBackend;
use platform::linux::LinuxBackend;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let processes_active = Arc::new(AtomicBool::new(false));
    let mut col = Collector::new(backend, Arc::clone(&processes_active));

    // Initial collection before entering the TUI so the first frame is not blank.
    match col.collect().await {
        Ok(snap) => state
            .lock()
            .expect("state mutex poisoned")
            .handle_action(Action::UpdateSnapshot(snap)),
        Err(e) => {
            state.lock().expect("state mutex poisoned").error_msg =
                Some((e.to_string(), Instant::now()))
        }
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
    terminal: &mut tui::Tui,
    state: Arc<Mutex<AppState>>,
    col: &mut Collector,
    processes_active: Arc<AtomicBool>,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut tick = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events = EventStream::new();

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
                    Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some((e.to_string(), Instant::now())),
                }
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    // Read tab-relevant state before dropping the lock
                    let (active_tab, confirm_action, selected_dev, has_devices,
                         filter_mode, sort_col) = {
                        let s = state.lock().expect("state mutex poisoned");
                        (
                            s.active_tab.clone(),
                            s.confirm_action.clone(),
                            s.selected_dev,
                            !s.devices.is_empty(),
                            s.filter_mode,
                            s.sort_col,
                        )
                    };

                    let action = input::resolve_key(key, &input::KeyContext {
                        active_tab:     &active_tab,
                        confirm_action: confirm_action.as_ref(),
                        selected_dev,
                        has_devices,
                        filter_mode,
                        sort_col:       &sort_col,
                        state:          &state,
                    });

                    // Spawn background task before dispatching ExecuteDeviceOp to AppState
                    if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        let tx   = action_tx.clone();
                        let path = path.clone();
                        let kind = kind.clone();
                        tokio::task::spawn_blocking(move || {
                            let backend = LinuxBackend::new();
                            let result = match kind {
                                DeviceOpKind::On          => backend.swap_on(&path),
                                DeviceOpKind::Off         => backend.swap_off(&path),
                                DeviceOpKind::OffAndDelete => {
                                    backend.swap_off(&path).and_then(|()| {
                                        std::fs::remove_file(&path).map_err(|e| {
                                            color_eyre::eyre::eyre!(
                                                "deactivated; delete failed: {e}"
                                            )
                                        })
                                    })
                                }
                                DeviceOpKind::Reset       => backend.swap_reset(&path),
                            };
                            let status = match result {
                                Ok(_)  => OpStatus::Done,
                                Err(e) => OpStatus::Error(e.to_string()),
                            };
                            let _ = tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
                        });
                    }

                    // Phase 5 — spawn background create-swap task
                    if let Some(Action::CreateSwapSubmit { activate_only }) = action {
                        let submit = {
                            let s = state.lock().expect("state mutex poisoned");
                            s.create_swap_modal.as_ref().map(|m| {
                                let size_n: u64 = m.size_input.value().trim().parse().unwrap_or(0);
                                let size_bytes = size_n * m.size_unit.multiplier();
                                let prio_n: i32 = m.priority_input.value().trim().parse().unwrap_or(0);
                                (
                                    std::path::PathBuf::from(m.path_input.value()),
                                    size_bytes,
                                    prio_n as i16,
                                    m.activate_after,
                                )
                            })
                        };
                        if let Some((path, size_bytes, priority, activate_after)) = submit {
                            let tx = action_tx.clone();
                            tokio::task::spawn_blocking(move || {
                                run_create_swap_steps(
                                    path,
                                    size_bytes,
                                    priority,
                                    activate_after,
                                    activate_only,
                                    tx,
                                );
                            });
                        }
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
