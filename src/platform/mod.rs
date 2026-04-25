#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
pub mod bsd;
pub mod factory;
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub(crate) mod swap_discovery;
pub mod types;
#[cfg(target_os = "windows")]
pub mod windows;

pub use types::*;

use color_eyre::Result;
use std::path::Path;

pub trait PlatformProvider: Send + Sync {
    fn system_swap(&mut self) -> Result<SwapInfo>;
    fn system_ram(&mut self) -> Result<SwapInfo>;
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>>;
    fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
        Ok(vec![])
    }
    fn swap_on(&self, device: &Path) -> Result<()>;
    fn swap_off(&self, device: &Path) -> Result<()>;
    fn swap_reset(&self, device: &Path) -> Result<()> {
        self.swap_off(device)?;
        self.swap_on(device)
    }
    #[allow(dead_code)]
    fn kill_process(&self, _pid: u32) -> Result<()> {
        Err(color_eyre::eyre::eyre!(
            "kill_process not supported on this platform"
        ))
    }
    fn capabilities(&self) -> Capabilities;
    fn create_swap_file(
        &self,
        _path: std::path::PathBuf,
        _size_bytes: u64,
        _priority: i16,
        _activate_after: bool,
        _activate_only: bool,
    ) -> std::sync::mpsc::Receiver<CreateSwapProgress> {
        let (_tx, rx) = std::sync::mpsc::channel();
        rx
    }
}
