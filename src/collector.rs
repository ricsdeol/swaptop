use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;
use futures::future::join_all;

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
            let mut rows = self.backend.process_list()?;

            // Spawn one task per process to read /proc/{pid}/smaps in parallel.
            let handles: Vec<_> = rows
                .iter()
                .map(|p| {
                    let pid = p.pid;
                    tokio::task::spawn_blocking(move || (pid, read_smaps_swap(pid)))
                })
                .collect();

            let swap_map: HashMap<u32, u64> = join_all(handles)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

            for row in &mut rows {
                if let Some(&bytes) = swap_map.get(&row.pid) {
                    row.swap = bytes;
                }
            }
            rows
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

/// Read `/proc/{pid}/smaps` and sum all `VmSwap:` fields, returning bytes.
/// Returns 0 if the file is unreadable (process exited mid-collection).
fn read_smaps_swap(pid: u32) -> u64 {
    let content = std::fs::read_to_string(format!("/proc/{pid}/smaps"))
        .unwrap_or_default();
    content
        .lines()
        .filter_map(|l| l.strip_prefix("VmSwap:"))
        .filter_map(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .sum::<u64>()
        * 1024
}
