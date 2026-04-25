pub(crate) mod create_swap;
mod proc_reader;

use std::path::{Path, PathBuf};

use color_eyre::Result;
use sysinfo::System;

use super::swap_discovery::discover_inactive_swap_files;
use super::{
    Capabilities, PlatformProvider, ProcessRow, SwapDevice, SwapInfo, SwapKind, parse_swap_header,
};
use proc_reader::ProcReader;

const LINUX_SCAN_DIRS: &[(&str, &[&str])] = &[
    ("/", &["swap*", "*.swap", "*.img"]),
    ("/var", &["swap*", "*.swap"]),
    ("/mnt", &["swap*", "*.swap"]),
];

pub struct LinuxBackend {
    sys: System,
    proc_reader: ProcReader,
}

impl LinuxBackend {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            proc_reader: ProcReader::new(),
        }
    }
}

impl PlatformProvider for LinuxBackend {
    fn system_ram(&mut self) -> Result<SwapInfo> {
        self.sys.refresh_memory();
        Ok(SwapInfo::new(
            self.sys.total_memory(),
            self.sys.used_memory(),
        ))
    }

    fn system_swap(&mut self) -> Result<SwapInfo> {
        self.sys.refresh_memory();
        Ok(SwapInfo::new(self.sys.total_swap(), self.sys.used_swap()))
    }

    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        let content = std::fs::read_to_string("/proc/swaps")?;
        let mut devices = parse_proc_swaps(&content);

        let active_paths: std::collections::HashSet<PathBuf> = devices
            .iter()
            .flat_map(|d| {
                let canonical = std::fs::canonicalize(&d.path).ok();
                std::iter::once(d.path.clone()).chain(canonical)
            })
            .collect();

        devices.extend(discover_inactive_swap_files(&active_paths, LINUX_SCAN_DIRS));

        // Probe block devices in /dev/ for inactive swap partitions
        if let Ok(entries) = std::fs::read_dir("/dev/") {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if active_paths.contains(&path) {
                    continue;
                }
                if let Some(dev) = probe_swap_device(&path) {
                    devices.push(dev);
                }
            }
        }

        Ok(devices)
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
        // NOTE: This runs inside spawn_blocking, so std::thread::sleep is
        // appropriate here. The PlatformProvider trait is synchronous by design.
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.swap_on(device)
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            can_swap_on: true,
            has_per_process: true,
        }
    }

    fn create_swap_file(
        &self,
        path: std::path::PathBuf,
        size_bytes: u64,
        priority: i16,
        activate_after: bool,
        activate_only: bool,
        on_progress: Box<dyn Fn(super::CreateSwapProgress) + Send>,
    ) {
        std::thread::spawn(move || {
            create_swap::run_create_swap_steps(
                path,
                size_bytes,
                priority,
                activate_after,
                activate_only,
                &on_progress,
            );
        });
    }
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse the contents of `/proc/swaps` into a list of `SwapDevice`s.
///
/// Exposed as `pub(crate)` so it can be unit-tested without touching the
/// filesystem or requiring a real `LinuxBackend`.
pub(crate) fn parse_proc_swaps(content: &str) -> Vec<SwapDevice> {
    content
        .lines()
        .skip(1)
        .filter_map(parse_swap_line)
        .collect()
}

fn parse_swap_line(line: &str) -> Option<SwapDevice> {
    let mut parts = line.split_whitespace();
    let raw_path = parts.next()?;
    let type_str = parts.next()?;
    let total_kb = parts.next()?.parse::<u64>().ok()?;
    let used_kb = parts.next()?.parse::<u64>().ok()?;
    let priority = parts.next()?.parse::<i16>().ok()?;

    let path = PathBuf::from(raw_path);
    let kind = if raw_path.contains("zram") {
        SwapKind::Zram
    } else {
        match type_str {
            "partition" => SwapKind::Partition,
            _ => SwapKind::File,
        }
    };

    Some(SwapDevice {
        path,
        total: total_kb * 1024,
        used: used_kb * 1024,
        priority,
        kind,
        active: true,
    })
}

/// Check if `path` is a block device with swap magic header.
/// Returns `None` silently on any I/O or permission error.
fn probe_swap_device(path: &Path) -> Option<SwapDevice> {
    if !is_block_device(path) {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    // `metadata().len()` returns 0 for block devices on Linux, so query the
    // real size via the BLKGETSIZE64 ioctl on the open fd.
    let size = block_device_size(&f)?;
    parse_swap_header(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::Partition,
        active: false,
    })
}

/// Return the size in bytes of a block device via `BLKGETSIZE64`.
fn block_device_size(file: &std::fs::File) -> Option<u64> {
    use std::os::unix::io::AsRawFd;
    // BLKGETSIZE64 = _IOR(0x12, 114, size_t) = 0x80081272 on Linux.
    const BLKGETSIZE64: nix::libc::c_ulong = 0x8008_1272;
    let fd = file.as_raw_fd();
    let mut size: u64 = 0;
    // SAFETY: `fd` is a valid open file descriptor for a block device;
    // `size` is a valid mutable u64 pointer matching the ioctl's size_t out-param.
    let ret = unsafe { nix::libc::ioctl(fd, BLKGETSIZE64, &mut size as *mut u64) };
    if ret == 0 { Some(size) } else { None }
}

fn is_block_device(path: &Path) -> bool {
    use nix::sys::stat::SFlag;
    nix::sys::stat::stat(path)
        .map(|s| SFlag::from_bits_truncate(s.st_mode) & SFlag::S_IFMT == SFlag::S_IFBLK)
        .unwrap_or(false)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n";

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
        assert_eq!(devices[0].used, 512_000 * 1024);
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

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn valid_line_always_produces_device(
                total_kb in 1_u64..10_000_000u64,
                used_kb in 0_u64..10_000_000u64,
                prio in -100_i16..100i16,
            ) {
                let used_kb = used_kb.min(total_kb);
                let line = format!("/dev/sda2\t\tpartition\t{total_kb}\t{used_kb}\t{prio}\n");
                let content = format!("{HEADER}{line}");
                let devices = parse_proc_swaps(&content);
                prop_assert_eq!(devices.len(), 1);
            }

            #[test]
            fn malformed_lines_never_panic(line in ".*") {
                let content = format!("{HEADER}{line}\n");
                let _ = parse_proc_swaps(&content);
            }

            #[test]
            fn parsed_bytes_are_kb_times_1024(
                total_kb in 1_u64..10_000_000u64,
                used_kb in 0_u64..10_000_000u64,
            ) {
                let used_kb = used_kb.min(total_kb);
                let line = format!("/dev/sda2\t\tpartition\t{total_kb}\t{used_kb}\t-1\n");
                let content = format!("{HEADER}{line}");
                let devices = parse_proc_swaps(&content);
                prop_assert_eq!(devices.len(), 1);
                prop_assert_eq!(devices[0].total, total_kb * 1024);
                prop_assert_eq!(devices[0].used,  used_kb  * 1024);
            }
        }
    }
}
