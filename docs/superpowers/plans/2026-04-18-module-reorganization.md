# Module Reorganization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix dependency inversion between `platform/` and `create_swap.rs`, split mixed responsibilities, and group Linux-specific code under `platform/linux/`.

**Architecture:** Move `parse_swap_header` (renamed from `detect_swap_magic`) to `platform/types.rs`. Convert `platform/linux.rs` to `platform/linux/mod.rs` directory module with `proc_reader.rs` and `create_swap.rs` as submodules. Strip `src/create_swap.rs` to state-only types. Keep `swap_discovery.rs` at `platform/` level as cross-platform.

**Tech Stack:** Rust 2021, Cargo, no new dependencies

**Spec:** `docs/superpowers/specs/2026-04-18-module-reorganization-design.md`

---

### Task 1: Add `parse_swap_header` to `platform/types.rs`

Additive step — both `detect_swap_magic` and `parse_swap_header` exist temporarily so the build stays green.

**Files:**
- Modify: `src/platform/types.rs`

- [ ] **Step 1: Add `parse_swap_header` function and tests to `platform/types.rs`**

Append to the end of `src/platform/types.rs`, before the existing `#[cfg(test)]` block:

```rust
/// Check the first 4096 bytes for swap header (`SWAPSPACE2` or `SWAP-SPACE`)
/// at offset 4086..4096. Returns the file size if the header is valid.
pub fn parse_swap_header(buf: &[u8], size_bytes: u64) -> Option<u64> {
    if buf.len() < 4096 {
        return None;
    }
    let magic = &buf[4086..4096];
    if magic == b"SWAPSPACE2" || magic == b"SWAP-SPACE" {
        Some(size_bytes)
    } else {
        None
    }
}
```

Add these tests inside the existing `mod tests` block in `src/platform/types.rs`:

```rust
    #[test]
    fn parse_swap_header_returns_size_on_swapspace2() {
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        assert_eq!(parse_swap_header(&buf, 2_147_483_648), Some(2_147_483_648));
    }

    #[test]
    fn parse_swap_header_returns_size_on_swap_space() {
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAP-SPACE");
        assert_eq!(parse_swap_header(&buf, 1024), Some(1024));
    }

    #[test]
    fn parse_swap_header_returns_none_on_unknown_bytes() {
        let buf = vec![0u8; 4096];
        assert_eq!(parse_swap_header(&buf, 4096), None);
    }

    #[test]
    fn parse_swap_header_returns_none_on_short_buffer() {
        let buf = vec![0u8; 100];
        assert_eq!(parse_swap_header(&buf, 100), None);
    }
```

- [ ] **Step 2: Verify build and tests pass**

Run: `cargo test -p swaptop -- parse_swap_header`
Expected: 4 tests PASS

Run: `cargo build`
Expected: clean build (zero errors)

- [ ] **Step 3: Commit**

```bash
git add src/platform/types.rs
git commit -m "refactor(platform): add parse_swap_header to types.rs

Additive step — the old detect_swap_magic still exists in create_swap.rs.
Callers will be switched in the next commit."
```

---

### Task 2: Switch all callers to `parse_swap_header` and remove `detect_swap_magic`

**Files:**
- Modify: `src/platform/swap_discovery.rs`
- Modify: `src/platform/linux.rs`
- Modify: `src/create_swap.rs`

- [ ] **Step 1: Update `swap_discovery.rs` import**

In `src/platform/swap_discovery.rs`, change line 4:

```rust
// Before:
use crate::create_swap::detect_swap_magic;

// After:
use super::parse_swap_header;
```

And in the `probe_swap_file` function body (line 35), change:

```rust
// Before:
    detect_swap_magic(&buf, size)?;

// After:
    parse_swap_header(&buf, size)?;
```

- [ ] **Step 2: Update `linux.rs` import**

In `src/platform/linux.rs`, change line 9:

```rust
// Before:
use crate::create_swap::detect_swap_magic;

// After:
use super::parse_swap_header;
```

And in the `probe_swap_device` function body (line 181), change:

```rust
// Before:
    detect_swap_magic(&buf, size)?;

// After:
    parse_swap_header(&buf, size)?;
```

- [ ] **Step 3: Remove `detect_swap_magic` and its tests from `src/create_swap.rs`**

