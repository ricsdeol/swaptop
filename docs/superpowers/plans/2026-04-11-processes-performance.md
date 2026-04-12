# Processes Performance Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the expensive smaps-based per-process swap collection with a lightweight direct `/proc` reader, reducing CPU usage by 90%+.

**Architecture:** New `ProcReader` struct in `src/platform/proc_reader.rs` reads `/proc/{pid}/status` + `/proc/{pid}/stat` sequentially in a single pass. `LinuxBackend` delegates `process_list()` to `ProcReader`. Collector drops all per-process `spawn_blocking` + `join_all` machinery.

**Tech Stack:** Rust, nix (sysconf, User), std::fs for `/proc` reads

**Worktree:** `/home/ricsdeol/projects/swaptop/.worktrees/processes-screen`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/platform/proc_reader.rs` | Create | `ProcReader` struct, `parse_status()`, `parse_stat_cpu_ticks()`, `collect()`, UID cache, CPU delta |
| `src/platform/mod.rs` | Modify | Add `pub mod proc_reader;` |
| `src/platform/linux.rs` | Modify | Add `ProcReader` field to `LinuxBackend`, delegate `process_list()`, remove sysinfo process code |
| `src/collector.rs` | Modify | Remove `read_smaps_swap()`, remove per-process spawning, simplify `collect()` |

---

## Task 1: `/proc/{pid}/status` parser with tests

**Files:**
- Create: `src/platform/proc_reader.rs`
- Modify: `src/platform/mod.rs`

- [ ] **Step 1: Write the failing tests for `parse_status`**

Create `src/platform/proc_reader.rs` with only the test module and a `StatusInfo` struct:

```rust
use std::collections::HashMap;
use std::time::Instant;

use crate::platform::ProcessRow;

/// Fields extracted from `/proc/{pid}/status`.
#[derive(Debug, PartialEq)]
struct StatusInfo {
    name: String,
    uid:  u32,
    rss:  u64,  // bytes
    vms:  u64,  // bytes
    swap: u64,  // bytes
}

/// Parse `/proc/{pid}/status` content into a `StatusInfo`.
/// Returns `None` if required fields are missing.
fn parse_status(content: &str) -> Option<StatusInfo> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
```

Add the module declaration to `src/platform/mod.rs`. Insert after line 4 (`pub mod linux;`):

```rust
pub mod proc_reader;
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /home/ricsdeol/projects/swaptop/.worktrees/processes-screen
cargo test platform::proc_reader::tests 2>&1 | tail -10
```

Expected: `not yet implemented` panic from `todo!()`

- [ ] **Step 3: Implement `parse_status`**

Replace the `todo!()` body of `parse_status` in `src/platform/proc_reader.rs`:

```rust
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

/// Parse a value like `"  524288 kB"` into bytes.
fn parse_kb_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test platform::proc_reader::tests 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 5: Commit**

```bash
git add src/platform/proc_reader.rs src/platform/mod.rs
git commit -m "feat(proc_reader): add parse_status with tests"
```

---

## Task 2: `/proc/{pid}/stat` CPU ticks parser with tests

**Files:**
- Modify: `src/platform/proc_reader.rs`

- [ ] **Step 1: Write the failing tests for `parse_stat_cpu_ticks`**

Add above the `#[cfg(test)]` block in `src/platform/proc_reader.rs`:

```rust
/// Parse `/proc/{pid}/stat` and return `utime + stime` (total CPU ticks).
/// Returns `None` if the format is unexpected.
fn parse_stat_cpu_ticks(content: &str) -> Option<u64> {
    todo!()
}
```

Add these tests inside the existing `mod tests` block:

```rust
    #[test]
    fn parse_stat_extracts_utime_plus_stime() {
        // Fields: pid (comm) state ppid pgrp session tty_nr tpgid flags
        //         minflt cminflt majflt cmajflt utime stime ...
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test platform::proc_reader::tests::parse_stat 2>&1 | tail -10
```

Expected: `not yet implemented` panic from `todo!()`

