# Phase 4 — Swap Device Management: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the Devices tab (key `3`) with a live device table, async swapon/swapoff/reset via nix, confirmation modal, and root-check feedback in the statusbar.

**Architecture:** Introduce a `tokio::sync::mpsc` action channel in `main.rs` so background `spawn_blocking` tasks can send `DeviceOpUpdate` actions back to AppState. New Phase 4 types (`DeviceOpKind`, `OpStatus`, `DeviceOp`) live in `actions.rs` to avoid circular dependencies with `app.rs`. Device-specific key handling is added tab-contextually in `main.rs`, and `r` is remapped in the Devices tab from `Refresh` to `SwapReset`.

**Tech Stack:** Rust, Ratatui 0.30 (`Table`, `TableState`, `Clear`), tokio 1.51 (`spawn_blocking`, `mpsc`), nix 0.31 (`mount::swapon`, `mount::swapoff`), crossterm 0.29

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/actions.rs` | Modify | Add `DeviceOpKind`, `OpStatus`, `DeviceOp` types + new `Action` variants |
| `src/app.rs` | Modify | Add `selected_dev`, `device_op`, `confirm_action` fields + handle new actions |
| `src/platform/linux.rs` | Modify | Implement `swap_on`, `swap_off`, `swap_reset` via nix |
| `src/ui/devices.rs` | Create | Devices tab: table, status column, footer, confirmation modal |
| `src/ui/mod.rs` | Modify | Route `Tab::Devices` to `devices::render` |
| `src/main.rs` | Modify | Add mpsc channel, device input handling, `spawn_blocking` dispatch |
| `docs/devices.md` | Create | End-user documentation for the Devices tab |

---

## Task 1: New types and action variants (`actions.rs`)

**Files:**
- Modify: `src/actions.rs`

- [ ] **Step 1.1: Write failing tests first**

Add at the bottom of `src/actions.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn device_op_kind_is_clone_and_partialeq() {
        let a = DeviceOpKind::SwapOn;
        assert_eq!(a.clone(), DeviceOpKind::SwapOn);
        assert_ne!(a, DeviceOpKind::SwapOff);
    }

    #[test]
    fn op_status_error_holds_message() {
        let s = OpStatus::Error("swapon failed: EPERM".to_string());
        assert!(matches!(s, OpStatus::Error(ref m) if m == "swapon failed: EPERM"));
    }

    #[test]
    fn device_op_fields_are_accessible() {
        let op = DeviceOp {
            path:   PathBuf::from("/dev/sda2"),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Running,
        };
        assert_eq!(op.path, PathBuf::from("/dev/sda2"));
        assert_eq!(op.kind, DeviceOpKind::SwapOff);
        assert_eq!(op.status, OpStatus::Running);
    }
}
```

Run: `cargo test actions::tests`
Expected: FAIL — types not defined yet

- [ ] **Step 1.2: Replace `src/actions.rs` with full implementation**

```rust
use std::path::PathBuf;

use crate::platform::MemSnapshot;

