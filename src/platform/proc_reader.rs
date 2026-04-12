use std::collections::HashMap;
use std::time::Instant;

use crate::platform::ProcessRow;

/// Fields extracted from `/proc/{pid}/status`.
#[derive(Debug, PartialEq)]
#[allow(dead_code)]
struct StatusInfo {
    name: String,
    uid:  u32,
    rss:  u64,
    vms:  u64,
    swap: u64,
}

#[allow(dead_code)]
pub struct ProcReader {
    prev_ticks:  HashMap<u32, u64>,
    prev_time:   Instant,
    uid_cache:   HashMap<u32, String>,
    clock_ticks: f64,
}

#[allow(dead_code)]
impl ProcReader {
    pub fn new() -> Self {
        let clock_ticks = nix::unistd::sysconf(nix::unistd::SysconfVar::CLK_TCK)
            .ok()
            .flatten()
            .unwrap_or(100) as f64;
        Self {
            prev_ticks:  HashMap::new(),
            prev_time:   Instant::now(),
            uid_cache:   HashMap::new(),
            clock_ticks,
        }
    }

    pub fn collect(&mut self) -> Vec<ProcessRow> {
        let now = Instant::now();
        let delta_secs = now.duration_since(self.prev_time).as_secs_f64();

        let mut rows = Vec::new();
        let mut new_ticks: HashMap<u32, u64> = HashMap::new();

        let entries = match std::fs::read_dir("/proc") {
            Ok(e) => e,
            Err(_) => return rows,
        };

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();
            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let status_path = format!("/proc/{pid}/status");
            let status_content = match std::fs::read_to_string(&status_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let info = match parse_status(&status_content) {
                Some(i) => i,
                None => continue,
            };

            if is_kernel_thread(&info.name) {
                continue;
            }

            let stat_path = format!("/proc/{pid}/stat");
            let stat_content = match std::fs::read_to_string(&stat_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let ticks = parse_stat_cpu_ticks(&stat_content).unwrap_or(0);
            new_ticks.insert(pid, ticks);

            let cpu_pct = if delta_secs > 0.0 {
                if let Some(&prev) = self.prev_ticks.get(&pid) {
                    let delta_ticks = ticks.saturating_sub(prev) as f64;
                    (delta_ticks / self.clock_ticks / delta_secs * 100.0) as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };

            let user = self.resolve_user(info.uid);

            rows.push(ProcessRow {
                pid,
                name: info.name,
                user,
                rss: info.rss,
                vms: info.vms,
                swap: info.swap,
                cpu_pct,
            });
        }

        self.prev_ticks = new_ticks;
        self.prev_time = now;

        rows
    }

    fn resolve_user(&mut self, uid: u32) -> String {
        if let Some(name) = self.uid_cache.get(&uid) {
            return name.clone();
        }
        let name = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(uid))
            .ok()
            .flatten()
            .map(|u| u.name)
            .unwrap_or_default();
        self.uid_cache.insert(uid, name.clone());
        name
    }
}

impl Default for ProcReader {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
fn is_kernel_thread(name: &str) -> bool {
    name.starts_with('[') && name.ends_with(']')
}

#[allow(dead_code)]
fn parse_status(content: &str) -> Option<StatusInfo> {
    let mut name: Option<String> = None;
    let mut uid: Option<u32> = None;
    let mut rss: u64 = 0;
    let mut vms: u64 = 0;
    let mut swap: u64 = 0;

    for line in content.lines() {
        if let Some(v) = line.strip_prefix("Name:\t") {
            name = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("Uid:\t") {
            uid = v.split_whitespace().next()?.parse().ok();
        } else if let Some(v) = line.strip_prefix("VmRSS:") {
            rss = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("VmSize:") {
            vms = parse_kb_value(v);
        } else if let Some(v) = line.strip_prefix("VmSwap:") {
            swap = parse_kb_value(v);
        }
    }

    Some(StatusInfo {
        name: name?,
        uid: uid?,
        rss,
        vms,
        swap,
    })
}

#[allow(dead_code)]
fn parse_kb_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

#[allow(dead_code)]
fn parse_stat_cpu_ticks(content: &str) -> Option<u64> {
    let after_comm = content.rfind(')')? + 1;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_status tests ────────────────────────────────────────────────────

    #[test]
    fn parse_status_extracts_all_fields() {
        let content = "\
Name:\tfirefox
Umask:\t0022
State:\tS (sleeping)
Tgid:\t1234
Pid:\t1234
Uid:\t1000\t1000\t1000\t1000
Gid:\t1000\t1000\t1000\t1000
VmPeak:\t 3145728 kB
VmSize:\t 2097152 kB
VmRSS:\t  524288 kB
VmSwap:\t  131072 kB
Threads:\t4
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.name, "firefox");
        assert_eq!(info.uid, 1000);
        assert_eq!(info.rss, 524288 * 1024);
        assert_eq!(info.vms, 2097152 * 1024);
        assert_eq!(info.swap, 131072 * 1024);
    }

    #[test]
    fn parse_status_returns_none_when_name_missing() {
        let content = "\
Uid:\t1000\t1000\t1000\t1000
VmSize:\t 2097152 kB
VmRSS:\t  524288 kB
VmSwap:\t  131072 kB
";
        assert!(parse_status(content).is_none());
    }

    #[test]
    fn parse_status_handles_kernel_thread_without_vm_fields() {
        let content = "\
Name:\t[kworker/0:0]
State:\tI (idle)
Pid:\t42
Uid:\t0\t0\t0\t0
Gid:\t0\t0\t0\t0
Threads:\t1
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.name, "[kworker/0:0]");
        assert_eq!(info.uid, 0);
        assert_eq!(info.rss, 0);
        assert_eq!(info.vms, 0);
        assert_eq!(info.swap, 0);
    }

    #[test]
    fn parse_status_handles_zero_swap() {
        let content = "\
Name:\tbash
Uid:\t1000\t1000\t1000\t1000
VmSize:\t 102400 kB
VmRSS:\t   51200 kB
VmSwap:\t       0 kB
";
        let info = parse_status(content).unwrap();
        assert_eq!(info.swap, 0);
    }

    // ── parse_stat_cpu_ticks tests ────────────────────────────────────────────

    #[test]
    fn parse_stat_extracts_utime_plus_stime() {
        let content = "1234 (firefox) S 1000 1234 1234 0 -1 4194304 \
                       1000 0 100 0 54321 12345 0 0 20 0 4 0 1000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 54321 + 12345);
    }

    #[test]
    fn parse_stat_handles_comm_with_spaces() {
        let content = "5678 (Web Content) S 1000 5678 5678 0 -1 4194304 \
                       500 0 50 0 11111 22222 0 0 20 0 1 0 2000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 11111 + 22222);
    }

    #[test]
    fn parse_stat_handles_comm_with_parentheses() {
        let content = "9999 (my (app) name) S 1000 9999 9999 0 -1 4194304 \
                       200 0 10 0 100 200 0 0 20 0 1 0 3000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
        let ticks = parse_stat_cpu_ticks(content).unwrap();
        assert_eq!(ticks, 100 + 200);
    }

    #[test]
    fn parse_stat_returns_none_for_garbage() {
        assert!(parse_stat_cpu_ticks("not a stat line").is_none());
    }
}
