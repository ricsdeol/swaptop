# Phase 4 — Swap Device Management: Design Spec

**Date:** 2026-04-11  
**Branch:** `feature/create-swap-ui`  
**Scope:** `src/ui/devices.rs`, `src/app.rs`, `src/actions.rs`, `src/platform/linux.rs`, `src/ui/mod.rs`

---

## Overview

Phase 4 adds the Devices tab (key `3`): a list of active swap devices with the ability to activate (`swapon`), deactivate (`swapoff`), and reset (swapoff + swapon) each device. Operations run asynchronously with inline spinner feedback. Destructive actions require a confirmation modal. All control operations require root; non-root users see an error in the statusbar.

---

## State Changes (`app.rs`)

### New fields on `AppState`

```rust
pub selected_dev:   usize,                // currently highlighted row index
pub device_op:      Option<DeviceOp>,     // in-flight async operation
pub confirm_action: Option<DeviceOpKind>, // pending confirmation (modal open)
```

### New types (defined in `actions.rs`, imported by `app.rs`)

`DeviceOpKind`, `DeviceOp`, and `OpStatus` live in `actions.rs` to avoid a circular dependency (`app.rs` already imports `Action` from `actions.rs`; placing these types there keeps the dependency one-way).

```rust
// actions.rs
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceOpKind { SwapOn, SwapOff, SwapReset }

#[derive(Debug, Clone, PartialEq)]
pub enum OpStatus { Running, Done, Error(String) }

#[derive(Debug, Clone)]
pub struct DeviceOp {
    pub path:   PathBuf,
    pub kind:   DeviceOpKind,
    pub status: OpStatus,
}
```

`app.rs` imports them with:
```rust
use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
```

### `handle_action` additions

| Action | State mutation |
|--------|---------------|
| `DeviceUp` | `selected_dev = selected_dev.saturating_sub(1)` |
| `DeviceDown` | `selected_dev = (selected_dev + 1).min(devices.len().saturating_sub(1))` |
| `RequestConfirm(kind)` | `confirm_action = Some(kind)` |
| `CancelConfirm` | `confirm_action = None` |
| `ExecuteDeviceOp { path, kind }` | `device_op = Some(DeviceOp { Running })`, `confirm_action = None` |
| `DeviceOpUpdate(op)` | `device_op = Some(op)` |
| `SetError(msg)` | `error_msg = Some(msg)` |

---

## Actions (`actions.rs`)

```rust
// Phase 4 — device list navigation
DeviceUp,
DeviceDown,

// Phase 4 — operation flow
RequestConfirm(DeviceOpKind),
CancelConfirm,
ExecuteDeviceOp { path: PathBuf, kind: DeviceOpKind },
DeviceOpUpdate(DeviceOp),

// Phase 4 — error reporting
SetError(String),
```

### Keybinding map (active only on `Tab::Devices`)

| Key | Context | Action |
|-----|---------|--------|
| `j` / `↓` | list | `DeviceDown` |
| `k` / `↑` | list | `DeviceUp` |
| `o` | list, is root, no modal | `RequestConfirm(SwapOn)` |
| `f` | list, is root, no modal | `RequestConfirm(SwapOff)` |
| `r` | list, is root, no modal | `RequestConfirm(SwapReset)` |
| `o` / `f` / `r` | list, not root | `SetError("Requires root — run: sudo swaptop")` |
| `s` | modal open | `ExecuteDeviceOp { path, kind }` + spawn async task |
| `Esc` | modal open | `CancelConfirm` |

Root check uses `nix::unistd::geteuid().is_root()` in `events.rs` — `handle_action` stays pure (no syscalls).

---

## Platform (`linux.rs`)

Implement `swap_on`, `swap_off`, and override `swap_reset`:

