use std::path::Path;

use color_eyre::Result;

use super::{Capabilities, SwapBackend, SwapDevice, SwapInfo};

pub struct BsdBackend;

impl BsdBackend {
    pub fn new() -> Self {
        Self
    }
}

impl SwapBackend for BsdBackend {
    fn system_ram(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("BSD backend not yet implemented")
    }
    fn system_swap(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("BSD backend not yet implemented")
    }
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        color_eyre::eyre::bail!("BSD backend not yet implemented")
    }
    fn swap_on(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("BSD backend not yet implemented")
    }
    fn swap_off(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("BSD backend not yet implemented")
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_swap_on:     true,
            has_per_process: false,
        }
    }
}
