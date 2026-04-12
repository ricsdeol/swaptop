use std::path::{Path, PathBuf};

use color_eyre::Result;
use sysinfo::System;

use super::proc_reader::ProcReader;
use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};

pub struct LinuxBackend {
    sys:         System,
    proc_reader: ProcReader,
}

impl LinuxBackend {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys, proc_reader: ProcReader::new() }
    }
}

impl SwapBackend for LinuxBackend {
    fn system_ram(&mut self) -> Result<SwapInfo> {
        self.sys.refresh_memory();
        Ok(SwapInfo::new(self.sys.total_memory(), self.sys.used_memory()))
    }

    fn system_swap(&mut self) -> Result<SwapInfo> {
        self.sys.refresh_memory();
        Ok(SwapInfo::new(self.sys.total_swap(), self.sys.used_swap()))
    }

    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        let content = std::fs::read_to_string("/proc/swaps")?;
        Ok(parse_proc_swaps(&content))
    }

    fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
        Ok(self.proc_reader.collect())
    }

    fn swap_on(&self, device: &Path) -> Result<()> {
        let path = std::ffi::CString::new(device.to_string_lossy().as_bytes())
            .map_err(|e| color_eyre::eyre::eyre!("invalid path: {e}"))?;
        // SAFETY: path is a valid NUL-terminated C string; 0 = no special flags.
        let ret = unsafe { nix::libc::swapon(path.as_ptr(), 0) };
        if ret == 0 {
            Ok(())
        } else {
            Err(color_eyre::eyre::eyre!(
                "swapon failed: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    fn swap_off(&self, device: &Path) -> Result<()> {
        let path = std::ffi::CString::new(device.to_string_lossy().as_bytes())
            .map_err(|e| color_eyre::eyre::eyre!("invalid path: {e}"))?;
        // SAFETY: path is a valid NUL-terminated C string.
        let ret = unsafe { nix::libc::swapoff(path.as_ptr()) };
        if ret == 0 {
            Ok(())
        } else {
            Err(color_eyre::eyre::eyre!(
                "swapoff failed: {}",
                std::io::Error::last_os_error()
            ))
        }
    }

    fn swap_reset(&self, device: &Path) -> Result<()> {
        self.swap_off(device)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.swap_on(device)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_swap_on:     true,
            can_swap_off:    true,
            has_per_process: true,
            has_device_list: true,
            can_create_swap: true,
            requires_root:   true,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn is_kernel_thread(name: &str) -> bool {
    name.starts_with('[') && name.ends_with(']')
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse the contents of `/proc/swaps` into a list of `SwapDevice`s.
///
/// Exposed as `pub(crate)` so it can be unit-tested without touching the
/// filesystem or requiring a real `LinuxBackend`.
pub(crate) fn parse_proc_swaps(content: &str) -> Vec<SwapDevice> {
    content.lines().skip(1).filter_map(parse_swap_line).collect()
}

fn parse_swap_line(line: &str) -> Option<SwapDevice> {
    let mut parts = line.split_whitespace();
    let raw_path = parts.next()?;
    let type_str = parts.next()?;
    let total_kb = parts.next()?.parse::<u64>().ok()?;
    let used_kb  = parts.next()?.parse::<u64>().ok()?;
    let priority = parts.next()?.parse::<i16>().ok()?;

    let path = PathBuf::from(raw_path);
    let kind = if raw_path.contains("zram") {
        SwapKind::Zram
    } else {
        match type_str {
            "partition" => SwapKind::Partition,
            _           => SwapKind::File,
        }
    };

    Some(SwapDevice {
        path,
        total: total_kb * 1024,
        used:  used_kb  * 1024,
        priority,
        kind,
        active: true,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n";

    #[test]
    fn parse_returns_empty_for_header_only() {
        let devices = parse_proc_swaps(HEADER);
        assert!(devices.is_empty());
    }

    #[test]
    fn parse_returns_empty_for_blank_content() {
        assert!(parse_proc_swaps("").is_empty());
    }

    #[test]
    fn parse_detects_partition_kind() {
        let content = format!("{HEADER}/dev/sda2\t\tpartition\t4194300\t102400\t-1\n");
        let devices = parse_proc_swaps(&content);
        assert_eq!(devices.len(), 1);
        assert!(matches!(devices[0].kind, SwapKind::Partition));
        assert_eq!(devices[0].priority, -1);
    }

    #[test]
    fn parse_converts_kilobytes_to_bytes() {
        let content = format!("{HEADER}/swapfile\t\tfile\t2097152\t512000\t0\n");
        let devices = parse_proc_swaps(&content);
        assert_eq!(devices[0].total, 2_097_152 * 1024);
        assert_eq!(devices[0].used,  512_000   * 1024);
    }

    #[test]
    fn parse_detects_zram_kind() {
        let content = format!("{HEADER}/dev/zram0\t\tpartition\t524284\t0\t100\n");
        let devices = parse_proc_swaps(&content);
        assert_eq!(devices.len(), 1);
        assert!(matches!(devices[0].kind, SwapKind::Zram));
    }

    #[test]
    fn parse_detects_file_kind() {
        let content = format!("{HEADER}/swapfile\t\tfile\t1048572\t0\t0\n");
        let devices = parse_proc_swaps(&content);
        assert!(matches!(devices[0].kind, SwapKind::File));
    }

    #[test]
    fn parse_skips_malformed_lines() {
        let content = format!("{HEADER}bad line here\n");
        assert!(parse_proc_swaps(&content).is_empty());
    }

    #[test]
    fn parse_handles_multiple_devices() {
        let content = format!(
            "{HEADER}\
             /dev/sda2\t\tpartition\t4194300\t102400\t-1\n\
             /swapfile\t\tfile\t2097152\t0\t0\n\
             /dev/zram0\t\tpartition\t524284\t200000\t100\n"
        );
        let devices = parse_proc_swaps(&content);
        assert_eq!(devices.len(), 3);
    }

    #[test]
    fn kernel_thread_filter_matches_bracketed_names() {
        assert!(is_kernel_thread("[kworker/0:0]"));
        assert!(is_kernel_thread("[migration/0]"));
        assert!(is_kernel_thread("[kswapd0]"));
    }

    #[test]
    fn kernel_thread_filter_rejects_regular_processes() {
        assert!(!is_kernel_thread("firefox"));
        assert!(!is_kernel_thread("kswapd0"));
        assert!(!is_kernel_thread("[incomplete"));
        assert!(!is_kernel_thread("trailing]"));
    }
}