```rust
fn swap_on(&self, device: &Path) -> Result<()> {
    nix::mount::swapon(device, None)
        .map_err(|e| color_eyre::eyre::eyre!("swapon failed: {e}"))
}

fn swap_off(&self, device: &Path) -> Result<()> {
    nix::mount::swapoff(device)
        .map_err(|e| color_eyre::eyre::eyre!("swapoff failed: {e}"))
}

fn swap_reset(&self, device: &Path) -> Result<()> {
    self.swap_off(device)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    self.swap_on(device)
}
```

Async task pattern in `events.rs` (spawned on `ExecuteDeviceOp`):

```rust
let tx = action_tx.clone();
tokio::task::spawn_blocking(move || {
    let backend = LinuxBackend::new();
    let result = match kind {
        DeviceOpKind::SwapOn    => backend.swap_on(&path),
        DeviceOpKind::SwapOff   => backend.swap_off(&path),
        DeviceOpKind::SwapReset => backend.swap_reset(&path),
    };
    let status = match result {
        Ok(_)  => OpStatus::Done,
        Err(e) => OpStatus::Error(e.to_string()),
    };
    let _ = tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
});
```

`spawn_blocking` is used because `swapon`/`swapoff` are blocking syscalls and `swap_reset` contains `std::thread::sleep`. This keeps the tokio executor threads unblocked. A fresh `LinuxBackend` instance is created per operation — no shared state with the collector backend.

---

## UI (`src/ui/devices.rs`)

### Layout

```
┌─ content area ──────────────────────────────────────┐
│  [0] header row   Length(1)                         │
│  [1] device list  Min(0)                            │
│  [2] footer hints Length(2)                         │
└─────────────────────────────────────────────────────┘
```

### Table columns

| Column | Constraint |
|--------|-----------|
| Path | `Min(20)` |
| Type | `Length(10)` |
| Total | `Length(9)` |
| Used | `Length(9)` |
| % | `Length(5)` |
| Pri | `Length(5)` |
| Status | `Length(10)` |

### Status column rendering

| Condition | Display |
|-----------|---------|
| `device_op.path == row.path && Running` | `⏳ ...` (yellow) |
| `device_op.path == row.path && Done` | `✓ OK` (green) |
| `device_op.path == row.path && Error(_)` | `✗ ERROR` (red) |
| `device.active == true` | `ACTIVE` (green) |
| `device.active == false` | `INACTIVE` (dark gray) |

### Confirmation modal

Centered overlay, 60% terminal width, height 7:

```
┌─ Confirm ────────────────────────────────┐
│                                          │
│  Deactivate /dev/sda2?                   │
│                                          │
│  [s] confirm    [Esc] cancel             │
│                                          │
└──────────────────────────────────────────┘
```

Rendered using `ratatui::widgets::Clear` before the block to erase the content beneath.

### Footer (2 lines)

- Line 1: `[o] activate  [f] deactivate  [r] reset  [j/k] navigate`
- Line 2 (Linux, capabilities ok): empty (root errors go to statusbar)
- Line 2 (macOS / `!can_swap_on`): `Managed by dynamic_pager — control unavailable` (yellow)

---

## Wiring

### `ui/mod.rs`

```rust
match state.active_tab {
    Tab::Overview => overview::render(f, layout[1], state),
    Tab::Devices  => devices::render(f, layout[1], state),
    _             => render_coming_soon(f, layout[1]),
}
```

### `statusbar.rs`

No changes — already renders `state.error_msg`.

---

## Error strings (all in English)

| Situation | Message |
|-----------|---------|
| Not root, tries action | `"Requires root — run: sudo swaptop"` |
| swapon fails | `"swapon failed: <nix error>"` |
| swapoff fails | `"swapoff failed: <nix error>"` |
| macOS capability missing | `"Managed by dynamic_pager — control unavailable"` |

---

## Out of scope for Phase 4

- Device detail screen on `Enter` (deferred to a future phase)
- `pkexec` / automatic privilege escalation
- Persisting inactive-but-known devices across sessions
- zram-specific controls (`zramctl`)
