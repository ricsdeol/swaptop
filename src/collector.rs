use color_eyre::Result;

use crate::platform::{MemSnapshot, SwapBackend};

pub struct Collector {
    backend: Box<dyn SwapBackend>,
}

impl Collector {
    pub fn new(backend: Box<dyn SwapBackend>) -> Self {
        Self { backend }
    }

    pub async fn collect(&mut self) -> Result<MemSnapshot> {
        let ram = self.backend.system_ram()?;
        let swap = self.backend.system_swap()?;
        let devices = self.backend.swap_devices()?;
        Ok(MemSnapshot {
            timestamp: std::time::Instant::now(),
            ram,
            swap,
            devices,
            processes: vec![], // Phase 2
        })
    }
}
