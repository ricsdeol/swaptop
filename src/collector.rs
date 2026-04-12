use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;

use crate::platform::{MemSnapshot, SwapBackend};

pub struct Collector {
    backend: Box<dyn SwapBackend>,
    processes_active: Arc<AtomicBool>,
}

impl Collector {
    pub fn new(backend: Box<dyn SwapBackend>, processes_active: Arc<AtomicBool>) -> Self {
        Self {
            backend,
            processes_active,
        }
    }

    pub async fn collect(&mut self) -> Result<MemSnapshot> {
        let ram = self.backend.system_ram()?;
        let swap = self.backend.system_swap()?;
        let devices = self.backend.swap_devices()?;

        let processes = if self.processes_active.load(Ordering::Relaxed) {
            self.backend.process_list()?
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo};
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
                    user: "root".into(),
                    rss: 1024,
                    swap: 512,
                    cpu_pct: 0.5,
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

    impl SwapBackend for MockBackend {
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
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                can_swap_on: true,
                has_per_process: true,
            }
        }
    }

    #[tokio::test]
    async fn collect_assembles_snapshot_from_backend() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert_eq!(snap.ram.total, 16_000_000);
        assert_eq!(snap.swap.total, 4_000_000);
        assert!(snap.devices.is_empty());
    }

    #[tokio::test]
    async fn collect_skips_processes_when_inactive() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert!(snap.processes.is_empty());
    }

    #[tokio::test]
    async fn collect_includes_processes_when_active() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(true));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert_eq!(snap.processes.len(), 1);
        assert_eq!(snap.processes[0].name, "init");
    }

    #[tokio::test]
    async fn collect_propagates_backend_error() {
        let backend = MockBackend::failing();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let result = col.collect().await;
        assert!(result.is_err());
    }
}
