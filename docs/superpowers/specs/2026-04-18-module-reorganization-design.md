# Module Reorganization Design

## Problem

Three architectural issues in `src/platform/` and `src/create_swap.rs`:

1. **Dependency inversion**: `platform/linux.rs` and `platform/swap_discovery.rs` import `detect_swap_magic` from `src/create_swap.rs`. The platform layer depends on the app layer — dependencies should flow downward (app → platform), never upward.

2. **Mixed responsibilities**: `src/create_swap.rs` combines UI state types (`CreateSwapModal`, `CreateSwapField`, etc.) with platform I/O operations (`run_create_swap_steps`, `detect_fs_type`, `Allocator`).

3. **Flat Linux internals**: `proc_reader.rs` and `swap_discovery.rs` are Linux-specific implementation details exposed at the `platform/` module level. They should be scoped under a Linux-specific submodule.

## Decision

Option A with cross-platform awareness: split by responsibility, group Linux-specific code in `platform/linux/`, keep genuinely cross-platform helpers at the `platform/` level.

## Target Structure

```
src/
├── create_swap.rs              ← app-layer state types only
│
├── platform/
│   ├── mod.rs                  ← SwapBackend trait + module declarations
│   ├── types.rs                ← shared data types + parse_swap_header
│   ├── factory.rs              ← detect() → Box<dyn SwapBackend>
│   │
│   ├── swap_discovery.rs       ← cross-platform helpers
│   │                              matches_pattern, probe_swap_file,
│   │                              discover_inactive_swap_files
│   │
│   ├── linux/
│   │   ├── mod.rs              ← LinuxBackend (was linux.rs)
│   │   ├── proc_reader.rs      ← /proc parser (moved from platform/)
│   │   └── create_swap.rs      ← run_create_swap_steps, detect_fs_type,
│   │                              allocator_for_fs, Allocator,
│   │                              do_swapon, do_swapon_with_priority,
│   │                              check_disk_space, check_target_file,
│   │                              run_cmd, TargetFileCheck
│   │
│   ├── macos.rs                ← stub (becomes macos/ when it grows)
│   ├── bsd.rs                  ← stub (becomes bsd/ when it grows)
│   └── windows.rs              ← stub (becomes windows/ when it grows)
│
└── ui/
    └── create_swap.rs          ← Ratatui renderer (no change)
```

## Dependency Flow (Corrected)

```
platform/types.rs                ← lowest level, zero dependencies
        ↑
platform/swap_discovery.rs       ← depends on platform/types
        ↑
platform/linux/proc_reader.rs    ← depends on platform/types
platform/linux/create_swap.rs    ← depends on platform/types + crate::actions
platform/linux/mod.rs            ← depends on linux/* + platform/swap_discovery
        ↑
src/create_swap.rs               ← app-layer state, no platform imports
src/app.rs                       ← depends on create_swap + platform/types
src/main.rs                      ← depends on platform::linux::create_swap
```

The inversion is eliminated: `platform/` never imports from `src/create_swap.rs`.

## What Changes Per File

### `src/create_swap.rs` — strip to state types only

**Stays:**
- `CreateSwapMode`, `CreateSwapField`, `SizeUnit`, `CreateSwapStep`, `StepStatus`, `CreateSwapModal`
- `impl Default for CreateSwapModal`
- `CreateSwapField::next()`, `prev()`
- `SizeUnit::label()`, `multiplier()`, `toggled()`
- `CreateSwapStep::pending()`
- All tests for the above

**Moves out:**
- `detect_swap_magic` → `platform/types.rs` as `parse_swap_header` (rename)
- `detect_fs_type`, `allocator_for_fs`, `Allocator` → `platform/linux/create_swap.rs`
- `run_create_swap_steps` → `platform/linux/create_swap.rs`
- `do_swapon`, `do_swapon_with_priority` → `platform/linux/create_swap.rs`
- `check_disk_space`, `check_target_file`, `TargetFileCheck` → `platform/linux/create_swap.rs`
- `run_cmd` → `platform/linux/create_swap.rs`
- All `use std::fs`, `use std::io::Read`, `use std::process::Command`, `use nix`, `use tokio::sync::mpsc` → move with the functions

**Removes:**
- `use std::fs`, `use std::io::Read`, `use std::os::unix::fs::PermissionsExt`, `use std::process::Command`
- `use tokio::sync::mpsc::UnboundedSender`
- `use crate::actions::Action`

### `platform/types.rs` — add `parse_swap_header`

