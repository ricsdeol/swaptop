# Phase 5 — Create Swap File: Design Spec

**Date:** 2026-04-13
**Status:** Approved

---

## Overview

Phase 5 adds a wizard modal for creating a new swap file. The wizard is accessed via `n` on the
Devices tab (Tab 3), **not** a separate tab — `Tab::CreateSwap` is removed from the codebase.
The modal overlays the Devices tab and runs a step-by-step progress flow via the existing
`action_tx` channel, consistent with the Phase 4 `DeviceOp` pattern.

---

## What Changes vs the Spec

| Spec (original)             | Design decision                                      |
|-----------------------------|------------------------------------------------------|
| Tab 4 — Create Swap         | Removed; modal on Devices tab via `n`                |
| `tui-textarea`              | Replaced by `tui-input` (single-line, no extra bulk) |
| `fallocate` with `dd` fallback triggered on error | Filesystem detected upfront via `/proc/mounts` |
| No pre-existence check      | Step 0 checks file existence + swap magic bytes      |

---

## Removed: `Tab::CreateSwap`

The following are removed across the codebase:

- `Tab::CreateSwap` variant from `app.rs`
- `Action::SelectTab(4)` and keybinding `'4'` from `input.rs`
- `render_coming_soon()` from `ui/mod.rs`
- Tab bar shrinks to 3 entries: Overview · Processes · Devices
- Tab cycling: Overview → Processes → Devices → Overview

---

## New Module: `src/create_swap.rs`

All Phase 5 types live here. `actions.rs` receives only the new `Action` variants.

```rust
/// Operating mode of the create-swap modal.
pub enum CreateSwapMode {
    Form { focused_field: CreateSwapField },
    Progress { steps: Vec<CreateSwapStep> },
    /// File exists and already has swap magic — ask user to just activate it.
    ConfirmActivateOnly { path: PathBuf, size_bytes: u64 },
}

/// Focusable fields in the form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CreateSwapField {
    Path,
    Size,
    SizeUnit,
    Priority,
    ActivateAfter,
    Submit,
}

/// Size unit selector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeUnit {
    Mb,
    Gb,
}

impl SizeUnit {
    pub fn label(self) -> &'static str {
        match self {
            SizeUnit::Mb => "MB",
            SizeUnit::Gb => "GB",
        }
    }
}

/// A single wizard step.
pub struct CreateSwapStep {
    pub label: String,
    pub status: StepStatus,
}

/// Status of each step. Cannot be Copy due to Error(String) payload.
#[derive(Debug, Clone, PartialEq)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Error(String),
}

/// Full modal state stored in AppState.
pub struct CreateSwapModal {
    pub mode: CreateSwapMode,
    // tui-input widgets — one per text field
    pub path_input:     tui_input::Input,
    pub size_input:     tui_input::Input,
    pub priority_input: tui_input::Input,
    pub size_unit:      SizeUnit,
    pub activate_after: bool,
    pub validation_error: Option<String>,
}

impl Default for CreateSwapModal {
    fn default() -> Self {
        Self {
            mode: CreateSwapMode::Form {
                focused_field: CreateSwapField::Path,
            },
            path_input:     tui_input::Input::default(),
            size_input:     tui_input::Input::from("2"),
            priority_input: tui_input::Input::from("0"),
            size_unit:      SizeUnit::Gb,
            activate_after: true,
            validation_error: None,
        }
    }
}
```

---

## AppState Changes (`app.rs`)

```rust
// Phase 5
pub create_swap_modal: Option<CreateSwapModal>,
```

Initialized as `None`. Set to `Some(CreateSwapModal::default())` on `Action::OpenCreateSwap`,
cleared back to `None` on `Action::CloseCreateSwap`.

---

## New Action Variants (`actions.rs`)

```rust
// Phase 5 — create swap modal
OpenCreateSwap,
CloseCreateSwap,
CreateSwapFocusField(CreateSwapField),
CreateSwapInputEvent(crossterm::event::Event),          // routed to active tui-input
CreateSwapToggleUnit,
CreateSwapToggleActivate,
CreateSwapSubmit { activate_only: bool },               // false = full wizard, true = swapon only
OpenConfirmActivateOnly { path: PathBuf, size_bytes: u64 }, // file already has swap magic
CreateSwapStepUpdate { index: usize, status: StepStatus },
```

---

## Modal State Machine

