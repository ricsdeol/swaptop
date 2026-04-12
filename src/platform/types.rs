use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SwapInfo {
    pub total: u64,   // bytes
    pub used: u64,    // bytes
    pub percent: f32, // 0.0–100.0
}

impl SwapInfo {
    /// Canonical constructor — derives `percent` from `total` and `used`.
    pub fn new(total: u64, used: u64) -> Self {
        let percent = if total > 0 {
            used as f32 / total as f32 * 100.0
        } else {
            0.0
        };
        Self {
            total,
            used,
            percent,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SwapDevice {
    pub path: PathBuf,
    pub total: u64,
    pub used: u64,
    pub priority: i16,
    pub kind: SwapKind,
    pub active: bool,
}

#[derive(Debug, Clone)]
pub enum SwapKind {
    Partition,
    File,
    Zram,
}

impl std::fmt::Display for SwapKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapKind::Partition => write!(f, "Partition"),
            SwapKind::File => write!(f, "File"),
            SwapKind::Zram => write!(f, "Zram"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub user: String,
    pub rss: u64,
    pub swap: u64,
    pub cpu_pct: f32,
}

#[derive(Debug, Clone)]
pub struct Capabilities {
    pub can_swap_on: bool,
    pub has_per_process: bool,
}

/// Full snapshot collected every tick.
#[derive(Debug, Clone)]
pub struct MemSnapshot {
    pub timestamp: Instant,
    pub ram: SwapInfo,
    pub swap: SwapInfo,
    pub devices: Vec<SwapDevice>,
    pub processes: Vec<ProcessRow>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_info_percent_is_zero_when_total_is_zero() {
        let info = SwapInfo::new(0, 0);
        assert_eq!(info.percent, 0.0);
    }

    #[test]
    fn swap_info_percent_at_fifty_percent() {
        let info = SwapInfo::new(2_000_000, 1_000_000);
        assert!((info.percent - 50.0).abs() < 0.01, "got {}", info.percent);
    }

    #[test]
    fn swap_info_handles_used_exceeding_total() {
        // When used exceeds total, percent can exceed 100
        let info = SwapInfo::new(100, 200);
        assert_eq!(info.percent, 200.0);
    }

    #[test]
    fn swap_info_full_usage_gives_hundred_percent() {
        let info = SwapInfo::new(4 * 1024 * 1024 * 1024, 4 * 1024 * 1024 * 1024);
        assert!((info.percent - 100.0).abs() < 0.01, "got {}", info.percent);
    }
}
