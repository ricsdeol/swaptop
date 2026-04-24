# Swap File Glob Discovery Design

**Date:** 2026-04-18
**Status:** Approved

---

## Goal

Replace the static `WELL_KNOWN_SWAP_PATHS` exact-path list in `linux.rs` with a pattern-based directory scanner that can find swap files named `swapfile1`, `swapfile2`, etc. Extract the logic into a shared `swap_discovery` module reusable by all platform backends (Linux, BSD, macOS).

## Problem

`WELL_KNOWN_SWAP_PATHS` only matches exact filenames:

```rust
const WELL_KNOWN_SWAP_PATHS: &[&str] = &["/swapfile", "/var/swapfile", "/swap", "/swap.img"];
```

Files like `/swapfile1`, `/var/swap2`, or `/mnt/swapfile` are silently missed. Additionally, the discovery logic and `probe_swap_file` function are buried in `linux.rs`, preventing reuse by BSD and macOS backends.

---

## Architecture

### Files Changed

| File | Action | Responsibility |
|---|---|---|
| `src/platform/swap_discovery.rs` | **Create** | Pattern-based directory scanning; `probe_swap_file` |
| `src/platform/linux.rs` | **Modify** | Remove `WELL_KNOWN_SWAP_PATHS` + `probe_swap_file`; define `LINUX_SCAN_DIRS`; call `swap_discovery::discover_inactive_swap_files` |
| `src/platform/mod.rs` | **Modify** | Add `pub(crate) mod swap_discovery` |

### Call Flow

```
LinuxBackend::swap_devices()
  └─ build active_paths HashSet from /proc/swaps
  └─ swap_discovery::discover_inactive_swap_files(&active_paths, LINUX_SCAN_DIRS)
        └─ for each (dir, patterns):
              read_dir(dir)
              → filter entries by matches_pattern(file_name, pattern)
              → probe_swap_file(path)   // reads magic header
              → push SwapDevice { active: false, ... }
  └─ probe block devices in /dev/ (unchanged, stays in linux.rs)
```

---

## `swap_discovery.rs` — Public API

```rust
use std::{collections::HashSet, path::{Path, PathBuf}};
use crate::platform::{SwapDevice, SwapKind};
use crate::platform::parse_swap_header;

/// Scan `dirs` for inactive swap files whose names match a pattern.
/// Skips any path already present in `active_paths`.
/// Silently ignores permission errors and non-swap files.
pub(crate) fn discover_inactive_swap_files(
    active_paths: &HashSet<PathBuf>,
    dirs: &[(&str, &[&str])],
) -> Vec<SwapDevice>

/// Return a SwapDevice if `path` is a regular file with a valid swap magic header.
/// Returns None on any I/O error, permission error, or missing magic.
pub(crate) fn probe_swap_file(path: &Path) -> Option<SwapDevice>

/// Return true if `name` matches `pattern`, where `*` is a single wildcard.
/// Supports: "swap*", "*.swap", "swapfile*", "*.img".
/// Only one `*` per pattern is supported.
fn matches_pattern(name: &str, pattern: &str) -> bool
```

### `matches_pattern` implementation

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

No external crate required. Handles all real-world swap patterns.

---

## Platform Scan Configurations

Only `linux.rs` receives a `LINUX_SCAN_DIRS` constant in this task. The BSD and macOS constants below are **documented here for reference only** and must NOT be added to `bsd.rs`/`macos.rs` yet — unused constants would fail `cargo clippy -- -D warnings`. They belong in a future task when those backends are implemented.

### Linux (`linux.rs`)

Research sources: ArchWiki, Ubuntu SwapFaq, Red Hat Storage Guide, serverdevworker.com.

```rust
const LINUX_SCAN_DIRS: &[(&str, &[&str])] = &[
    ("/",    &["swap*", "*.swap", "*.img"]),  // /swapfile, /swapfile1, /swap.img
    ("/var", &["swap*", "*.swap"]),            // /var/swap, /var/swapfile
    ("/mnt", &["swap*", "*.swap"]),            // /mnt/swap (dedicated disk setups)
];
```

### macOS (`macos.rs`) — future use

Research source: Apple `dynamic_pager(8)` man page, OSXDaily.

`dynamic_pager` writes to `/private/var/vm/swapfile0`, `swapfile1`, etc. All swap is managed by the OS; no manual swapon/swapoff.

```rust
const MACOS_SCAN_DIRS: &[(&str, &[&str])] = &[
    ("/private/var/vm", &["swapfile*"]),
];
```

### BSD (`bsd.rs`) — future use

Research source: OpenBSD `swapctl(8)` man page, FreeBSD Klara Systems article.

FreeBSD/OpenBSD configure swap in `/etc/fstab` with type `sw`. Files typically live in `/` or `/var`. Block device discovery (`/dev/ada0s1b`, `/dev/da0s1b`) requires BSD-specific ioctl and is deferred to full BSD backend implementation.

```rust
const BSD_SCAN_DIRS: &[(&str, &[&str])] = &[
    ("/",    &["swap*"]),
    ("/var", &["swap*"]),
];
```

---

## What Does NOT Change

- `probe_swap_device` (block device probing via `BLKGETSIZE64` ioctl) stays in `linux.rs` — it uses Linux-specific ioctls.
- `parse_swap_header` stays in `platform::types` — it is called from both `swap_discovery` and the create-swap wizard.
- `parse_proc_swaps` stays in `linux.rs` — it is Linux-specific.
- BSD and macOS backends continue returning `bail!` on all methods.

---

## Testing

### Unit tests in `swap_discovery.rs`

- `matches_pattern` — cover: exact match, `swap*`, `*.swap`, `*.img`, no-match cases, empty name, empty pattern
- `probe_swap_file` — nonexistent path → `None`; zeroed file → `None`; file with `SWAPSPACE2` magic at offset 4086 → `Some(SwapDevice)`
- `discover_inactive_swap_files` — skips active paths; finds files matching pattern; ignores non-swap files

### Existing tests in `linux.rs`

All existing tests for `parse_proc_swaps` and `probe_swap_file` remain valid. The `probe_swap_file` tests move to `swap_discovery.rs`.

---

## Constraints

- No new crate dependencies.
- `pub(crate)` visibility only — `swap_discovery` is internal infrastructure.
- `matches_pattern` supports exactly one `*` per pattern. Multi-wildcard patterns are not needed and not supported.
- Discovery is best-effort: any `read_dir` or `open` error is silently skipped (permission errors on `/mnt` are expected on most systems).