In `src/create_swap.rs`:

Remove the function (lines 152-166):
```rust
/// Swap header magic lives at bytes 4086..4096 of the first page.
///
/// Returns `Some(size_bytes)` if `buf` is ≥4096 bytes AND the magic matches.
/// `size_bytes` is supplied by the caller (from `fs::metadata().len()`).
pub fn detect_swap_magic(buf: &[u8], size_bytes: u64) -> Option<u64> {
    ...
}
```

Remove these four tests from the `mod tests` block:
- `detect_swap_magic_returns_size_on_swapspace2`
- `detect_swap_magic_returns_size_on_swap_space`
- `detect_swap_magic_returns_none_on_unknown_bytes`
- `detect_swap_magic_returns_none_on_short_buffer`

- [ ] **Step 4: Verify build and all tests pass**

Run: `cargo build`
Expected: clean build

Run: `cargo test`
Expected: all tests pass (the 4 detect_swap_magic tests are gone, replaced by 4 parse_swap_header tests)

- [ ] **Step 5: Commit**

```bash
git add src/platform/swap_discovery.rs src/platform/linux.rs src/create_swap.rs
git commit -m "refactor(platform): switch callers to parse_swap_header; remove detect_swap_magic

Fixes the dependency inversion: platform/ no longer imports from create_swap."
```

---

### Task 3: Convert `platform/linux.rs` to directory module + move `proc_reader.rs`

**Files:**
- Move: `src/platform/linux.rs` → `src/platform/linux/mod.rs`
- Move: `src/platform/proc_reader.rs` → `src/platform/linux/proc_reader.rs`
- Modify: `src/platform/mod.rs`

- [ ] **Step 1: Create directory and move `linux.rs`**

```bash
mkdir -p src/platform/linux
mv src/platform/linux.rs src/platform/linux/mod.rs
```

- [ ] **Step 2: Move `proc_reader.rs` into `linux/`**

```bash
mv src/platform/proc_reader.rs src/platform/linux/proc_reader.rs
```

- [ ] **Step 3: Add `mod proc_reader;` to `platform/linux/mod.rs`**

At the top of `src/platform/linux/mod.rs`, before any `use` statements, add:

```rust
mod proc_reader;
```

Keep the existing `use super::proc_reader::ProcReader;` import — change it to:

```rust
// Before:
use super::proc_reader::ProcReader;

// After:
use proc_reader::ProcReader;
```

- [ ] **Step 4: Update `platform/mod.rs` — remove `proc_reader` module declaration**

In `src/platform/mod.rs`, remove the line:

```rust
pub mod proc_reader;
```

The `linux` module declaration stays as-is (`pub mod linux;`) — Rust resolves it to `linux/mod.rs` automatically.

- [ ] **Step 5: Verify build and all tests pass**

Run: `cargo build`
Expected: clean build

Run: `cargo test`
Expected: all tests pass (proc_reader tests still run from their new location)

- [ ] **Step 6: Commit**

```bash
git add -A src/platform/linux/ src/platform/mod.rs
git rm --cached src/platform/proc_reader.rs 2>/dev/null; true
git commit -m "refactor(platform): move linux.rs → linux/mod.rs; move proc_reader into linux/"
```

---

### Task 4: Extract I/O functions to `platform/linux/create_swap.rs`

This is the largest task. We create the new file, move all I/O functions from `src/create_swap.rs`, and update `main.rs`.

**Files:**
- Create: `src/platform/linux/create_swap.rs`
- Modify: `src/platform/linux/mod.rs`
- Modify: `src/create_swap.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/platform/linux/create_swap.rs`**

Create `src/platform/linux/create_swap.rs` with the following content — these are the I/O functions extracted from `src/create_swap.rs`:

