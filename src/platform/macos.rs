use std::path::Path;

use color_eyre::Result;

use super::{Capabilities, SwapBackend, SwapDevice, SwapInfo};

pub struct MacosBackend;

impl MacosBackend {
    pub fn new() -> Self {
        Self
    }
}

impl SwapBackend for MacosBackend {
    fn system_ram(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("macOS backend not yet implemented")
    }
    fn system_swap(&mut self) -> Result<SwapInfo> {
        color_eyre::eyre::bail!("macOS backend not yet implemented")
    }
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        color_eyre::eyre::bail!("macOS backend not yet implemented")
    }
    fn swap_on(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("controlled by dynamic_pager on macOS")
    }
    fn swap_off(&self, _device: &Path) -> Result<()> {
        color_eyre::eyre::bail!("controlled by dynamic_pager on macOS")
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_swap_on:     false,
            has_per_process: false,
        }
    }
}