```
Devices tab (root)
  │
  ├─ n (not root)  ──────────────→  Action::SetError("Requires root — run: sudo swaptop")
  │
  └─ n (is root)   ──────────────→  Action::OpenCreateSwap
                                         │
                              ┌──────────▼──────────┐
                              │      Form mode       │
                              │  ↑/↓  move focus     │
                              │  ←/→ / chars → tui-input active field
                              │  Space  toggle checkbox / unit
                              │  Enter on Submit → validate → CreateSwapSubmit
                              │  Esc  → CloseCreateSwap
                              └──────────┬──────────┘
                                         │ validation passes
                              ┌──────────▼──────────┐
                              │   Progress mode      │
                              │                      │
                              │  Step 0: Check file  │
                              │    exists + magic    │
                              │  Step 1: Detect FS   │
                              │  Step 2: allocate    │
                              │    (fallocate/dd)    │
                              │  Step 3: chmod 600   │
                              │  Step 4: mkswap      │
                              │  Step 5: swapon*     │
                              │                      │
                              └──────────┬──────────┘
                                         │
                      ┌──────────────────┴─────────────────┐
                      │ any Error                          │ all Done
                      ▼                                    ▼
               step red + Esc          2s delay → CloseCreateSwap
               returns to Form                  (devices list refreshes
                                                 on next tick)
```

*Step 5 (swapon) only runs if `activate_after = true`.

---

## Step 0: File Existence + Swap Magic Check

Before allocating anything, the background task checks:

1. **File does not exist** → proceed to Step 1.
2. **File exists, has swap magic** → send `Action::CreateSwapStepUpdate` with a special
   `StepStatus::Done` label, then send `Action::OpenConfirmActivateOnly { path, size_bytes }`.
   The modal transitions to `CreateSwapMode::ConfirmActivateOnly`. The user sees:
   `"Already a swap file (X GB) — press s to activate, Esc to cancel"`.
   If `s` → dispatch `Action::CreateSwapSubmit` with `activate_only = true` → runs only swapon.
   If `Esc` → `CloseCreateSwap`.
3. **File exists, no swap magic** → `StepStatus::Error("file exists and is not a swap file — refusing to overwrite")`.

**Swap magic detection:** read first 4096 bytes of the file, check bytes 4086–4095 for
`SWAPSPACE2` or `SWAP-SPACE`. File size from `fs::metadata().len()`.

---

## Step 1: Filesystem Detection

Parse `/proc/mounts` to find the mount point covering the target path. Extract the filesystem type
(column 3). Decision table:

| Filesystem         | Allocation method       |
|--------------------|-------------------------|
| ext2/3/4, xfs, f2fs, btrfs* | `fallocate -l <size> <path>` |
| tmpfs, ramfs       | `dd if=/dev/zero bs=1M count=<n>` |
| unknown / other    | `dd if=/dev/zero bs=1M count=<n>` (safe fallback) |

*btrfs supports `fallocate` but creates a non-contiguous file — acceptable for swap on modern kernels (≥5.0).

The step label includes the detected method, e.g.:
`"Allocate via fallocate (ext4)"` or `"Allocate via dd (tmpfs)"`.

---

## Disk Space Validation

Validation happens in two places:

**UI (non-blocking, for display):** The form shows `Free on /: X GB` using the last
`MemSnapshot` (from `sysinfo::Disks`) — updated every tick, no syscall at render time.

**Pre-flight check (in `spawn_blocking`, Step 0):** The background task calls
`nix::sys::statvfs::statvfs(parent_dir)` as its very first check, before the swap magic check.
Requires `available > requested_bytes + 10%` margin. If insufficient:
`StepStatus::Error("Not enough space: need X GB, have Y GB")` — task stops immediately.

Disk space is **not** validated in the AppState reducer — reducers are pure/no I/O.

---

## Background Task Pattern

Mirrors `ExecuteDeviceOp` in `main.rs`:

```rust
if let Some(Action::CreateSwapSubmit) = action {
    // ... read form values from AppState, clone what's needed ...
    let tx = action_tx.clone();
    tokio::task::spawn_blocking(move || {
        run_create_swap_steps(path, size_bytes, priority, activate_after, tx);
    });
}
```

`run_create_swap_steps` sends `Action::CreateSwapStepUpdate { index, status }` after each
step. Each step returns `Result<(), String>` internally; errors use `?` for propagation within
helper functions. No `unwrap` outside tests.