// ── Phase 4 types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceOpKind {
    SwapOn,
    SwapOff,
    SwapReset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpStatus {
    Running,
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DeviceOp {
    pub path:   PathBuf,
    pub kind:   DeviceOpKind,
    pub status: OpStatus,
}

// ── Actions ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Action {
    // Global
    Quit,
    Refresh,
    NextTab,
    PrevTab,
    SelectTab(usize),
    UpdateSnapshot(MemSnapshot),
    SetError(String),

    // Phase 4 — device navigation
    DeviceUp,
    DeviceDown,

    // Phase 4 — operation flow
    RequestConfirm(DeviceOpKind),
    CancelConfirm,
    ExecuteDeviceOp { path: PathBuf, kind: DeviceOpKind },
    DeviceOpUpdate(DeviceOp),
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn device_op_kind_is_clone_and_partialeq() {
        let a = DeviceOpKind::SwapOn;
        assert_eq!(a.clone(), DeviceOpKind::SwapOn);
        assert_ne!(a, DeviceOpKind::SwapOff);
    }

    #[test]
    fn op_status_error_holds_message() {
        let s = OpStatus::Error("swapon failed: EPERM".to_string());
        assert!(matches!(s, OpStatus::Error(ref m) if m == "swapon failed: EPERM"));
    }

    #[test]
    fn device_op_fields_are_accessible() {
        let op = DeviceOp {
            path:   PathBuf::from("/dev/sda2"),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Running,
        };
        assert_eq!(op.path, PathBuf::from("/dev/sda2"));
        assert_eq!(op.kind, DeviceOpKind::SwapOff);
        assert_eq!(op.status, OpStatus::Running);
    }
}
```

- [ ] **Step 1.3: Run tests**

`cargo test actions::tests`
Expected: 3 tests pass

> Note: `cargo build` will now fail with a non-exhaustive match in `app.rs`. That is expected — Task 2 fixes it.

- [ ] **Step 1.4: Commit**

```bash
git add src/actions.rs
git commit -m "feat(phase4): add DeviceOp types and action variants"
```

---

## Task 2: AppState new fields and action handlers (`app.rs`)

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 2.1: Write failing tests**

Add these tests to the `tests` module at the bottom of `src/app.rs`:

```rust
    // ── Phase 4 — device navigation ───────────────────────────────────────────

    #[test]
    fn device_up_decrements_selected_dev_with_floor_at_zero() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![
            make_device("/dev/sda2"),
            make_device("/swapfile"),
        ];
        state.selected_dev = 1;
        state.handle_action(Action::DeviceUp);
        assert_eq!(state.selected_dev, 0);
        state.handle_action(Action::DeviceUp); // already at 0
        assert_eq!(state.selected_dev, 0);
    }

    #[test]
    fn device_down_increments_selected_dev_capped_at_last() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![
            make_device("/dev/sda2"),
            make_device("/swapfile"),
        ];
        state.handle_action(Action::DeviceDown);
        assert_eq!(state.selected_dev, 1);
        state.handle_action(Action::DeviceDown); // already at last
        assert_eq!(state.selected_dev, 1);
    }

    #[test]
    fn request_confirm_sets_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::RequestConfirm(DeviceOpKind::SwapOff));
        assert_eq!(state.confirm_action, Some(DeviceOpKind::SwapOff));
    }

    #[test]
    fn cancel_confirm_clears_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::SwapOn);
        state.handle_action(Action::CancelConfirm);
        assert!(state.confirm_action.is_none());
    }

    #[test]
    fn execute_device_op_sets_running_and_clears_confirm() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::SwapOff);
        state.handle_action(Action::ExecuteDeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::SwapOff,
        });
        assert!(state.confirm_action.is_none());
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Running);
        assert_eq!(op.path, std::path::PathBuf::from("/dev/sda2"));
    }

    #[test]
    fn device_op_update_replaces_device_op() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path:   "/dev/sda2".into(),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Done,
        }));
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Done);
    }

    #[test]
    fn device_op_update_with_error_sets_error_msg() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path:   "/dev/sda2".into(),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Error("swapoff failed: EPERM".to_string()),
        }));
        assert_eq!(state.error_msg, Some("swapoff failed: EPERM".to_string()));
        assert!(state.device_op.is_some());
    }

    #[test]
    fn set_error_stores_message() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::SetError("Requires root".to_string()));
        assert_eq!(state.error_msg, Some("Requires root".to_string()));
    }
```

Also add this helper to the tests module:

```rust
    fn make_device(path: &str) -> crate::platform::SwapDevice {
        crate::platform::SwapDevice {
            path:     path.into(),
            total:    4 * 1024 * 1024 * 1024,
            used:     1 * 1024 * 1024 * 1024,
            priority: -1,
            kind:     SwapKind::Partition,
            active:   true,
        }
    }
```

Run: `cargo test app::tests`
Expected: FAIL — new fields and imports missing

- [ ] **Step 2.2: Update `src/app.rs`**

Replace the entire file:

```rust
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use crate::platform::{Capabilities, MemSnapshot, SwapDevice};

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
    CreateSwap,
}

pub struct AppState {
    pub active_tab:     Tab,
    pub ram_history:    VecDeque<(Instant, u64)>,
    pub swap_history:   VecDeque<(Instant, u64)>,
    pub max_history:    usize,
    pub current:        Option<MemSnapshot>,
    pub devices:        Vec<SwapDevice>,
    pub capabilities:   Capabilities,
    pub error_msg:      Option<String>,
    pub start_time:     Instant,
    pub should_quit:    bool,

