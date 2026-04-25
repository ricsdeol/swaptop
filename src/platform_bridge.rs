use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use crate::platform::{MemSnapshot, PlatformProvider};

pub enum PlatformCommand {
    Collect,
    DeviceOp {
        path: PathBuf,
        kind: DeviceOpKind,
    },
    CreateSwap {
        path: PathBuf,
        size_bytes: u64,
        priority: i16,
        activate_after: bool,
        activate_only: bool,
    },
    #[allow(dead_code)] // wired in Task 10
    KillProcess { pid: u32 },
    Shutdown,
}

pub struct PlatformBridge {
    cmd_tx: std::sync::mpsc::Sender<PlatformCommand>,
}

impl PlatformBridge {
    pub fn spawn_with_backend(
        mut backend: Box<dyn PlatformProvider>,
        action_tx: UnboundedSender<Action>,
        processes_active: Arc<AtomicBool>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    PlatformCommand::Collect => {
                        let _ = action_tx.send(Action::CollectStarted);
                        Self::handle_collect(&mut *backend, &action_tx, &processes_active);
                        let _ = action_tx.send(Action::CollectFinished);
                    }
                    PlatformCommand::DeviceOp { path, kind } => {
                        Self::handle_device_op(&mut *backend, &action_tx, path, kind);
                    }
                    PlatformCommand::CreateSwap {
                        path,
                        size_bytes,
                        priority,
                        activate_after,
                        activate_only,
                    } => {
                        let rx = backend.create_swap_file(
                            path,
                            size_bytes,
                            priority,
                            activate_after,
                            activate_only,
                        );
                        let tx = action_tx.clone();
                        std::thread::spawn(move || {
                            while let Ok(progress) = rx.recv() {
                                let _ = tx.send(Action::CreateSwapProgress(progress));
                            }
                        });
                    }
                    PlatformCommand::KillProcess { pid } => {
                        let result = backend.kill_process(pid);
                        let action = match result {
                            Ok(()) => Action::KillProcessResult { pid, success: true, msg: None },
                            Err(e) => Action::KillProcessResult { pid, success: false, msg: Some(e.to_string()) },
                        };
                        let _ = action_tx.send(action);
                    }
                    PlatformCommand::Shutdown => break,
                }
            }
        });
        Self { cmd_tx }
    }

    pub fn send(&self, cmd: PlatformCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn handle_collect(
        backend: &mut dyn PlatformProvider,
        action_tx: &UnboundedSender<Action>,
        processes_active: &AtomicBool,
    ) {
        let result: color_eyre::Result<MemSnapshot> = (|| {
            let ram = backend.system_ram()?;
            let swap = backend.system_swap()?;
            let devices = backend.swap_devices()?;
            let processes = if processes_active.load(Ordering::Relaxed) {
                backend.process_list()?
            } else {
                vec![]
            };
            Ok(MemSnapshot {
                timestamp: std::time::Instant::now(),
                ram,
                swap,
                devices,
                processes,
            })
        })();
        match result {
            Ok(snap) => {
                let _ = action_tx.send(Action::UpdateSnapshot(snap));
            }
            Err(e) => {
                let _ = action_tx.send(Action::SetError(e.to_string()));
            }
        }
    }

    fn handle_device_op(
        backend: &mut dyn PlatformProvider,
        action_tx: &UnboundedSender<Action>,
        path: PathBuf,
        kind: DeviceOpKind,
    ) {
        let result = match kind {
            DeviceOpKind::On => backend.swap_on(&path),
            DeviceOpKind::Off => backend.swap_off(&path),
            DeviceOpKind::OffAndDelete => backend.swap_off(&path).and_then(|()| {
                std::fs::remove_file(&path)
                    .map_err(|e| color_eyre::eyre::eyre!("deactivated; delete failed: {e}"))
            }),
            DeviceOpKind::DeleteOnly => std::fs::remove_file(&path)
                .map_err(|e| color_eyre::eyre::eyre!("delete failed: {e}")),
            DeviceOpKind::Reset => backend.swap_reset(&path),
        };
        let status = match result {
            Ok(_) => OpStatus::Done,
            Err(e) => OpStatus::Error(e.to_string()),
        };
        let _ = action_tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Capabilities, PlatformProvider, ProcessRow, SwapDevice, SwapInfo};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    struct MockBackend {
        ram: SwapInfo,
        swap: SwapInfo,
        devices: Vec<SwapDevice>,
        processes: Vec<ProcessRow>,
        fail: bool,
    }

    impl MockBackend {
        fn healthy() -> Self {
            Self {
                ram: SwapInfo::new(16_000_000, 8_000_000),
                swap: SwapInfo::new(4_000_000, 1_000_000),
                devices: vec![],
                processes: vec![ProcessRow {
                    pid: 1,
                    name: "init".into(),
                    exe_path: None,
                    user: "root".into(),
                    rss: 1024,
                    swap: 512,
                    cpu_pct: 0.5,
                    threads: 1,
                    status: 'S',
                }],
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::healthy()
            }
        }
    }

    impl PlatformProvider for MockBackend {
        fn system_ram(&mut self) -> color_eyre::Result<SwapInfo> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock ram error"));
            }
            Ok(self.ram.clone())
        }
        fn system_swap(&mut self) -> color_eyre::Result<SwapInfo> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock swap error"));
            }
            Ok(self.swap.clone())
        }
        fn swap_devices(&mut self) -> color_eyre::Result<Vec<SwapDevice>> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock devices error"));
            }
            Ok(self.devices.clone())
        }
        fn process_list(&mut self) -> color_eyre::Result<Vec<ProcessRow>> {
            Ok(self.processes.clone())
        }
        fn swap_on(&self, _device: &Path) -> color_eyre::Result<()> {
            Ok(())
        }
        fn swap_off(&self, _device: &Path) -> color_eyre::Result<()> {
            Ok(())
        }
        fn kill_process(&self, _pid: u32) -> color_eyre::Result<()> {
            Ok(())
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                can_swap_on: true,
                has_per_process: true,
            }
        }
    }

    fn recv_action(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Action>) -> Action {
        rx.blocking_recv()
            .expect("channel closed before action received")
    }

    #[test]
    fn collect_sends_update_snapshot() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        // CollectStarted arrives first; skip it to reach the payload
        let _started = recv_action(&mut action_rx);
        let action = recv_action(&mut action_rx);
        assert!(matches!(action, Action::UpdateSnapshot(_)));
    }

    #[test]
    fn collect_includes_processes_when_active() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(true));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        // CollectStarted arrives first; skip it to reach the payload
        let _started = recv_action(&mut action_rx);
        let action = recv_action(&mut action_rx);
        if let Action::UpdateSnapshot(snap) = action {
            assert_eq!(snap.processes.len(), 1);
        } else {
            panic!("expected UpdateSnapshot");
        }
    }

    #[test]
    fn collect_skips_processes_when_inactive() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        // CollectStarted arrives first; skip it to reach the payload
        let _started = recv_action(&mut action_rx);
        let action = recv_action(&mut action_rx);
        if let Action::UpdateSnapshot(snap) = action {
            assert!(snap.processes.is_empty());
        } else {
            panic!("expected UpdateSnapshot");
        }
    }

    #[test]
    fn collect_error_sends_set_error() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::failing()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        // CollectStarted arrives first; skip it to reach the payload
        let _started = recv_action(&mut action_rx);
        let action = recv_action(&mut action_rx);
        assert!(matches!(action, Action::SetError(_)));
    }

    #[test]
    fn device_op_sends_update() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::DeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::On,
        });
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        let action = recv_action(&mut action_rx);
        if let Action::DeviceOpUpdate(op) = action {
            assert_eq!(op.status, OpStatus::Done);
            assert_eq!(op.kind, DeviceOpKind::On);
        } else {
            panic!("expected DeviceOpUpdate, got {action:?}");
        }
    }

    #[test]
    fn shutdown_exits_thread() {
        let (action_tx, _action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
    }

    #[test]
    fn collect_emits_started_and_finished_around_snapshot() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let first = recv_action(&mut action_rx);
        assert!(
            matches!(first, Action::CollectStarted),
            "expected CollectStarted, got {first:?}"
        );
        let second = recv_action(&mut action_rx);
        assert!(
            matches!(second, Action::UpdateSnapshot(_)),
            "expected UpdateSnapshot, got {second:?}"
        );
        let third = recv_action(&mut action_rx);
        assert!(
            matches!(third, Action::CollectFinished),
            "expected CollectFinished, got {third:?}"
        );
    }

    #[test]
    fn collect_error_still_emits_finished() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::failing()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let first = recv_action(&mut action_rx);
        assert!(
            matches!(first, Action::CollectStarted),
            "expected CollectStarted, got {first:?}"
        );
        let second = recv_action(&mut action_rx);
        assert!(
            matches!(second, Action::SetError(_)),
            "expected SetError, got {second:?}"
        );
        let third = recv_action(&mut action_rx);
        assert!(
            matches!(third, Action::CollectFinished),
            "expected CollectFinished, got {third:?}"
        );
    }

    #[test]
    fn kill_process_sends_result() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );
        bridge.send(PlatformCommand::KillProcess { pid: 1234 });
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        if let Action::KillProcessResult { pid, success, msg } = action {
            assert_eq!(pid, 1234);
            assert!(success);
            assert!(msg.is_none());
        } else {
            panic!("expected KillProcessResult, got {action:?}");
        }
    }

    #[test]
    fn create_swap_does_not_block_collect() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::CreateSwap {
            path: "/tmp/nonexistent_swaptest".into(),
            size_bytes: 1024,
            priority: -1,
            activate_after: false,
            activate_only: false,
        });
        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let mut found_snapshot = false;
        while let Some(action) = action_rx.blocking_recv() {
            if matches!(action, Action::UpdateSnapshot(_)) {
                found_snapshot = true;
                break;
            }
        }
        assert!(
            found_snapshot,
            "Collect should not be blocked by CreateSwap"
        );
    }
}
