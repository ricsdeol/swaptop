# Processes Screen Performance Fix — Design Spec

**Date:** 2026-04-11
**Branch:** feature/processes-screen
**Scope:** Replace smaps-based per-process swap collection with lightweight `/proc` reader. No UI or state changes.

---

## Problem

Every 1-second tick with the Processes tab active, the collector:

1. Calls `sysinfo::System::refresh_processes(All, true)` — re-reads multiple `/proc/{pid}/*` files for every process, including fields we don't need
2. Spawns **one `tokio::task::spawn_blocking` per process** to read `/proc/{pid}/smaps`
3. `join_all` waits for all of them

On a system with 300 processes this means 300 blocking threads/sec, each parsing multi-KB smaps files. This saturates CPU.

---

## Solution Summary

- Replace sysinfo-based process listing + per-process smaps reads with a single sequential `/proc` reader
- New `ProcReader` module reads two small files per process: `/proc/{pid}/status` (~1KB) + `/proc/{pid}/stat` (~300B)
- One `spawn_blocking` for the entire collection, not one per process
- CPU% computed via tick-over-tick delta stored in `ProcReader`
- Expected CPU reduction: **90%+** for the process collection path

---

## 1. New Module: `src/platform/proc_reader.rs`

### Struct

```rust
pub struct ProcReader {
    prev_ticks:    HashMap<u32, u64>,       // PID -> previous (utime + stime)
    prev_time:     Instant,                 // wall-clock of last collection
    uid_cache:     HashMap<u32, String>,    // UID -> username, populated lazily
    clock_ticks:   u64,                     // sysconf(_SC_CLK_TCK)
}
```

### Public API

```rust
impl ProcReader {
    pub fn new() -> Self;
    pub fn collect(&mut self) -> Vec<ProcessRow>;
}
```

### `collect()` algorithm

1. `std::fs::read_dir("/proc")` — filter entries where name parses as `u32` (PID directories)
2. For each PID, read:
   - `/proc/{pid}/status` -> `Name:`, `Uid:` (first field, real UID), `VmRSS:`, `VmSize:`, `VmSwap:` (all in kB, multiply by 1024)
   - `/proc/{pid}/stat` -> fields 14+15 after the last `)` (`utime` + `stime`)
3. Filter kernel threads: `name.starts_with('[') && name.ends_with(']')` — skip
4. Compute CPU%:
   ```
   delta_ticks = (utime + stime) - prev_ticks[pid]
   delta_secs  = now.duration_since(prev_time).as_secs_f64()
   cpu_pct     = (delta_ticks as f64 / clock_ticks as f64 / delta_secs) * 100.0
   ```
   First-tick processes get `cpu_pct: 0.0`.
5. Resolve UID -> username via `uid_cache`. On cache miss, call `nix::unistd::User::from_uid()` and cache the result.
6. Update `prev_ticks` (replace entire map with current tick's values) and `prev_time`.
7. Return `Vec<ProcessRow>`.

Processes that vanish mid-read (ENOENT, permission denied) are silently skipped.

### `/proc/{pid}/status` fields

```
Name:   firefox
Uid:    1000    1000    1000    1000
VmSize:   3145728 kB
VmRSS:     524288 kB
VmSwap:    131072 kB
```

Kernel threads have no `VmRSS`/`VmSize`/`VmSwap` lines — they are filtered out by the `is_kernel_thread` name check.

### `/proc/{pid}/stat` parsing

Single line, space-separated. The `comm` field (field 2) is wrapped in `()` and can contain spaces (e.g. `(Web Content)`). Parse by finding the **last** `)` in the line, then splitting fields after it. `utime` is field 14, `stime` is field 15 (1-indexed from start of line; 1st and 2nd fields after `)` skipping the state field).

### Clock ticks

Use `nix::unistd::sysconf(SysconfVar::CLK_TCK)` at `ProcReader::new()` time. Cached for the lifetime of the struct.

### UID cache

On first encounter of a UID, resolve via `nix::unistd::User::from_uid()` and cache. The cache is never invalidated — user creation mid-session is not a concern for a monitoring tool.

---

## 2. Changes to `LinuxBackend`

`LinuxBackend` gains a `ProcReader` field:

```rust
pub struct LinuxBackend {
    sys:         System,
    users:       Users,
    proc_reader: ProcReader,
}
```

`process_list()` becomes:

```rust
fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
    Ok(self.proc_reader.collect())
}
```

No other `LinuxBackend` methods change. `system_ram()`, `system_swap()`, `swap_devices()` remain sysinfo-based.

---

## 3. Changes to `Collector`

The `collect()` method simplifies to:

```rust
pub async fn collect(&mut self) -> Result<MemSnapshot> {
    let ram     = self.backend.system_ram()?;
    let swap    = self.backend.system_swap()?;
    let devices = self.backend.swap_devices()?;

    let processes = if self.processes_active.load(Ordering::Relaxed) {
        self.backend.process_list()?
    } else {
        vec![]
    };

    Ok(MemSnapshot { timestamp: Instant::now(), ram, swap, devices, processes })
}
```

**Deleted:**
- `read_smaps_swap()` function
- Per-process `tokio::task::spawn_blocking` spawning
- `futures::future::join_all` import and `HashMap` import

The `futures` crate stays in `Cargo.toml` — `main.rs` still uses `futures::{FutureExt, StreamExt}`.

---

## 4. Files Changed

| File | Action | Change |
|---|---|---|
| `src/platform/proc_reader.rs` | Create | `ProcReader` struct, `/proc` parsing, CPU delta, UID cache |
| `src/platform/mod.rs` | Modify | Add `pub mod proc_reader;` |
| `src/platform/linux.rs` | Modify | Add `ProcReader` field, delegate `process_list()` |
| `src/collector.rs` | Modify | Remove smaps/spawn_blocking machinery, simplify to direct `process_list()` call |

**No changes to:** `app.rs`, `ui/processes.rs`, `main.rs`, `actions.rs`, `types.rs`, other backends.

---

## 5. Testing

Unit tests in `proc_reader.rs` for pure parsing functions:

- `parse_status()` — given `/proc/{pid}/status` content string, extract Name/Uid/VmRSS/VmSize/VmSwap
- `parse_stat_cpu_ticks()` — given `/proc/{pid}/stat` content string, extract utime + stime
  - Edge case: `comm` field containing spaces, e.g. `(Web Content)`
- `is_kernel_thread()` — already tested in `linux.rs`, reuse via `pub(crate)` or move to shared location

All parsing tests are pure string-in, struct-out — no filesystem access needed.

---

## 6. Performance Comparison

| Metric | Before | After |
|---|---|---|
| Blocking tasks per tick | ~300 (one per process) | 1 (entire collection) |
| Files read per process | 1 large (`smaps`, multi-KB) | 2 small (`status` ~1KB + `stat` ~300B) |
| sysinfo refresh | `refresh_processes(All, true)` | None for processes |
| Estimated tick duration | 50-200ms+ | 5-15ms |
| Thread pool pressure | High (300 spawn_blocking/sec) | Minimal (1 spawn_blocking/sec or none) |