    // Phase 4
    pub selected_dev:   usize,
    pub device_op:      Option<DeviceOp>,
    pub confirm_action: Option<DeviceOpKind>,
}

impl AppState {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            active_tab:     Tab::Overview,
            ram_history:    VecDeque::new(),
            swap_history:   VecDeque::new(),
            max_history:    3600,
            current:        None,
            devices:        Vec::new(),
            capabilities,
            error_msg:      None,
            start_time:     Instant::now(),
            should_quit:    false,
            selected_dev:   0,
            device_op:      None,
            confirm_action: None,
        }
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,

            Action::NextTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview   => Tab::Processes,
                    Tab::Processes  => Tab::Devices,
                    Tab::Devices    => Tab::CreateSwap,
                    Tab::CreateSwap => Tab::Overview,
                };
            }

            Action::PrevTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview   => Tab::CreateSwap,
                    Tab::Processes  => Tab::Overview,
                    Tab::Devices    => Tab::Processes,
                    Tab::CreateSwap => Tab::Devices,
                };
            }

            Action::SelectTab(n) => {
                self.active_tab = match n {
                    1 => Tab::Overview,
                    2 => Tab::Processes,
                    3 => Tab::Devices,
                    4 => Tab::CreateSwap,
                    _ => return,
                };
            }

            Action::UpdateSnapshot(snapshot) => {
                self.ram_history.push_back((snapshot.timestamp, snapshot.ram.used));
                self.swap_history.push_back((snapshot.timestamp, snapshot.swap.used));
                while self.ram_history.len() > self.max_history {
                    self.ram_history.pop_front();
                }
                while self.swap_history.len() > self.max_history {
                    self.swap_history.pop_front();
                }
                self.devices   = snapshot.devices.clone();
                self.current   = Some(snapshot);
                self.error_msg = None;
                // Clamp selection if device list shrank
                if !self.devices.is_empty() {
                    self.selected_dev = self.selected_dev.min(self.devices.len() - 1);
                }
            }

            Action::Refresh => {}

            Action::SetError(msg) => {
                self.error_msg = Some(msg);
            }

            // Phase 4 — device navigation
            Action::DeviceUp => {
                self.selected_dev = self.selected_dev.saturating_sub(1);
            }

            Action::DeviceDown => {
                if !self.devices.is_empty() {
                    self.selected_dev = (self.selected_dev + 1).min(self.devices.len() - 1);
                }
            }

            // Phase 4 — operation flow
            Action::RequestConfirm(kind) => {
                self.confirm_action = Some(kind);
            }

            Action::CancelConfirm => {
                self.confirm_action = None;
            }

            Action::ExecuteDeviceOp { path, kind } => {
                self.confirm_action = None;
                self.device_op = Some(DeviceOp {
                    path,
                    kind,
                    status: OpStatus::Running,
                });
            }

            Action::DeviceOpUpdate(op) => {
                if let OpStatus::Error(ref msg) = op.status {
                    self.error_msg = Some(msg.clone());
                }
                self.device_op = Some(op);
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{SwapInfo, SwapKind};

    fn make_caps() -> Capabilities {
        Capabilities {
            can_swap_on:     true,
            can_swap_off:    true,
            has_per_process: true,
            has_device_list: true,
            can_create_swap: true,
            requires_root:   true,
        }
    }

    fn make_snapshot() -> MemSnapshot {
        MemSnapshot {
            timestamp: Instant::now(),
            ram:       SwapInfo::new(16 * 1024 * 1024 * 1024, 8 * 1024 * 1024 * 1024),
            swap:      SwapInfo::new(4  * 1024 * 1024 * 1024, 1 * 1024 * 1024 * 1024),
            devices:   vec![],
            processes: vec![],
        }
    }

    fn make_device(path: &str) -> SwapDevice {
        SwapDevice {
            path:     path.into(),
            total:    4 * 1024 * 1024 * 1024,
            used:     1 * 1024 * 1024 * 1024,
            priority: -1,
            kind:     SwapKind::Partition,
            active:   true,
        }
    }

    // ── Quit ──────────────────────────────────────────────────────────────────

    #[test]
    fn quit_action_sets_should_quit() {
        let mut state = AppState::new(make_caps());
        assert!(!state.should_quit);
        state.handle_action(Action::Quit);
        assert!(state.should_quit);
    }

    // ── Tab navigation ────────────────────────────────────────────────────────

    #[test]
    fn next_tab_cycles_forward_through_all_tabs() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Processes);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Devices);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::CreateSwap);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Overview);
    }

    #[test]
    fn prev_tab_wraps_backward_from_overview() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::PrevTab);
        assert_eq!(state.active_tab, Tab::CreateSwap);
    }

    #[test]
    fn select_tab_jumps_directly() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::SelectTab(3));
        assert_eq!(state.active_tab, Tab::Devices);
        state.handle_action(Action::SelectTab(1));
        assert_eq!(state.active_tab, Tab::Overview);
    }

    #[test]
    fn select_tab_out_of_range_is_ignored() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::SelectTab(99));
        assert_eq!(state.active_tab, Tab::Overview);
    }

    // ── Snapshot / history ────────────────────────────────────────────────────

    #[test]
    fn update_snapshot_appends_to_history() {
        let mut state = AppState::new(make_caps());
        assert!(state.ram_history.is_empty());
        state.handle_action(Action::UpdateSnapshot(make_snapshot()));
        assert_eq!(state.ram_history.len(), 1);
        assert_eq!(state.swap_history.len(), 1);
        assert!(state.current.is_some());
    }

    #[test]
    fn history_is_capped_at_max_history() {
        let mut state = AppState::new(make_caps());
        state.max_history = 3;
        for _ in 0..6 {
            state.handle_action(Action::UpdateSnapshot(make_snapshot()));
        }
        assert_eq!(state.ram_history.len(), 3);
        assert_eq!(state.swap_history.len(), 3);
    }

    #[test]
    fn update_snapshot_clears_error_message() {
        let mut state = AppState::new(make_caps());
        state.error_msg = Some("previous error".to_string());
        state.handle_action(Action::UpdateSnapshot(make_snapshot()));
        assert!(state.error_msg.is_none());
    }

    #[test]
    fn update_snapshot_stores_current_devices() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.devices = vec![SwapDevice {
            path:     "/dev/sda2".into(),
            total:    4 * 1024 * 1024 * 1024,
            used:     1 * 1024 * 1024 * 1024,
            priority: -1,
            kind:     SwapKind::Partition,
            active:   true,
        }];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.devices.len(), 1);
    }

    #[test]
    fn history_values_match_snapshot_used_bytes() {
        let mut state = AppState::new(make_caps());
        let snap = make_snapshot();
        let expected_ram  = snap.ram.used;
        let expected_swap = snap.swap.used;
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.ram_history.back().map(|(_, v)| *v),  Some(expected_ram));
        assert_eq!(state.swap_history.back().map(|(_, v)| *v), Some(expected_swap));
    }

    // ── Phase 4 — device navigation ───────────────────────────────────────────

    #[test]
    fn device_up_decrements_selected_dev_with_floor_at_zero() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.selected_dev = 1;
        state.handle_action(Action::DeviceUp);
        assert_eq!(state.selected_dev, 0);
        state.handle_action(Action::DeviceUp);
        assert_eq!(state.selected_dev, 0);
    }

    #[test]
    fn device_down_increments_selected_dev_capped_at_last() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.handle_action(Action::DeviceDown);
        assert_eq!(state.selected_dev, 1);
        state.handle_action(Action::DeviceDown);
        assert_eq!(state.selected_dev, 1);
    }

    #[test]
    fn request_confirm_sets_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::RequestConfirm(DeviceOpKind::SwapOff));
        assert_eq!(state.confirm_action, Some(DeviceOpKind::SwapOff));
    }

    #[test]
    fn cancel_confirm_clears_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::SwapOn);
        state.handle_action(Action::CancelConfirm);
        assert!(state.confirm_action.is_none());
    }

    #[test]
    fn execute_device_op_sets_running_and_clears_confirm() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::SwapOff);
        state.handle_action(Action::ExecuteDeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::SwapOff,
        });
        assert!(state.confirm_action.is_none());
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Running);
        assert_eq!(op.path, PathBuf::from("/dev/sda2"));
    }

    #[test]
    fn device_op_update_replaces_device_op() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path:   "/dev/sda2".into(),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Done,
        }));
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Done);
    }

    #[test]
    fn device_op_update_with_error_sets_error_msg() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path:   "/dev/sda2".into(),
            kind:   DeviceOpKind::SwapOff,
            status: OpStatus::Error("swapoff failed: EPERM".to_string()),
        }));
        assert_eq!(state.error_msg, Some("swapoff failed: EPERM".to_string()));
        assert!(state.device_op.is_some());
    }

    #[test]
    fn set_error_stores_message() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::SetError("Requires root".to_string()));
        assert_eq!(state.error_msg, Some("Requires root".to_string()));
    }

    #[test]
    fn update_snapshot_clamps_selected_dev_when_list_shrinks() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.selected_dev = 1;
        // Snapshot with only 1 device
        let mut snap = make_snapshot();
        snap.devices = vec![make_device("/dev/sda2")];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.selected_dev, 0);
    }
}
```

- [ ] **Step 2.3: Run all tests**

`cargo test`
Expected: all tests pass (including existing ones)

- [ ] **Step 2.4: Commit**

```bash
git add src/app.rs
git commit -m "feat(phase4): add device state fields and action handlers to AppState"
```

---

## Task 3: Implement `swap_on`, `swap_off`, `swap_reset` in `linux.rs`

**Files:**
- Modify: `src/platform/linux.rs`

> Note: These functions require root and real hardware — they cannot be unit tested in isolation. The existing unit tests (parsing `/proc/swaps`) are unaffected. Correctness is verified by running the app as root.

- [ ] **Step 3.1: Replace stubs with nix implementation**

In `src/platform/linux.rs`, replace the three stub methods:

```rust
fn swap_on(&self, device: &Path) -> Result<()> {
    nix::mount::swapon(device, nix::mount::SwaponFlags::empty())
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

- [ ] **Step 3.2: Build and run existing tests**

```bash
cargo build
cargo test platform::linux::tests
```

Expected: builds clean, all existing parse tests pass

- [ ] **Step 3.3: Commit**

```bash
git add src/platform/linux.rs
git commit -m "feat(phase4): implement swapon/swapoff/reset via nix in LinuxBackend"
```

---

## Task 4: Create `src/ui/devices.rs` — device table and footer

**Files:**
- Create: `src/ui/devices.rs`

- [ ] **Step 4.1: Write layout tests first**

Create `src/ui/devices.rs` with only the layout function and its tests:

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};

