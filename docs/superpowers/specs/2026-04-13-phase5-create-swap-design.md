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
CreateSwapReturnToForm,                                 // Progress(error) → Form, preserves inputs
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
                              │  Step 0: Disk space  │
                              │  Step 1: Check file  │
                              │    exists + magic    │
                              │  Step 2: Detect FS   │
                              │  Step 3: allocate    │
                              │    (fallocate/dd)    │
                              │  Step 4: chmod 600   │
                              │  Step 5: mkswap      │
                              │  Step 6: swapon*     │
                              │                      │
                              └──────────┬──────────┘
                                         │
                      ┌──────────────────┴─────────────────┐
                      │ any Error                          │ all Done
                      ▼                                    ▼
          step red + Esc →                   2s delay → CloseCreateSwap
          CreateSwapReturnToForm           (devices list refreshes on
          (inputs preserved)                next tick)
```

*Step 6 (swapon) only runs if `activate_after = true`.

---

## Step Sequence (Background Task)

The background task runs these steps in order. Each step sends
`Action::CreateSwapStepUpdate { index, status }` at its start (Running) and end (Done / Error).

### Step 0: Disk Space Check

First syscall in `spawn_blocking`. Calls `nix::sys::statvfs::statvfs(parent_dir)` on the target
path's parent directory. Requires `available_bytes > requested_bytes + (requested_bytes / 10)`
(10% margin). If insufficient:
`StepStatus::Error("Not enough space: need X GB, have Y GB")` — task stops.

Label: `"Check disk space"`.

### Step 1: File Existence + Swap Magic Check

Before allocating anything, the background task checks:

1. **File does not exist** → Step 1 Done, proceed to Step 2.
2. **File exists, has swap magic** → Step 1 Done, then send
   `Action::OpenConfirmActivateOnly { path, size_bytes }`. The background task exits here.
   The modal transitions from `Progress` to `ConfirmActivateOnly`. The user sees:
   `"Already a swap file (X GB) — press s to activate, Esc to cancel"`.
   If `s` → dispatch `Action::CreateSwapSubmit { activate_only: true }` → spawns a new task
   that runs only Step 6 (swapon).
   If `Esc` → `CloseCreateSwap`.
3. **File exists, no swap magic** → `StepStatus::Error("file exists and is not a swap file — refusing to overwrite")` — task stops.

Label: `"Check target file"`.

**Swap magic detection:** read first 4096 bytes of the file, check bytes 4086–4095 for
`SWAPSPACE2` or `SWAP-SPACE`. File size from `fs::metadata().len()`.

---

### Step 2: Filesystem Detection

Parse `/proc/mounts` to find the mount point covering the target path. Extract the filesystem type
(column 3). Decision table:

| Filesystem         | Allocation method       |
|--------------------|-------------------------|
| ext2/3/4, xfs, f2fs | `fallocate -l <size> <path>` |
| btrfs              | `dd if=/dev/zero bs=1M count=<n>` (user must `chattr +C` first) |
| tmpfs, ramfs       | `dd if=/dev/zero bs=1M count=<n>` |
| unknown / other    | `dd if=/dev/zero bs=1M count=<n>` (safe fallback) |

btrfs accepts `fallocate` but the resulting preallocated extents are rejected by
`swapon` (`EINVAL`). Swap files on btrfs require fully-allocated, non-COW extents,
which only `dd` (on a file with `chattr +C` applied while empty) produces.

Label: `"Detect filesystem"`. On Done, the Step 3 label is updated to reflect the chosen method:
`"Allocate via fallocate (ext4)"` or `"Allocate via dd (tmpfs)"`.

### Steps 3–6

- **Step 3: Allocate** — `fallocate -l <bytes> <path>` or `dd if=/dev/zero of=<path> bs=1M count=<n>` via `tokio::process::Command` (sync wait in blocking task).
- **Step 4: chmod 600** — `nix::sys::stat::fchmodat` or `std::fs::set_permissions`.
- **Step 5: mkswap** — `Command::new("mkswap").arg(path)`.
- **Step 6: swapon** — reuses `LinuxBackend::swap_on(&path)`. Skipped if `activate_after = false`.

---

## Form Validation (sync, in reducer)

Before dispatching `CreateSwapSubmit`, `input.rs` validates the parsed inputs and either
produces `Action::CreateSwapSubmit { activate_only: false }` or
`Action::SetError(msg)` for obvious errors:

| Check                             | Error message                                  |
|-----------------------------------|------------------------------------------------|
| `path_input` empty                | `"Path is required"`                           |
| `path_input` not absolute         | `"Path must be absolute"`                      |
| `size_input` not a positive integer | `"Size must be a positive integer"`          |
| `size_input` parsed size == 0     | `"Size must be greater than zero"`             |
| `priority_input` not parseable as `i16` in `[-1, 32767]` | `"Priority must be an integer between -1 and 32767"` |

Validation errors go into `CreateSwapModal.validation_error` (rendered inline below the form),
not into `AppState.error_msg` — this keeps the modal self-contained.

Disk space is **not** validated here — it's a syscall and reducers are pure/no I/O. The
`spawn_blocking` task's Step 0 covers it.

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

**Form mode** — dispatch depends on the focused field:

| Key               | Focused field              | Action                                          |
|-------------------|----------------------------|-------------------------------------------------|
| `Esc`             | any                        | `CloseCreateSwap` (back to Devices)             |
| `↑` / `k`        | any                        | `CreateSwapFocusField(prev_field)`              |
| `↓` / `j`        | any                        | `CreateSwapFocusField(next_field)`              |
| `Enter`           | `Submit`                   | validate → `CreateSwapSubmit { activate_only: false }` or stay with `validation_error` |
| `Space`           | `SizeUnit`                 | `CreateSwapToggleUnit`                          |
| `Space`           | `ActivateAfter`            | `CreateSwapToggleActivate`                      |
| `Space`           | `Path` / `Size` / `Priority` | `CreateSwapInputEvent(event)` — space as char |
| any char / `Backspace` / `←` / `→` / `Home` / `End` | `Path` / `Size` / `Priority` | `CreateSwapInputEvent(event)` |
| unhandled         | any                        | ignored                                         |

Rationale: `↑`/`↓` on a text field with no vertical history are unambiguous (we intercept them
for focus). Space is context-sensitive: it's a valid path character, so it must pass through
to the tui-input when a text field is focused.

**ConfirmActivateOnly mode:**

| Key    | Action                                          |
|--------|-------------------------------------------------|
| `s`    | `CreateSwapSubmit { activate_only: true }`      |
| `Esc`  | `CloseCreateSwap`                               |

**Progress mode:**

| Key    | Condition                              | Action                   |
|--------|----------------------------------------|--------------------------|
| `Esc`  | Task still at Step 0, 1, or 2          | `CloseCreateSwap` (safe — nothing written yet) |
| `Esc`  | Task at Step 3 or later                | ignored — file write in progress, cancelling would leave a partial file |
| `Esc`  | Any step has `StepStatus::Error(...)`  | `CreateSwapReturnToForm` (error recovery — inputs preserved, user can fix and retry) |

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
tui-input = { version = "0.15", features = ["crossterm"] }
```

(Latest stable on crates.io at 2026-04-13: `0.15.1`. Verify compatibility with
`crossterm 0.29` and `ratatui 0.30` during implementation.)

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
