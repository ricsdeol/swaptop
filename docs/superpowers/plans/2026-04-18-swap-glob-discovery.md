# Swap File Glob Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace static exact-path swap file discovery with pattern-based directory scanning in a shared module reusable across platform backends.

**Architecture:** Create `src/platform/swap_discovery.rs` with `matches_pattern`, `probe_swap_file`, and `discover_inactive_swap_files`. Linux backend defines its own `LINUX_SCAN_DIRS` constant and calls the shared module. Tests for `probe_swap_file` and discovery logic move from `linux.rs` to `swap_discovery.rs`.

**Tech Stack:** Rust std (`fs::read_dir`, `path`), existing `detect_swap_magic` from `create_swap.rs`

---

### Task 1: Create `swap_discovery.rs` with `matches_pattern` + tests

**Files:**
- Create: `src/platform/swap_discovery.rs`
- Modify: `src/platform/mod.rs:1-12`

- [ ] **Step 1: Write the failing tests for `matches_pattern`**

Create `src/platform/swap_discovery.rs` with only the test module and a stub:

```rust
fn matches_pattern(_name: &str, _pattern: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_name() {
        assert!(matches_pattern("swapfile", "swapfile"));
    }

    #[test]
    fn prefix_wildcard_matches() {
        assert!(matches_pattern("swapfile", "swap*"));
        assert!(matches_pattern("swapfile1", "swap*"));
        assert!(matches_pattern("swap", "swap*"));
        assert!(matches_pattern("swap.img", "swap*"));
    }

    #[test]
    fn suffix_wildcard_matches() {
        assert!(matches_pattern("data.swap", "*.swap"));
        assert!(matches_pattern("big.swap", "*.swap"));
        assert!(matches_pattern("myfile.img", "*.img"));
    }

    #[test]
    fn prefix_and_suffix_wildcard_matches() {
        assert!(matches_pattern("swapfile1.bak", "swap*.bak"));
    }

    #[test]
    fn rejects_non_matching_names() {
        assert!(!matches_pattern("readme.txt", "swap*"));
        assert!(!matches_pattern("data.txt", "*.swap"));
        assert!(!matches_pattern("swapfile", "swapfile2"));
    }

    #[test]
    fn empty_name_matches_lone_star() {
        assert!(matches_pattern("", "*"));
    }

    #[test]
    fn empty_name_rejects_non_star_pattern() {
        assert!(!matches_pattern("", "swap*"));
    }

    #[test]
    fn pattern_without_star_requires_exact_match() {
        assert!(matches_pattern("swap.img", "swap.img"));
        assert!(!matches_pattern("swap.img2", "swap.img"));
    }
}
```

- [ ] **Step 2: Register the module in `mod.rs`**

In `src/platform/mod.rs`, add `pub(crate) mod swap_discovery;` after the existing module declarations. Insert it between the `pub mod linux;` line and the `#[cfg(target_os = "macos")]` block. The file should look like:

```rust
#[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
pub mod bsd;
pub mod factory;
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod proc_reader;
pub(crate) mod swap_discovery;
pub mod types;
#[cfg(target_os = "windows")]
pub mod windows;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `rtk cargo test --lib platform::swap_discovery -- --nocapture`

Expected: Multiple FAIL — the stub returns `false` so all positive assertions fail.

- [ ] **Step 4: Implement `matches_pattern`**

Replace the stub in `src/platform/swap_discovery.rs` with:

```rust
fn matches_pattern(name: &str, pattern: &str) -> bool {
    match pattern.find('*') {
        None => name == pattern,
        Some(i) => {
            let prefix = &pattern[..i];
            let suffix = &pattern[i + 1..];
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `rtk cargo test --lib platform::swap_discovery -- --nocapture`

Expected: All 8 tests PASS.

- [ ] **Step 6: Run clippy**

Run: `rtk cargo clippy -- -D warnings`

Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
rtk git add src/platform/swap_discovery.rs src/platform/mod.rs
rtk git commit -m "feat(swap-discovery): add matches_pattern with tests"
```

---

### Task 2: Move `probe_swap_file` to `swap_discovery.rs` + tests

**Files:**
- Modify: `src/platform/swap_discovery.rs`
- Modify: `src/platform/linux.rs:165-192` (remove `probe_swap_file`, `WELL_KNOWN_SWAP_PATHS`, `use crate::create_swap::detect_swap_magic`)

- [ ] **Step 1: Add `probe_swap_file` and its tests to `swap_discovery.rs`**

Add these imports at the top of `src/platform/swap_discovery.rs` (before `matches_pattern`):

```rust
use std::path::Path;

use crate::create_swap::detect_swap_magic;
use crate::platform::{SwapDevice, SwapKind};
```

Add `probe_swap_file` after `matches_pattern`:

```rust
pub(crate) fn probe_swap_file(path: &Path) -> Option<SwapDevice> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    if size < 4096 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    detect_swap_magic(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::File,
        active: false,
    })
}
```

Add these tests inside the existing `#[cfg(test)] mod tests` block, after the `matches_pattern` tests:

```rust
    #[test]
    fn probe_returns_none_for_nonexistent() {
        let result = probe_swap_file(Path::new("/tmp/nonexistent_swap_probe_test_xyz"));
        assert!(result.is_none());
    }

    #[test]
    fn probe_returns_none_for_non_swap() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_discovery_test_non_swap");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn probe_returns_device_for_swap_magic() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_discovery_test_swap_magic");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_some());
        let dev = result.unwrap();
        assert_eq!(dev.path, path);
        assert!(!dev.active);
        assert!(matches!(dev.kind, SwapKind::File));
        assert_eq!(dev.total, 4096);

        std::fs::remove_file(&path).unwrap();
    }
```

- [ ] **Step 2: Run the new tests to verify they pass**

Run: `rtk cargo test --lib platform::swap_discovery -- --nocapture`

Expected: All 11 tests PASS (8 matches_pattern + 3 probe).

- [ ] **Step 3: Remove `probe_swap_file` and related code from `linux.rs`**

In `src/platform/linux.rs`, remove these lines (165–192):

```rust
use crate::create_swap::detect_swap_magic;

const WELL_KNOWN_SWAP_PATHS: &[&str] = &["/swapfile", "/var/swapfile", "/swap", "/swap.img"];

/// Check if `path` is a regular file with swap magic header.
/// Returns `None` silently on any I/O or permission error.
fn probe_swap_file(path: &Path) -> Option<SwapDevice> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    if size < 4096 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    detect_swap_magic(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::File,
        active: false,
    })
}
```

Add this import at the top of `linux.rs` (in the existing imports area, after `use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};`):

```rust
use super::swap_discovery::probe_swap_file;
```

Note: `probe_swap_device` in `linux.rs` (line 196–214) still uses `detect_swap_magic` directly, so add this import to `linux.rs` to keep it compiling:

```rust
use crate::create_swap::detect_swap_magic;
```

Wait — `detect_swap_magic` was already imported. Since we're removing the lines that contained it, we need to keep it but only for `probe_swap_device`. The simplest approach: keep the `use crate::create_swap::detect_swap_magic;` import but move it up to the imports section at the top of the file, next to the other `use` statements.

The top of `linux.rs` should look like:

```rust
use std::path::{Path, PathBuf};

use color_eyre::Result;
use sysinfo::System;

use super::proc_reader::ProcReader;
use super::swap_discovery::probe_swap_file;
use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};
use crate::create_swap::detect_swap_magic;
```

- [ ] **Step 4: Remove the old `probe_swap_file` tests from `linux.rs`**

In `src/platform/linux.rs`, remove these test functions from the `mod tests` block (lines 306–367):

```rust
    #[test]
    fn probe_swap_file_returns_none_for_nonexistent() {
        let result = probe_swap_file(Path::new("/tmp/nonexistent_swap_probe_test_xyz"));
        assert!(result.is_none());
    }

    #[test]
    fn probe_swap_file_returns_none_for_non_swap() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_test_non_swap");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_none());

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn probe_swap_file_returns_device_for_swap_magic() {
        use std::io::Write;
        let dir = std::env::temp_dir();
        let path = dir.join("swaptop_test_swap_magic");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let result = probe_swap_file(&path);
        assert!(result.is_some());
        let dev = result.unwrap();
        assert_eq!(dev.path, path);
        assert!(!dev.active);
        assert!(matches!(dev.kind, SwapKind::File));
        assert_eq!(dev.total, 4096);

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn discover_inactive_skips_active_paths() {
        use std::collections::HashSet;

        let active: HashSet<PathBuf> = [PathBuf::from("/swapfile")].into_iter().collect();

        let mut found = Vec::new();
        for candidate in WELL_KNOWN_SWAP_PATHS {
            let path = PathBuf::from(candidate);
            if active.contains(&path) {
                continue;
            }
            if let Some(dev) = probe_swap_file(&path) {
                found.push(dev);
            }
        }
        // /swapfile should NOT appear because it's in the active set
        assert!(!found.iter().any(|d| d.path == PathBuf::from("/swapfile")));
    }
```

- [ ] **Step 5: Run all tests to verify nothing is broken**

Run: `rtk cargo test`

Expected: All tests PASS. The `linux.rs` tests for `parse_proc_swaps` and proptests still pass. The `swap_discovery` tests pass.

- [ ] **Step 6: Run clippy**

Run: `rtk cargo clippy -- -D warnings`

Expected: No warnings.

- [ ] **Step 7: Commit**

```bash
rtk git add src/platform/swap_discovery.rs src/platform/linux.rs
rtk git commit -m "refactor(swap-discovery): move probe_swap_file to shared module"
```

---

### Task 3: Implement `discover_inactive_swap_files` + tests

**Files:**
- Modify: `src/platform/swap_discovery.rs`

- [ ] **Step 1: Write the failing tests for `discover_inactive_swap_files`**

Add these imports at the top of the test module inside `src/platform/swap_discovery.rs`:

```rust
    use std::collections::HashSet;
    use std::path::PathBuf;
```

Add these tests inside the `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn discover_finds_swap_file_matching_pattern() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("swaptop_discover_test");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("swapfile1");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let active = HashSet::new();
        let dirs: &[(&str, &[&str])] = &[(dir.to_str().unwrap(), &["swap*"])];
        let found = discover_inactive_swap_files(&active, dirs);

        assert!(found.iter().any(|d| d.path == path));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_skips_active_paths() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("swaptop_discover_skip_test");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("swapfile");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let active: HashSet<PathBuf> = [path.clone()].into_iter().collect();
        let dirs: &[(&str, &[&str])] = &[(dir.to_str().unwrap(), &["swap*"])];
        let found = discover_inactive_swap_files(&active, dirs);

        assert!(!found.iter().any(|d| d.path == path));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_ignores_non_matching_files() {
        use std::io::Write;
        let dir = std::env::temp_dir().join("swaptop_discover_nomatch_test");
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("readme.txt");
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&buf).unwrap();
        drop(f);

        let active = HashSet::new();
        let dirs: &[(&str, &[&str])] = &[(dir.to_str().unwrap(), &["swap*"])];
        let found = discover_inactive_swap_files(&active, dirs);

        assert!(found.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn discover_ignores_nonexistent_dir() {
        let active = HashSet::new();
        let dirs: &[(&str, &[&str])] = &[("/tmp/swaptop_nonexistent_dir_xyz", &["swap*"])];
        let found = discover_inactive_swap_files(&active, dirs);
        assert!(found.is_empty());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `rtk cargo test --lib platform::swap_discovery -- discover --nocapture`

Expected: FAIL — `discover_inactive_swap_files` does not exist yet.

- [ ] **Step 3: Implement `discover_inactive_swap_files`**

Add this import at the top of `src/platform/swap_discovery.rs` (alongside the existing `use std::path::Path;`):

```rust
use std::collections::HashSet;
use std::path::{Path, PathBuf};
```

And remove the standalone `use std::path::Path;` that was added in Task 2 (it's now covered by the combined import).

Add this function after `probe_swap_file`:

```rust
pub(crate) fn discover_inactive_swap_files(
    active_paths: &HashSet<PathBuf>,
    dirs: &[(&str, &[&str])],
) -> Vec<SwapDevice> {
    let mut devices = Vec::new();
    for &(dir, patterns) in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if active_paths.contains(&path) {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            if patterns.iter().any(|p| matches_pattern(name, p)) {
                if let Some(dev) = probe_swap_file(&path) {
                    devices.push(dev);
                }
            }
        }
    }
    devices
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `rtk cargo test --lib platform::swap_discovery -- --nocapture`

Expected: All 15 tests PASS (8 matches_pattern + 3 probe + 4 discover).

- [ ] **Step 5: Run clippy**

Run: `rtk cargo clippy -- -D warnings`

Expected: No warnings.

- [ ] **Step 6: Commit**

```bash
rtk git add src/platform/swap_discovery.rs
rtk git commit -m "feat(swap-discovery): add discover_inactive_swap_files"
```

---

### Task 4: Wire Linux backend to use `swap_discovery`

**Files:**
- Modify: `src/platform/linux.rs:39-71` (`swap_devices` method)

- [ ] **Step 1: Define `LINUX_SCAN_DIRS` in `linux.rs`**

In `src/platform/linux.rs`, add this constant after the imports (after the `use` block, before `pub struct LinuxBackend`):

```rust
const LINUX_SCAN_DIRS: &[(&str, &[&str])] = &[
    ("/", &["swap*", "*.swap", "*.img"]),
    ("/var", &["swap*", "*.swap"]),
    ("/mnt", &["swap*", "*.swap"]),
];
```

- [ ] **Step 2: Add the `discover_inactive_swap_files` import**

Add to the imports section of `linux.rs`:

```rust
use super::swap_discovery::discover_inactive_swap_files;
```

The full imports section should now be:

```rust
use std::path::{Path, PathBuf};

use color_eyre::Result;
use sysinfo::System;

use super::proc_reader::ProcReader;
use super::swap_discovery::{discover_inactive_swap_files, probe_swap_file};
use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};
use crate::create_swap::detect_swap_magic;
```

(Note: combine both `swap_discovery` imports into one `use` statement.)

- [ ] **Step 3: Replace the `swap_devices` body**

Replace the `swap_devices` method in the `impl SwapBackend for LinuxBackend` block. The old body (lines ~39–71) was:

```rust
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        let content = std::fs::read_to_string("/proc/swaps")?;
        let mut devices = parse_proc_swaps(&content);

        let active_paths: std::collections::HashSet<PathBuf> =
            devices.iter().map(|d| d.path.clone()).collect();

        // Probe well-known file paths for inactive swap files
        for candidate in WELL_KNOWN_SWAP_PATHS {
            let path = PathBuf::from(candidate);
            if active_paths.contains(&path) {
                continue;
            }
            if let Some(dev) = probe_swap_file(&path) {
                devices.push(dev);
            }
        }

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
```

Replace with:

```rust
    fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
        let content = std::fs::read_to_string("/proc/swaps")?;
        let mut devices = parse_proc_swaps(&content);

        let active_paths: std::collections::HashSet<PathBuf> =
            devices.iter().map(|d| d.path.clone()).collect();

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
```

- [ ] **Step 4: Remove unused import of `probe_swap_file` if no longer used directly in `linux.rs`**

Check: `probe_swap_file` is no longer called directly in `linux.rs` (it's called via `discover_inactive_swap_files` inside `swap_discovery.rs`). Remove it from the import:

```rust
use super::swap_discovery::discover_inactive_swap_files;
```

(Instead of `use super::swap_discovery::{discover_inactive_swap_files, probe_swap_file};`)

- [ ] **Step 5: Build to check compilation**

Run: `rtk cargo build`

Expected: Compiles clean with zero warnings.

- [ ] **Step 6: Run all tests**

Run: `rtk cargo test`

Expected: All tests PASS — both `swap_discovery` tests and `linux.rs` tests (parse_proc_swaps, proptests).

- [ ] **Step 7: Run clippy**

Run: `rtk cargo clippy -- -D warnings`

Expected: No warnings.

- [ ] **Step 8: Commit**

```bash
rtk git add src/platform/linux.rs
rtk git commit -m "feat(linux): wire swap_devices to pattern-based discovery"
```

---

### Task 5: Final verification

**Files:**
- None (read-only verification)

- [ ] **Step 1: Full build**

Run: `rtk cargo build`

Expected: Compiles clean.

- [ ] **Step 2: Full test suite**

Run: `rtk cargo test`

Expected: All tests pass.

- [ ] **Step 3: Clippy**

Run: `rtk cargo clippy -- -D warnings`

Expected: No warnings.

- [ ] **Step 4: Verify the final state of `swap_discovery.rs`**

Read `src/platform/swap_discovery.rs` and confirm it contains:
- `matches_pattern` (private)
- `probe_swap_file` (pub(crate))
- `discover_inactive_swap_files` (pub(crate))
- All 15 tests (8 matches_pattern + 3 probe + 4 discover)

- [ ] **Step 5: Verify `linux.rs` no longer has `WELL_KNOWN_SWAP_PATHS` or inline `probe_swap_file`**

Run: `grep -n "WELL_KNOWN_SWAP_PATHS\|fn probe_swap_file" src/platform/linux.rs`

Expected: No matches.

- [ ] **Step 6: Verify `linux.rs` uses `LINUX_SCAN_DIRS` and `discover_inactive_swap_files`**

Run: `grep -n "LINUX_SCAN_DIRS\|discover_inactive_swap_files" src/platform/linux.rs`

Expected: Two matches — the constant definition and the call site.