```rust
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command as StdCommand;

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::Action;
use crate::create_swap::StepStatus;
use crate::platform::parse_swap_header;

/// Parse `/proc/mounts` content and return the filesystem type covering `target`.
///
/// Picks the longest mount-point prefix that is a parent of `target`.
pub fn detect_fs_type(mounts_content: &str, target: &std::path::Path) -> Option<String> {
    let target_str = target.to_string_lossy();
    let mut best: Option<(usize, String)> = None;
    for line in mounts_content.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_dev), Some(mount_point), Some(fs_type)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if target_str.starts_with(mount_point)
            && (target_str.len() == mount_point.len()
                || target_str.as_bytes().get(mount_point.len()) == Some(&b'/')
                || mount_point == "/")
        {
            let len = mount_point.len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, fs_type.to_string()));
            }
        }
    }
    best.map(|(_, fs)| fs)
}

/// Given the filesystem type, decide whether to use `fallocate` or `dd`.
pub fn allocator_for_fs(fs_type: &str) -> Allocator {
    match fs_type {
        "ext2" | "ext3" | "ext4" | "xfs" | "f2fs" => Allocator::Fallocate,
        _ => Allocator::Dd,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allocator {
    Fallocate,
    Dd,
}

impl Allocator {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fallocate => "fallocate",
            Self::Dd => "dd",
        }
    }
}

/// Run all create-swap steps in `spawn_blocking`. Sends `CreateSwapStepUpdate`
/// for each step transition. On `activate_only`, skips to Step 6 (swapon).
#[allow(clippy::too_many_arguments)]
pub fn run_create_swap_steps(
    path: PathBuf,
    size_bytes: u64,
    priority: i16,
    activate_after: bool,
    activate_only: bool,
    tx: UnboundedSender<Action>,
) {
    let send = |idx: usize, status: StepStatus| {
        let _ = tx.send(Action::CreateSwapStepUpdate { index: idx, status });
    };

    if activate_only {
        send(6, StepStatus::Running);
        match do_swapon(&path) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => send(6, StepStatus::Error(e)),
        }
        return;
    }

    // Step 0 — Disk space
    send(0, StepStatus::Running);
    let parent = path.parent().unwrap_or(std::path::Path::new("/"));
    match check_disk_space(parent, size_bytes) {
        Ok(()) => send(0, StepStatus::Done),
        Err(e) => {
            send(0, StepStatus::Error(e));
            return;
        }
    }

    // Step 1 — File existence / magic
    send(1, StepStatus::Running);
    match check_target_file(&path) {
        TargetFileCheck::DoesNotExist => send(1, StepStatus::Done),
        TargetFileCheck::AlreadySwap { size } => {
            send(1, StepStatus::Done);
            let _ = tx.send(Action::OpenConfirmActivateOnly {
                path: path.clone(),
                size_bytes: size,
            });
            return;
        }
        TargetFileCheck::ExistsNotSwap => {
            send(
                1,
                StepStatus::Error(
                    "file exists and is not a swap file — refusing to overwrite".to_string(),
                ),
            );
            return;
        }
        TargetFileCheck::IoError(e) => {
            send(1, StepStatus::Error(format!("cannot inspect target: {e}")));
            return;
        }
    }

    // Step 2 — Filesystem detection
    send(2, StepStatus::Running);
    let fs_type = match fs::read_to_string("/proc/mounts") {
        Ok(content) => detect_fs_type(&content, &path).unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unknown".to_string(),
    };
    let allocator = allocator_for_fs(&fs_type);
    send(2, StepStatus::Done);

    // Step 3 — Allocate
    send(3, StepStatus::Running);
    let alloc_result = match allocator {
        Allocator::Fallocate => run_cmd(
            StdCommand::new("fallocate")
                .arg("-l")
                .arg(size_bytes.to_string())
                .arg(&path),
        ),
        Allocator::Dd => {
            let mb = size_bytes / (1024 * 1024);
            let mb = if mb == 0 { 1 } else { mb };
            run_cmd(
                StdCommand::new("dd")
                    .arg("if=/dev/zero")
                    .arg(format!("of={}", path.display()))
                    .arg("bs=1M")
                    .arg(format!("count={mb}")),
            )
        }
    };
    match alloc_result {
        Ok(()) => send(3, StepStatus::Done),
        Err(e) => {
            send(
                3,
                StepStatus::Error(format!("{} failed: {e}", allocator.label())),
            );
            return;
        }
    }

    // Step 4 — chmod 600
    send(4, StepStatus::Running);
    match fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
        Ok(()) => send(4, StepStatus::Done),
        Err(e) => {
            send(4, StepStatus::Error(format!("chmod failed: {e}")));
            return;
        }
    }

    // Step 5 — mkswap
    send(5, StepStatus::Running);
    match run_cmd(StdCommand::new("mkswap").arg(&path)) {
        Ok(()) => send(5, StepStatus::Done),
        Err(e) => {
            send(5, StepStatus::Error(format!("mkswap failed: {e}")));
            return;
        }
    }

    // Step 6 — swapon (skipped if activate_after is false)
    if activate_after {
        send(6, StepStatus::Running);
        match do_swapon_with_priority(&path, priority) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => {
                send(6, StepStatus::Error(e));
            }
        }
    } else {
        send(6, StepStatus::Done);
    }
}

enum TargetFileCheck {
    DoesNotExist,
    AlreadySwap { size: u64 },
    ExistsNotSwap,
    IoError(String),
}

fn check_target_file(path: &std::path::Path) -> TargetFileCheck {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return TargetFileCheck::DoesNotExist,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    if !meta.is_file() {
        return TargetFileCheck::ExistsNotSwap;
    }
    let size = meta.len();
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    let mut buf = vec![0u8; 4096];
    match f.read_exact(&mut buf) {
        Ok(()) => {
            if let Some(size) = parse_swap_header(&buf, size) {
                TargetFileCheck::AlreadySwap { size }
            } else {
                TargetFileCheck::ExistsNotSwap
            }
        }
        Err(_) => TargetFileCheck::ExistsNotSwap,
    }
}

fn check_disk_space(parent: &std::path::Path, needed: u64) -> Result<(), String> {
    let stat = nix::sys::statvfs::statvfs(parent).map_err(|e| e.to_string())?;
    let available = stat.blocks_available() as u64 * stat.fragment_size() as u64;
    let required = needed + needed / 10;
    if available >= required {
        Ok(())
    } else {
        Err(format!(
            "not enough space: need {} (incl. 10% margin), have {}",
            human_bytes::human_bytes(required as f64),
            human_bytes::human_bytes(available as f64),
        ))
    }
}

fn run_cmd(cmd: &mut StdCommand) -> Result<(), String> {
    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

fn do_swapon(path: &std::path::Path) -> Result<(), String> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: `c` is a valid NUL-terminated C string pointing to a valid path.
    let ret = unsafe { nix::libc::swapon(c.as_ptr(), 0) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "swapon failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

fn do_swapon_with_priority(path: &std::path::Path, priority: i16) -> Result<(), String> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let flags: i32 = if priority < 0 {
        0
    } else {
        0x8000 | (priority as i32 & 0x7fff)
    };
    // SAFETY: `c` is a valid NUL-terminated C string pointing to a valid path.
    let ret = unsafe { nix::libc::swapon(c.as_ptr(), flags) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!(
            "swapon failed: {}",
            std::io::Error::last_os_error()
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_fs_type_matches_root_mount() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
proc /proc proc rw 0 0
tmpfs /tmp tmpfs rw 0 0
";
        let fs = detect_fs_type(mounts, std::path::Path::new("/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn detect_fs_type_prefers_longest_match() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
tmpfs /home/user/ramdisk tmpfs rw 0 0
";
        let fs = detect_fs_type(mounts, std::path::Path::new("/home/user/ramdisk/swapfile"));
        assert_eq!(fs.as_deref(), Some("tmpfs"));
    }

    #[test]
    fn detect_fs_type_ignores_unrelated_mount_point() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n";
        let fs = detect_fs_type(mounts, std::path::Path::new("/var/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn allocator_for_fs_picks_fallocate_on_ext4() {
        assert_eq!(allocator_for_fs("ext4"), Allocator::Fallocate);
        assert_eq!(allocator_for_fs("xfs"), Allocator::Fallocate);
    }

    #[test]
    fn allocator_for_fs_falls_back_to_dd_on_tmpfs_or_unknown() {
        assert_eq!(allocator_for_fs("tmpfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("ramfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("whatever"), Allocator::Dd);
    }

    #[test]
    fn allocator_for_fs_uses_dd_on_btrfs() {
        assert_eq!(allocator_for_fs("btrfs"), Allocator::Dd);
    }
}
```