use crate::app::AppState;
use crate::actions::{DeviceOpKind, OpStatus};
use crate::ui::design;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area);
    render_header(f, layout[0]);
    render_table(f, layout[1], state);
    render_footer(f, layout[2], state);

    if state.confirm_action.is_some() {
        render_modal(f, area, state);
    }
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // [0] header row
            Constraint::Min(0),    // [1] device list
            Constraint::Length(2), // [2] footer hints
        ])
        .spacing(design::INNER_GAP)
        .split(area)
}

fn render_header(f: &mut Frame, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Path").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("Type").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("Total").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("Used").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("%").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("Pri").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        Cell::from("Status").style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
    ]);
    let t = Table::new(vec![header], column_widths())
        .block(Block::default());
    f.render_widget(t, area);
}

fn render_table(f: &mut Frame, area: Rect, state: &AppState) {
    let rows: Vec<Row> = state.devices.iter().enumerate().map(|(i, dev)| {
        let status_cell = status_cell(dev, state);
        let percent = if dev.total > 0 {
            dev.used as f64 / dev.total as f64 * 100.0
        } else {
            0.0
        };

        let row = Row::new(vec![
            Cell::from(dev.path.to_string_lossy().to_string()),
            Cell::from(dev.kind.to_string()),
            Cell::from(human_bytes::human_bytes(dev.total as f64)),
            Cell::from(human_bytes::human_bytes(dev.used as f64)),
            Cell::from(format!("{percent:.0}%")),
            Cell::from(format!("{}", dev.priority)),
            status_cell,
        ]);

        if i == state.selected_dev {
            row.style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        } else {
            row
        }
    }).collect();

    let table = Table::new(rows, column_widths())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Swap Devices ",
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                )),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut table_state = TableState::default().with_selected(Some(state.selected_dev));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn status_cell<'a>(dev: &crate::platform::SwapDevice, state: &AppState) -> Cell<'a> {
    if let Some(op) = &state.device_op {
        if op.path == dev.path {
            return match &op.status {
                OpStatus::Running       => Cell::from("⏳ ...").style(Style::default().fg(Color::Yellow)),
                OpStatus::Done          => Cell::from("✓ OK").style(Style::default().fg(Color::Green)),
                OpStatus::Error(_)      => Cell::from("✗ ERROR").style(Style::default().fg(Color::Red)),
            };
        }
    }
    if dev.active {
        Cell::from("ACTIVE").style(Style::default().fg(Color::Green))
    } else {
        Cell::from("INACTIVE").style(Style::default().fg(Color::DarkGray))
    }
}

