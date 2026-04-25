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
mod create_swap;
mod input;
mod platform;
mod platform_bridge;
mod tui;
mod ui;

use actions::Action;
use app::{AppState, Tab};
use create_swap::CreateSwapMode;
use platform::{MemSnapshot, ProcessRow};
use platform_bridge::{PlatformBridge, PlatformCommand};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let mut backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let processes_active = Arc::new(AtomicBool::new(false));

    {
        let ram = backend.system_ram()?;
        let swap = backend.system_swap()?;
        let devices = backend.swap_devices()?;
        let snap = MemSnapshot {
            timestamp: Instant::now(),
            ram,
            swap,
            devices,
            processes: vec![],
        };
        state
            .lock()
            .expect("state mutex poisoned")
            .handle_action(Action::UpdateSnapshot(snap));
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

    let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();
    let bridge =
        PlatformBridge::spawn_with_backend(backend, action_tx, Arc::clone(&processes_active));

    let result = run(
        &mut terminal,
        state,
        &bridge,
        processes_active,
        shutdown,
        action_rx,
    )
    .await;
    bridge.send(PlatformCommand::Shutdown);
    tui::restore()?;
    result
}

async fn run(
    terminal: &mut tui::Tui,
    state: Arc<Mutex<AppState>>,
    bridge: &PlatformBridge,
    processes_active: Arc<AtomicBool>,
    shutdown: CancellationToken,
    mut action_rx: mpsc::UnboundedReceiver<Action>,
) -> Result<()> {
    let mut tick = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events = EventStream::new();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            Some(action) = action_rx.recv() => {
                state.lock().expect("state mutex poisoned").handle_action(action);
            }

            _ = tick.tick() => {
                bridge.send(PlatformCommand::Collect);
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    let ctx = {
                        let locked_state = state.lock().expect("state mutex poisoned");
                        input::KeyContext::from_state(&locked_state)
                    };

                    let selected_process_pid = {
                        let locked_state = state.lock().expect("state mutex poisoned");
                        let lower = locked_state.filter_text.to_lowercase();
                        let visible: Vec<&ProcessRow> = if lower.is_empty() {
                            locked_state.processes.iter().collect()
                        } else {
                            locked_state.processes.iter().filter(|process| {
                                process.name.to_lowercase().contains(&lower)
                                    || process.exe_path.as_ref().is_some_and(|path| path.to_lowercase().contains(&lower))
                            }).collect()
                        };
                        let clamped = locked_state.selected_row.min(visible.len().saturating_sub(1));
                        visible.get(clamped).map(|process| process.pid)
                    };

                    let action = input::resolve_key(key, &ctx);

                    let action = match action {
                        Some(Action::OpenProcessDetail { .. }) => {
                            selected_process_pid.map(|pid| Action::OpenProcessDetail { pid })
                        }
                        other => other,
                    };

                    let action = match action {
                        Some(Action::KillProcess { pid }) => {
                            bridge.send(PlatformCommand::KillProcess { pid });
                            None
                        }
                        other => other,
                    };

                    // Extract info before consuming action
                    let device_op_cmd = if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        Some((path.clone(), kind.clone()))
                    } else {
                        None
                    };
                    let submit_activate_only = if let Some(Action::CreateSwapSubmit { activate_only }) = &action {
                        Some(*activate_only)
                    } else {
                        None
                    };

                    // Send device op to bridge
                    if let Some((path, kind)) = device_op_cmd {
                        bridge.send(PlatformCommand::DeviceOp { path, kind });
                    }

                    // Dispatch to reducer
                    if let Some(a) = action {
                        let mut s = state.lock().expect("state mutex poisoned");
                        s.handle_action(a);

                        // After CreateSwapSubmit: send to bridge only if
                        // validation passed (mode transitioned to Progress)
                        if let Some(activate_only) = submit_activate_only
                            && let Some(modal) = s.create_swap_modal.as_ref()
                            && matches!(modal.mode, CreateSwapMode::Progress { .. })
                        {
                            let size_n: u64 = modal
                                .size_input.value().trim().parse()
                                .expect("validated by reducer");
                            let size_bytes = size_n * modal.size_unit.multiplier();
                            let prio_n: i32 = modal
                                .priority_input.value().trim().parse()
                                .expect("validated by reducer");
                            let prio_i16 =
                                prio_n.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                            bridge.send(PlatformCommand::CreateSwap {
                                path: std::path::PathBuf::from(
                                    modal.path_input.value(),
                                ),
                                size_bytes,
                                priority: prio_i16,
                                activate_after: modal.activate_after,
                                activate_only,
                            });
                        }

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