- [ ] **Step 2: Add module declaration in `platform/linux/mod.rs`**

In `src/platform/linux/mod.rs`, add after the `mod proc_reader;` line:

```rust
pub(crate) mod create_swap;
```

- [ ] **Step 3: Strip I/O code from `src/create_swap.rs`**

Remove the following sections from `src/create_swap.rs`:

1. Remove the `use` statements for I/O (lines 224-228 area):
```rust
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Command as StdCommand;

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::Action;
```

2. Remove the entire `// ── Pure helper functions ──` section (detect_fs_type, allocator_for_fs, Allocator enum+impl) — lines ~150-220.

3. Remove the entire `// ── Background step runner ──` section (run_create_swap_steps and all private helpers) — lines ~222-463.

4. Remove these tests from the `mod tests` block:
   - `detect_fs_type_matches_root_mount`
   - `detect_fs_type_prefers_longest_match`
   - `detect_fs_type_ignores_unrelated_mount_point`
   - `allocator_for_fs_picks_fallocate_on_ext4`
   - `allocator_for_fs_falls_back_to_dd_on_tmpfs_or_unknown`
   - `allocator_for_fs_uses_dd_on_btrfs`

5. Update the module doc comment at the top (lines 1-5) to:
```rust
//! Create-swap wizard state types.
//!
//! All types for the create-swap modal live here. The background step runner
//! that performs the actual file operations lives in `platform::linux::create_swap`.
```