- [ ] **Step 3: Implement `parse_stat_cpu_ticks`**

Replace the `todo!()` body:

```rust
fn parse_stat_cpu_ticks(content: &str) -> Option<u64> {
    // Find the last ')' to skip the comm field (which can contain spaces and parens).
    let after_comm = content.rfind(')')? + 1;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    // After ')': state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5) flags(6)
    //            minflt(7) cminflt(8) majflt(9) cmajflt(10) utime(11) stime(12)
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test platform::proc_reader::tests::parse_stat 2>&1 | tail -10
```

Expected: `test result: ok. 4 passed`

- [ ] **Step 5: Commit**

```bash
git add src/platform/proc_reader.rs
git commit -m "feat(proc_reader): add parse_stat_cpu_ticks with tests"
```

---

## Task 3: `ProcReader` struct and `collect()` method

**Files:**
- Modify: `src/platform/proc_reader.rs`

- [ ] **Step 1: Add the `ProcReader` struct and `collect()` implementation**

Replace the top of `src/platform/proc_reader.rs` (everything before `#[cfg(test)]`) with the full module:

```rust
use std::collections::HashMap;
use std::time::Instant;

use crate::platform::ProcessRow;

/// Fields extracted from `/proc/{pid}/status`.
#[derive(Debug, PartialEq)]
struct StatusInfo {
    name: String,
    uid:  u32,
    rss:  u64,
    vms:  u64,
    swap: u64,
}

pub struct ProcReader {
    prev_ticks:  HashMap<u32, u64>,
    prev_time:   Instant,
    uid_cache:   HashMap<u32, String>,
    clock_ticks: f64,
}

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
        let mut new_ticks = HashMap::new();

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

fn is_kernel_thread(name: &str) -> bool {
    name.starts_with('[') && name.ends_with(']')
}

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

fn parse_kb_value(s: &str) -> u64 {
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        * 1024
}

fn parse_stat_cpu_ticks(content: &str) -> Option<u64> {
    let after_comm = content.rfind(')')? + 1;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}
```

- [ ] **Step 2: Run all tests to verify nothing broke**

```bash
cargo test platform::proc_reader::tests 2>&1 | tail -10
```

Expected: `test result: ok. 8 passed`

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: no warnings

- [ ] **Step 4: Commit**

```bash
git add src/platform/proc_reader.rs
git commit -m "feat(proc_reader): add ProcReader struct with collect() method"
```

---

## Task 4: Wire `ProcReader` into `LinuxBackend`

**Files:**
- Modify: `src/platform/linux.rs`

- [ ] **Step 1: Add `ProcReader` field and update constructor**

In `src/platform/linux.rs`, replace the import line and struct definition (lines 1-19):

```rust
use std::path::Path;

use color_eyre::Result;
use sysinfo::{System, Users};

use super::proc_reader::ProcReader;
use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};

pub struct LinuxBackend {
    sys:         System,
    users:       Users,
    proc_reader: ProcReader,
}

impl LinuxBackend {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let users = Users::new_with_refreshed_list();
        Self { sys, users, proc_reader: ProcReader::new() }
    }
}
```

- [ ] **Step 2: Replace `process_list()` body**

Replace the current `process_list()` method (lines 38-63 of `linux.rs`) with:

```rust
    fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
        Ok(self.proc_reader.collect())
    }
```

- [ ] **Step 3: Remove the now-unused `PathBuf` import**

The old import line was `use std::path::{Path, PathBuf};`. Since `PathBuf` was used for swap device parsing which still uses it — check:

```bash
cd /home/ricsdeol/projects/swaptop/.worktrees/processes-screen
grep -n 'PathBuf' src/platform/linux.rs
```

If `PathBuf` is still used in `parse_swap_line`, keep it. Otherwise remove it from the import.

- [ ] **Step 4: Run all tests**

```bash
cargo test 2>&1 | tail -15
```