---

## Form Navigation (tui-input + focus model)

Key handling when modal is open (intercept **before** routing to tui-input):

**Form mode:**

| Key               | Action                                                   |
|-------------------|----------------------------------------------------------|
| `Esc`             | `CloseCreateSwap` (back to Devices)                      |
| `↑` / `k`        | `CreateSwapFocusField(prev_field)`                       |
| `↓` / `j`        | `CreateSwapFocusField(next_field)`                       |
| `Space`           | Toggle checkbox / cycle unit (ActivateAfter / SizeUnit)  |
| `Enter` on Submit | `CreateSwapSubmit`                                       |
| all others        | `CreateSwapInputEvent(event)` → active tui-input         |

**ConfirmActivateOnly mode:**

| Key    | Action                                          |
|--------|-------------------------------------------------|
| `s`    | `CreateSwapSubmit` with `activate_only = true`  |
| `Esc`  | `CloseCreateSwap`                               |

**Progress mode:**

| Key    | Condition                        | Action            |
|--------|----------------------------------|-------------------|
| `Esc`  | Before Step 2 (allocate) starts  | `CloseCreateSwap` |
| `Esc`  | Step 2 or later in progress      | ignored — file write in progress, cancelling would leave a partial file |

`Tab` is **not** used for field navigation — it remains the global tab-switch key (cycles 3 tabs).

---

## UI Layout (`ui/create_swap.rs`)

Modal overlays the Devices tab area. Two visual states:

### Form mode

```
┌─ New Swap File ─────────────────────────────────┐
│                                                  │
│  Path:      [/swapfile2                       ]  │
│  Size:      [2        ] [GB]                     │
│  Priority:  [0        ]                          │
│  Activate:  [x] activate after create            │
│                                                  │
│  Free on /: 45.2 GB                              │
│                                                  │
│  ▶ [  Create  ]                                  │
│                                                  │
│  ↑/↓ navigate · Space toggle · Esc cancel        │
└──────────────────────────────────────────────────┘
```

Active field highlighted with `Color::Cyan` border. Validation error shown in `Color::Red`
below the form. Disk free space updates from `AppState.current` (last snapshot).

### Progress mode

```
┌─ Creating /swapfile2 ───────────────────────────┐
│                                                  │
│  ✓  File check (does not exist)                  │
│  ✓  Detect filesystem → fallocate (ext4)         │
│  ✓  fallocate -l 2147483648 /swapfile2           │
│  ✓  chmod 600 /swapfile2                         │
│  ⏳  mkswap /swapfile2...                        │
│     swapon /swapfile2                            │
│                                                  │
│  Esc cancel (if not yet writing)                 │
└──────────────────────────────────────────────────┘
```

Step icons: `✓` (Done, green) · `⏳` (Running, yellow) · `✗` (Error, red) · `·` (Pending, gray).

---

## `docs/devices.md` Update

Add `n` keybinding to the Devices tab documentation:

```markdown
| `n` | Create new swap file (modal wizard, requires root) |
```

Update the tab reference from `1-4` to `1-3`.

---

## Cargo.toml

Add `tui-input`:

```toml
tui-input = { version = "0.11", features = ["crossterm"] }
```

Remove: no crate is removed (no `tui-textarea` was added yet).

---

## Files Touched Summary

| File                          | Change                                              |
|-------------------------------|-----------------------------------------------------|
| `src/create_swap.rs`          | **New** — all Phase 5 types                         |
| `src/actions.rs`              | Add Phase 5 `Action` variants                       |
| `src/app.rs`                  | Remove `Tab::CreateSwap`, add `create_swap_modal`   |
| `src/input.rs`                | Remove `SelectTab(4)` / `'4'` key, add Phase 5 key routing |
| `src/main.rs`                 | Add `CreateSwapSubmit` spawn, route `CreateSwapInputEvent` |
| `src/ui/mod.rs`               | Remove `Tab::CreateSwap` arm + `render_coming_soon` |
| `src/ui/create_swap.rs`       | **New** — modal render (Form + Progress)            |
| `src/ui/devices.rs`           | Add `n` key hint in footer                          |
| `docs/devices.md`             | Add `n` keybinding row, update tab range to `1-3`   |
| `Cargo.toml`                  | Add `tui-input`                                     |