New function (renamed from `detect_swap_magic`):

```rust
/// Check the first 4096 bytes for Linux swap magic (`SWAPSPACE2` or `SWAP-SPACE`)
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

Tests for `parse_swap_header` move here from `src/create_swap.rs`.

### `platform/swap_discovery.rs` — update import only

```rust
// Before:
use crate::create_swap::detect_swap_magic;

// After:
use crate::platform::types::parse_swap_header;
// (or just `use super::parse_swap_header` since types is re-exported via mod.rs)
```

`matches_pattern`, `probe_swap_file`, `discover_inactive_swap_files` stay here unchanged — they are cross-platform by design. Any future BSD/macOS backend can call `discover_inactive_swap_files` with its own scan dirs.

### `platform/linux.rs` → `platform/linux/mod.rs`

- File moves from `platform/linux.rs` to `platform/linux/mod.rs`
- Declares submodules: `mod proc_reader;` and `pub(crate) mod create_swap;`
- Updates import: `use crate::create_swap::detect_swap_magic` → `use super::parse_swap_header`
- `LINUX_SCAN_DIRS` stays here (Linux-specific constant)

### `platform/linux/proc_reader.rs` — move only

- Moves from `platform/proc_reader.rs` to `platform/linux/proc_reader.rs`
- Update `use crate::platform::ProcessRow` — no change needed (path still works)
- Zero code changes

### `platform/linux/create_swap.rs` — new file

Receives from `src/create_swap.rs`:
- `run_create_swap_steps` (pub)
- `detect_fs_type` (pub for tests)
- `allocator_for_fs` (pub for tests)
- `Allocator` (pub)
- `do_swapon`, `do_swapon_with_priority` (private)
- `check_disk_space`, `check_target_file`, `TargetFileCheck` (private)
- `run_cmd` (private)
- All associated tests

Imports:
```rust
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command as StdCommand;

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::Action;
use crate::create_swap::StepStatus;
use super::parse_swap_header;
```

Note: `platform/linux/create_swap.rs` imports `StepStatus` from `src/create_swap.rs` (app layer). This is acceptable: the runner needs to communicate step status via `Action::CreateSwapStepUpdate` to the app. The dependency flows `platform/linux/create_swap.rs` → `crate::actions` → `crate::create_swap::StepStatus`. This is a narrow, intentional coupling: the runner sends status updates using the app's vocabulary.

### `platform/mod.rs` — update module declarations

```rust
// Before:
pub mod linux;
pub mod proc_reader;
pub(crate) mod swap_discovery;

// After:
pub mod linux;              // now a directory module
pub(crate) mod swap_discovery;
// proc_reader removed — now linux::proc_reader
```

### `src/main.rs` — update import path

```rust
// Before:
use create_swap::run_create_swap_steps;

// After:
use platform::linux::create_swap::run_create_swap_steps;
```

## Cross-Platform Design Principle

The placement of a file signals its portability:

| Location | Meaning |
|----------|---------|
| `platform/*.rs` (top-level) | Cross-platform: usable by any backend |
| `platform/linux/*.rs` | Linux-only implementation detail |
| `platform/bsd/*.rs` (future) | BSD-only implementation detail |
| `platform/macos/*.rs` (future) | macOS-only implementation detail |

When a future backend (e.g., BSD) needs swap file discovery, it imports `platform::swap_discovery::discover_inactive_swap_files` and provides its own scan directories. No code duplication needed.

Stubs (`macos.rs`, `bsd.rs`, `windows.rs`) stay as flat files until they accumulate enough logic to warrant a directory. Converting to a directory is a future decision per-OS, following the same pattern as `linux/`.

## Rename: `detect_swap_magic` → `parse_swap_header`

The old name used "magic" (Unix jargon for file-type identifier bytes). The new name `parse_swap_header` aligns with the project's existing `parse_*` naming convention (`parse_proc_swaps`, `parse_status`, `parse_swap_line`, `parse_stat_cpu_ticks`) and is self-descriptive without domain jargon.

## Risks

- **Import path churn**: Many files reference `crate::create_swap::detect_swap_magic` — all must be updated. Mechanical but tedious; `cargo build` catches all misses.
- **`StepStatus` coupling**: `platform/linux/create_swap.rs` imports from `crate::create_swap::StepStatus`. This is intentional and narrow, not a layering violation — it's the runner's output vocabulary.
- **Test relocation**: Tests move with their functions. No test logic changes, only import paths.