Expected: all tests pass (existing `linux::tests` + new `proc_reader::tests`)

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: no warnings. May warn about unused `sysinfo::ProcessesToUpdate` import — remove if so.

- [ ] **Step 6: Commit**

```bash
git add src/platform/linux.rs
git commit -m "refactor(linux): delegate process_list() to ProcReader"
```

---

## Task 5: Simplify `Collector` — remove smaps machinery

**Files:**
- Modify: `src/collector.rs`

- [ ] **Step 1: Replace the entire `src/collector.rs` contents**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;

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
            self.backend.process_list()?
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
```

This removes:
- `read_smaps_swap()` function
- `futures::future::join_all` import
- `std::collections::HashMap` import
- All per-process `tokio::task::spawn_blocking` logic

- [ ] **Step 2: Run all tests**

```bash
cargo test 2>&1 | tail -15
```

Expected: all tests pass

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: no warnings

- [ ] **Step 4: Build release to confirm no dead code warnings**

```bash
cargo build 2>&1 | tail -10
```

Expected: clean build, no warnings

- [ ] **Step 5: Commit**

```bash
git add src/collector.rs
git commit -m "refactor(collector): remove smaps/spawn_blocking, use direct process_list()"
```

---

## Task 6: Clean up `linux.rs` — remove dead `is_kernel_thread` if duplicated

**Files:**
- Modify: `src/platform/linux.rs`

- [ ] **Step 1: Check if `is_kernel_thread` in `linux.rs` is still referenced**

```bash
cd /home/ricsdeol/projects/swaptop/.worktrees/processes-screen
grep -rn 'is_kernel_thread' src/
```

If `is_kernel_thread` in `linux.rs` (line 113) is no longer called from `process_list()` (which now delegates to `ProcReader`), and `proc_reader.rs` has its own copy, then the one in `linux.rs` is dead code.

- [ ] **Step 2: Remove `is_kernel_thread` from `linux.rs` if dead**

Delete the function at line 113-115 of `linux.rs`:

```rust
pub(crate) fn is_kernel_thread(name: &str) -> bool {
    name.starts_with('[') && name.ends_with(']')
}
```

- [ ] **Step 3: Update tests in `linux.rs`**

The `kernel_thread_filter_matches_bracketed_names` and `kernel_thread_filter_rejects_regular_processes` tests (lines 226-238) reference `is_kernel_thread`. Two options:

If the function was removed from `linux.rs`, delete these tests — equivalent tests exist in `proc_reader.rs`. Remove lines 226-238 from the `tests` module.

- [ ] **Step 4: Run all tests and clippy**

```bash
cargo test 2>&1 | tail -15 && cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: all pass, no warnings

- [ ] **Step 5: Commit**

```bash
git add src/platform/linux.rs
git commit -m "chore(linux): remove duplicated is_kernel_thread (now in proc_reader)"
```

---

## Task 7: Add `is_kernel_thread` tests to `proc_reader.rs`

**Files:**
- Modify: `src/platform/proc_reader.rs`

- [ ] **Step 1: Add kernel thread filter tests**

Add these tests inside the existing `mod tests` block in `src/platform/proc_reader.rs`:

```rust
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
```

- [ ] **Step 2: Run tests**

```bash
cargo test platform::proc_reader::tests::kernel_thread 2>&1 | tail -10
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 3: Commit**

```bash
git add src/platform/proc_reader.rs
git commit -m "test(proc_reader): add is_kernel_thread tests"
```

---

## Task 8: Final verification

**Files:** None (read-only verification)

- [ ] **Step 1: Run full test suite**

```bash
cd /home/ricsdeol/projects/swaptop/.worktrees/processes-screen
cargo test 2>&1 | tail -20
```

Expected: all tests pass

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: no warnings

- [ ] **Step 3: Clean build**

```bash
cargo build 2>&1 | tail -10
```

Expected: clean, no warnings

- [ ] **Step 4: Verify no leftover smaps references**

```bash
grep -rn 'smaps\|read_smaps\|join_all' src/
```

Expected: no matches
