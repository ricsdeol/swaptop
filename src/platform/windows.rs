use std::path::Path;

use color_eyre::Result;

use super::{Capabilities, PlatformProvider, SwapDevice, SwapInfo};

pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> Self {
        Self
    }
}

impl PlatformProvider for WindowsBackend {
    fn system_ram(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("Windows backend not yet implemented")
    }
    fn system_swap(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("Windows backend not yet implemented")
    }
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        color_eyre::eyre::bail!("Windows backend not yet implemented")
    }
    fn swap_on(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("not supported on Windows")
    }
    fn swap_off(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("not supported on Windows")
    }
    fn kill_process(&self, _pid: u32) -> Result<()> {
        Err(color_eyre::eyre::eyre!("kill_process not supported on Windows"))
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_swap_on: false,
            has_per_process: false,
        }
    }
}