fn column_widths() -> Vec<Constraint> {
    vec![
        Constraint::Min(20),    // Path
        Constraint::Length(10), // Type
        Constraint::Length(9),  // Total
        Constraint::Length(9),  // Used
        Constraint::Length(5),  // %
        Constraint::Length(5),  // Pri
        Constraint::Length(10), // Status
    ]
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hint_line = Line::from(vec![
        key_span("o"), desc_span(" activate  "),
        key_span("f"), desc_span(" deactivate  "),
        key_span("r"), desc_span(" reset  "),
        key_span("j/k"), desc_span(" navigate"),
    ]);

    let warning_line = if !state.capabilities.can_swap_on {
        Line::from(Span::styled(
            "  Managed by dynamic_pager — control unavailable",
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from("")
    };

    f.render_widget(
        Paragraph::new(vec![hint_line, warning_line]),
        area,
    );
}

fn key_span(k: &str) -> Span<'static> {
    Span::styled(
        format!(" {k} "),
        Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
    )
}

fn desc_span(d: &str) -> Span<'static> {
    Span::styled(d.to_string(), Style::default().fg(Color::DarkGray))
}

fn render_modal(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(kind) = &state.confirm_action else { return };

    let modal_width  = (area.width * 60 / 100).max(40);
    let modal_height = 7u16;
    let modal_x = area.x + (area.width.saturating_sub(modal_width))  / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_rect = Rect::new(modal_x, modal_y, modal_width, modal_height);

    let op_label = match kind {
        DeviceOpKind::SwapOn    => "Activate",
        DeviceOpKind::SwapOff   => "Deactivate",
        DeviceOpKind::SwapReset => "Reset",
    };

    let dev_path = state
        .devices
        .get(state.selected_dev)
        .map(|d| d.path.to_string_lossy().to_string())
        .unwrap_or_default();

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {op_label} {dev_path}?"),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            key_span("s"),
            desc_span(" confirm    "),
            key_span("Esc"),
            desc_span(" cancel"),
        ]),
        Line::from(""),
    ];

    f.render_widget(Clear, modal_rect);
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(Span::styled(" Confirm ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        modal_rect,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use crate::ui::design::INNER_GAP;
    use super::build_layout;

    #[test]
    fn header_row_starts_at_top_and_is_one_line() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 1);
    }

    #[test]
    fn footer_is_two_lines_at_bottom() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[2].height, 2);
        assert_eq!(layout[2].y,      area.height - 2);
    }

    #[test]
    fn all_sections_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        for rect in layout.iter() {
            assert_eq!(rect.x,     0);
            assert_eq!(rect.width, 120);
        }
    }

    #[test]
    fn layout_stable_on_minimal_terminal() {
        // minimum: 1 (header) + INNER_GAP(1) + 0 (list Min) + INNER_GAP(1) + 2 (footer) = 5
        let area = Rect::new(0, 0, 40, 5);
        let layout = build_layout(area);
        assert_eq!(layout[1].height, 0);
    }
}
```

- [ ] **Step 4.2: Wire up in `ui/mod.rs`** (needed to compile)

In `src/ui/mod.rs`, add `mod devices;` at the top (alongside `mod overview;`):

```rust
mod design;
mod devices;
mod overview;
mod statusbar;
```

And update the match in `render()`:

```rust
match state.active_tab {
    Tab::Overview => overview::render(f, layout[1], state),
    Tab::Devices  => devices::render(f, layout[1], state),
    _             => render_coming_soon(f, layout[1]),
}
```

- [ ] **Step 4.3: Build and run all tests**

```bash
cargo build
cargo test
```

Expected: all tests pass, including the 4 new layout tests in `ui::devices::tests`

- [ ] **Step 4.4: Commit**

```bash
git add src/ui/devices.rs src/ui/mod.rs
git commit -m "feat(phase4): add devices tab UI with table, footer, and confirmation modal"
```

---

## Task 5: Action channel + device input handling (`main.rs`)

**Files:**
- Modify: `src/main.rs`

This task:
1. Introduces an `mpsc` channel so background tasks can send actions back
2. Adds a new `select!` arm to receive those actions
3. Adds tab-contextual input handling for `Tab::Devices`
4. Remaps `r` in Devices tab to `SwapReset` instead of `Refresh`
5. Spawns `spawn_blocking` on `ExecuteDeviceOp`

- [ ] **Step 5.1: Replace `src/main.rs` with full implementation**

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyModifiers};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod platform;
mod tui;
mod ui;

use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use app::{AppState, Tab};
use collector::Collector;
use platform::linux::LinuxBackend;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let mut col = Collector::new(backend);

    match col.collect().await {
        Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
        Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
    }

    let mut terminal = tui::init()?;

    let shutdown = CancellationToken::new();
    {
        let token = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                token.cancel();
            }
        });
    }

    let result = run(&mut terminal, state, &mut col, shutdown).await;
    tui::restore()?;
    result
}

async fn run(
    terminal: &mut tui::Tui,
    state: Arc<Mutex<AppState>>,
    col: &mut Collector,
    shutdown: CancellationToken,
) -> Result<()> {
    let mut tick       = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events     = EventStream::new();

    // Channel for background tasks (spawn_blocking) to send actions back.
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            // Background task result (e.g. DeviceOpUpdate from swapon/swapoff)
            Some(action) = action_rx.recv() => {
                state.lock().expect("state mutex poisoned").handle_action(action);
            }

            _ = tick.tick() => {
                match col.collect().await {
                    Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
                    Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
                }
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    // Read tab-relevant state before dropping the lock
                    let (active_tab, confirm_action, selected_dev, has_devices) = {
                        let s = state.lock().expect("state mutex poisoned");
                        (
                            s.active_tab.clone(),
                            s.confirm_action.clone(),
                            s.selected_dev,
                            !s.devices.is_empty(),
                        )
                    };

                    let action: Option<Action> = match key.code {
                        // Global keys (always active, except 'r' which is overridden in Devices tab)
                        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
                        KeyCode::Tab    => Some(Action::NextTab),
                        KeyCode::BackTab => Some(Action::PrevTab),
                        KeyCode::Char('1') => Some(Action::SelectTab(1)),
                        KeyCode::Char('2') => Some(Action::SelectTab(2)),
                        KeyCode::Char('3') => Some(Action::SelectTab(3)),
                        KeyCode::Char('4') => Some(Action::SelectTab(4)),

                        // Tab-specific keys
                        _ => match active_tab {
                            Tab::Devices => handle_devices_key(
                                key.code,
                                confirm_action.as_ref(),
                                selected_dev,
                                has_devices,
                                &state,
                            ),
                            _ => match key.code {
                                KeyCode::Char('r') => Some(Action::Refresh),
                                _ => None,
                            },
                        },
                    };

                    // Spawn background task before dispatching ExecuteDeviceOp to AppState
                    if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        let tx   = action_tx.clone();
                        let path = path.clone();
                        let kind = kind.clone();
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
                    }

                    if let Some(a) = action {
                        state.lock().expect("state mutex poisoned").handle_action(a);
                    }
                }
            }
        }
    }

    Ok(())
}

fn handle_devices_key(
    code: KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev: usize,
    has_devices: bool,
    state: &Arc<Mutex<AppState>>,
) -> Option<Action> {
    if let Some(kind) = confirm_action {
        // Modal is open — only 's' and Esc are active
        return match code {
            KeyCode::Char('s') | KeyCode::Enter => {
                let path = state
                    .lock()
                    .expect("state mutex poisoned")
                    .devices
                    .get(selected_dev)?
                    .path
                    .clone();
                Some(Action::ExecuteDeviceOp { path, kind: kind.clone() })
            }
            KeyCode::Esc => Some(Action::CancelConfirm),
            _ => None,
        };
    }

    match code {
        KeyCode::Char('j') | KeyCode::Down  => Some(Action::DeviceDown),
        KeyCode::Char('k') | KeyCode::Up    => Some(Action::DeviceUp),
        KeyCode::Char('r') if has_devices   => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::SwapReset))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('o') if has_devices   => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::SwapOn))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('f') if has_devices   => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::SwapOff))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        _ => None,
    }
}
```