After stripping, `src/create_swap.rs` should contain only:
- Module doc (updated)
- `use std::path::PathBuf;`
- `CreateSwapMode` enum
- `CreateSwapField` enum + impl (next/prev)
- `SizeUnit` enum + impl (label/multiplier/toggled)
- `CreateSwapStep` struct + impl (pending)
- `StepStatus` enum
- `CreateSwapModal` struct + impl Default
- Tests for all the above (8 tests)

- [ ] **Step 4: Update `src/main.rs` import**

In `src/main.rs`, change line 25:

```rust
// Before:
use create_swap::run_create_swap_steps;

// After:
use platform::linux::create_swap::run_create_swap_steps;
```

- [ ] **Step 5: Verify build and all tests pass**

Run: `cargo build`
Expected: clean build

Run: `cargo test`
Expected: all tests pass

Run: `cargo clippy -- -D warnings`
Expected: no warnings

- [ ] **Step 6: Commit**

```bash
git add -A src/platform/linux/create_swap.rs src/create_swap.rs src/main.rs src/platform/linux/mod.rs
git commit -m "refactor(platform): extract I/O functions to platform/linux/create_swap.rs

Moves run_create_swap_steps, detect_fs_type, allocator_for_fs, Allocator,
and all private helpers (do_swapon, check_disk_space, etc.) to
platform/linux/create_swap.rs. src/create_swap.rs now contains only
wizard state types."
```

---

### Task 5: Final verification and formatting

**Files:** None (verification only)

- [ ] **Step 1: Run full build**

```bash
cargo build
```

Expected: clean build, zero warnings.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: no warnings.

- [ ] **Step 3: Run formatter check**

```bash
cargo fmt --check
```

Expected: no formatting issues. If any, run `cargo fmt` and commit.

- [ ] **Step 4: Run all tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Verify final file structure**

```bash
find src/platform -type f | sort
```

Expected output:
```
src/platform/bsd.rs
src/platform/factory.rs
src/platform/linux/create_swap.rs
src/platform/linux/mod.rs
src/platform/linux/proc_reader.rs
src/platform/macos.rs
src/platform/mod.rs
src/platform/swap_discovery.rs
src/platform/types.rs
src/platform/windows.rs
```

- [ ] **Step 6: Verify dependency direction — no upward imports**

```bash
grep -rn 'use crate::create_swap' src/platform/
```

Expected: **one result** — `platform/linux/create_swap.rs` importing `crate::create_swap::StepStatus` (intentional narrow coupling documented in the spec). Verify it's only `StepStatus`.

```bash
grep -n 'crate::create_swap' src/platform/linux/create_swap.rs
```

Expected: exactly one line: `use crate::create_swap::StepStatus;`

- [ ] **Step 7: Commit formatting fixes if any**

If `cargo fmt` made changes:
```bash
git add -A
git commit -m "style: apply cargo fmt after module reorganization"
```
