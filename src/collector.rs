use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;

use crate::platform::{MemSnapshot, SwapBackend};

pub struct Collector {
    backend:          Box<dyn SwapBackend>,
    processes_active: Arc<AtomicBool>,
}

impl Collector {
    pub fn new(backend: Box<dyn SwapBackend>, processes_active: Arc<AtomicBool>) -> Self {
        Self { backend, processes_active }
    }

    pub async fn collect(&mut self) -> Result<MemSnapshot> {
        let ram     = self.backend.system_ram()?;
        let swap    = self.backend.system_swap()?;
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