- [ ] **Step 5.2: Build clean**

```bash
cargo build
```

Expected: compiles with zero warnings

- [ ] **Step 5.3: Run all tests**

```bash
cargo test
```

Expected: all tests pass

- [ ] **Step 5.4: Lint**

```bash
cargo clippy -- -D warnings
```

Expected: no warnings

- [ ] **Step 5.5: Commit**

```bash
git add src/main.rs
git commit -m "feat(phase4): add action channel and device input handling to event loop"
```

---

## Task 6: User documentation (`docs/devices.md`)

**Files:**
- Create: `docs/devices.md`

- [ ] **Step 6.1: Create the file**

```markdown
# Devices Tab

The Devices tab (press `3` or navigate with `Tab`) shows all active swap devices and lets you activate, deactivate, or reset them.

## Requirements

Control operations (`o`, `f`, `r`) require root. Run as:

```
sudo swaptop
```

If you run without root, you can still view device status — only control is restricted.

## Columns

| Column | Description |
|--------|-------------|
| Path | Device path (e.g. `/dev/sda2`, `/swapfile`) |
| Type | `Partition`, `File`, `Zram`, or `DynamicPager` |
| Total | Total swap capacity |
| Used | Currently used swap |
| % | Usage percentage |
| Pri | Kernel priority (higher = preferred) |
| Status | `ACTIVE`, `INACTIVE`, `⏳ ...`, `✓ OK`, or `✗ ERROR` |

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `o` | Activate selected device (`swapon`) |
| `f` | Deactivate selected device (`swapoff`) |
| `r` | Reset selected device (`swapoff` + 100ms + `swapon`) |
| `s` | Confirm action (when modal is open) |
| `Esc` | Cancel confirmation modal |
| `Tab` / `1-4` | Switch tabs |
| `q` | Quit |

## Status Indicators

- **`ACTIVE`** — device is currently active as swap
- **`INACTIVE`** — device is known but not currently active
- **`⏳ ...`** — operation in progress (swapon/swapoff running)
- **`✓ OK`** — last operation succeeded
- **`✗ ERROR`** — last operation failed (check statusbar for details)

## Reset Operation

Reset (`r`) performs `swapoff` followed by `swapon` with a 100ms pause. This forces the kernel to move all swap pages back to RAM and then re-enable the device, which clears fragmentation. Use it when swap usage is high but actual data could be consolidated.

**Note:** Reset requires enough free RAM to hold all data currently in that swap device. If RAM is too full, `swapoff` will fail with an error.

## Platform Notes

On **macOS**, swap is managed automatically by `dynamic_pager`. The Devices tab shows the active swapfiles but control operations are unavailable.
```

- [ ] **Step 6.2: Commit**

```bash
git add docs/devices.md
git commit -m "docs: add Devices tab user documentation"
```

---

## Task 7: Final verification

- [ ] **Step 7.1: Full build + lint + test**

```bash
cargo build
cargo clippy -- -D warnings
cargo test
```

Expected: zero warnings, zero failures

- [ ] **Step 7.2: Manual smoke test (optional — requires Linux + swap configured)**

```bash
cargo run
# Press 3 to navigate to Devices tab
# Verify: device list renders with correct columns
# Verify: j/k navigation highlights rows
# Run as root and press f to verify modal appears
# Press Esc to cancel
```

- [ ] **Step 7.3: Final commit if any fixes were needed**

If steps above required any fixes, commit them:

```bash
git add -p
git commit -m "fix(phase4): address issues found during final verification"
```
