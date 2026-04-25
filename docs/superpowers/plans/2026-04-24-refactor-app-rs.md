# Refatorar `src/app.rs` em Módulos por Domínio

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Quebrar `src/app.rs` (~1500 linhas) em 5 módulos Rust espelhando `src/ui/`, mantendo comportamento idêntico e testes distribuídos.

**Architecture:** `src/app/mod.rs` mantém `AppState` struct e `handle_action` dispatcher. Cada domínio (snapshot, devices, processes, create_swap) vive em arquivo próprio com handlers + tests. `UpdateSnapshot` fica em `snapshot.rs` como orquestrador com callbacks `on_processes_updated()` / `on_devices_updated()`.

**Tech Stack:** Rust 2024 edition, `tokio`, `ratatui`, `crossterm`, `nix`

---

## File Structure

```
src/app/
  mod.rs         # AppState struct + new() + handle_action dispatcher + test helpers
  snapshot.rs    # UpdateSnapshot orquestrador + history helpers + tests
  devices.rs     # DeviceUp, DeviceDown, ExecuteDeviceOp, DeviceOpUpdate, ConfirmOffDelete + tests
  processes.rs   # NavigateUp, NavigateDown, SortBy, Filter* + tests
  create_swap.rs # Open*, Close*, Focus*, Submit, Progress, Completion* + tests
```

---

## Task 1: Criar Diretório e `mod.rs` Base

**Files:**
- Delete: `src/app.rs`
- Create: `src/app/mod.rs`
- Modify: `src/lib.rs` (se necessário)

- [ ] **Step 1: Criar diretório `src/app/` e backup temporário**

```bash
mkdir -p src/app
cp src/app.rs /tmp/app.rs.backup
```

- [ ] **Step 2: Escrever `src/app/mod.rs` — struct, new(), dispatcher, helpers cross-cutting**

```rust
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus, SortColumn, SortDir};
use crate::create_swap::{CreateSwapModal, CreateSwapMode, CreateSwapStep};
use crate::platform::{Capabilities, MemSnapshot, ProcessRow, SwapDevice};

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
}

#[derive(Debug, Clone)]
pub struct ConfirmOffDelete {
    pub path: PathBuf,
    pub delete_file: bool,
    pub active: bool,
}

pub struct AppState {
    pub active_tab: Tab,
    pub ram_history: VecDeque<(Instant, u64)>,
    pub swap_history: VecDeque<(Instant, u64)>,
    pub max_history: usize,
    pub current: Option<MemSnapshot>,
    pub devices: Vec<SwapDevice>,
    pub capabilities: Capabilities,
    pub error_msg: Option<(String, Instant)>,
    pub start_time: Instant,
    pub should_quit: bool,
    pub selected_dev: usize,
    pub device_op: Option<DeviceOp>,
    pub confirm_action: Option<DeviceOpKind>,
    pub processes: Vec<ProcessRow>,
    pub sort_col: SortColumn,
    pub sort_dir: SortDir,
    pub selected_row: usize,
    pub filter_text: String,
    pub filter_mode: bool,
    pub create_swap_modal: Option<CreateSwapModal>,
    pub confirm_off_delete: Option<ConfirmOffDelete>,
    pub is_root: bool,
    pub collect_in_progress: bool,
    pub last_collect_completed: Instant,
    pub device_op_started: Option<Instant>,
}

impl AppState {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            active_tab: Tab::Overview,
            ram_history: VecDeque::new(),
            swap_history: VecDeque::new(),
            max_history: 3600,
            current: None,
            devices: Vec::new(),
            capabilities,
            error_msg: None,
            start_time: Instant::now(),
            should_quit: false,
            selected_dev: 0,
            device_op: None,
            confirm_action: None,
            processes: Vec::new(),
            sort_col: SortColumn::Swap,
            sort_dir: SortDir::Desc,
            selected_row: 0,
            filter_text: String::new(),
            filter_mode: false,
            create_swap_modal: None,
            confirm_off_delete: None,
            is_root: nix::unistd::geteuid().is_root(),
            collect_in_progress: false,
            last_collect_completed: Instant::now(),
            device_op_started: None,
        }
    }

    pub fn filtered_len(&self) -> usize {
        if self.filter_text.is_empty() {
            self.processes.len()
        } else {
            let lower = self.filter_text.to_lowercase();
            self.processes
                .iter()
                .filter(|p| p.name.to_lowercase().contains(&lower))
                .count()
        }
    }

    pub(crate) fn sort_processes(&mut self) {
        let col = self.sort_col;
        let dir = self.sort_dir;
        self.processes.sort_by(|a, b| {
            let ord = match col {
                SortColumn::Pid => a.pid.cmp(&b.pid),
                SortColumn::Name => a.name.cmp(&b.name),
                SortColumn::User => a.user.cmp(&b.user),
                SortColumn::Rss => a.rss.cmp(&b.rss),
                SortColumn::Swap => a.swap.cmp(&b.swap),
                SortColumn::Cpu => a
                    .cpu_pct
                    .partial_cmp(&b.cpu_pct)
                    .unwrap_or(std::cmp::Ordering::Equal),
            };
            if dir == SortDir::Desc {
                ord.reverse()
            } else {
                ord
            }
        });
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,

            Action::NextTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview => Tab::Processes,
                    Tab::Processes => Tab::Devices,
                    Tab::Devices => Tab::Overview,
                };
            }

            Action::PrevTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview => Tab::Devices,
                    Tab::Processes => Tab::Overview,
                    Tab::Devices => Tab::Processes,
                };
            }

            Action::SelectTab(n) => {
                self.active_tab = match n {
                    1 => Tab::Overview,
                    2 => Tab::Processes,
                    3 => Tab::Devices,
                    _ => return,
                };
            }

            Action::UpdateSnapshot(snapshot) => self.apply_snapshot(snapshot),
            Action::SetError(msg) => {
                self.error_msg = Some((msg, Instant::now()));
            }
            Action::CollectStarted => {
                self.collect_in_progress = true;
            }
            Action::CollectFinished => {
                self.collect_in_progress = false;
            }

            // Delegados para módulos
            Action::DeviceUp => self.handle_device_up(),
            Action::DeviceDown => self.handle_device_down(),
            Action::RequestConfirm(kind) => self.handle_request_confirm(kind),
            Action::CancelConfirm => self.handle_cancel_confirm(),
            Action::ExecuteDeviceOp { path, kind } => self.handle_execute_device_op(path, kind),
            Action::DeviceOpUpdate(op) => self.handle_device_op_update(op),

            Action::NavigateUp => self.handle_navigate_up(),
            Action::NavigateDown => self.handle_navigate_down(),
            Action::SortBy(col) => self.handle_sort_by(col),
            Action::EnterFilterMode => self.handle_enter_filter_mode(),
            Action::FilterChar(c) => self.handle_filter_char(c),
            Action::FilterBackspace => self.handle_filter_backspace(),
            Action::ExitFilterMode => self.handle_exit_filter_mode(),

            Action::OpenCreateSwap => self.handle_open_create_swap(),
            Action::CloseCreateSwap => self.handle_close_create_swap(),
            Action::CreateSwapReturnToForm => self.handle_create_swap_return_to_form(),
            Action::CreateSwapFocusField(field) => self.handle_create_swap_focus_field(field),
            Action::CreateSwapInputEvent(event) => self.handle_create_swap_input_event(event),
            Action::CreateSwapToggleUnit => self.handle_create_swap_toggle_unit(),
            Action::CreateSwapToggleActivate => self.handle_create_swap_toggle_activate(),
            Action::CreateSwapSubmit { activate_only } => {
                self.handle_create_swap_submit(activate_only);
            }
            Action::CreateSwapProgress(progress) => self.handle_create_swap_progress(progress),
            Action::CreateSwapSetCompletions(items) => {
                self.handle_create_swap_set_completions(items);
            }
            Action::CreateSwapCompletionMove(delta) => {
                self.handle_create_swap_completion_move(delta);
            }
            Action::CreateSwapApplyCompletion => self.handle_create_swap_apply_completion(),
            Action::CreateSwapClearCompletions => self.handle_create_swap_clear_completions(),

            Action::RequestConfirmOffDelete => self.handle_request_confirm_off_delete(),
            Action::ToggleConfirmDeleteFile => self.handle_toggle_confirm_delete_file(),
            Action::CancelConfirmOffDelete => self.handle_cancel_confirm_off_delete(),
        }
    }
}

// ── Test helpers (pub(crate) para reuso nos submódulos) ────────────────────────

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::platform::{SwapInfo, SwapKind};
    use std::path::PathBuf;

    pub fn make_caps() -> Capabilities {
        Capabilities {
            can_swap_on: true,
            has_per_process: true,
        }
    }

    pub fn make_snapshot() -> MemSnapshot {
        MemSnapshot {
            timestamp: Instant::now(),
            ram: SwapInfo::new(16 * 1024 * 1024 * 1024, 8 * 1024 * 1024 * 1024),
            swap: SwapInfo::new(4 * 1024 * 1024 * 1024, 1024 * 1024 * 1024),
            devices: vec![],
            processes: vec![],
        }
    }

    pub fn make_device(path: &str) -> SwapDevice {
        SwapDevice {
            path: path.into(),
            total: 4 * 1024 * 1024 * 1024,
            used: 1024 * 1024 * 1024,
            priority: -1,
            kind: SwapKind::Partition,
            active: true,
        }
    }

    pub fn make_process(pid: u32, name: &str, swap: u64) -> ProcessRow {
        ProcessRow {
            pid,
            name: name.to_string(),
            user: "user".to_string(),
            rss: 0,
            swap,
            cpu_pct: 0.0,
        }
    }
}

// ── Tests do dispatcher (triviais) ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn quit_action_sets_should_quit() {
        let mut state = AppState::new(make_caps());
        assert!(!state.should_quit);
        state.handle_action(Action::Quit);
        assert!(state.should_quit);
    }

    #[test]
    fn next_tab_cycles_forward_through_all_tabs() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Processes);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Devices);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Overview);
    }

    #[test]
    fn prev_tab_wraps_backward_from_overview() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::PrevTab);
        assert_eq!(state.active_tab, Tab::Devices);
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

    #[test]
    fn is_root_field_matches_actual_euid() {
        let state = AppState::new(make_caps());
        assert_eq!(state.is_root, nix::unistd::geteuid().is_root());
    }
}
```

Run: `rtk cargo check`
Expected: FAIL — métodos delegados não existem ainda (snapshot, devices, etc.)

---

## Task 2: Extrair `snapshot.rs` — UpdateSnapshot Orquestrador

**Files:**
- Create: `src/app/snapshot.rs`

- [ ] **Step 1: Escrever `src/app/snapshot.rs`**

```rust
use std::time::Instant;

use crate::app::AppState;
use crate::platform::MemSnapshot;

impl AppState {
    pub(crate) fn apply_snapshot(&mut self, snapshot: MemSnapshot) {
        // 1. History
        self.push_history(&snapshot);

        // 2. Domain data
        self.devices = snapshot.devices.clone();
        self.processes = snapshot.processes.clone();

        // 3. Domain-specific reactions (delegado)
        self.on_processes_updated();
        self.on_devices_updated();

        // 4. Snapshot metadata
        self.current = Some(snapshot);
        self.last_collect_completed = Instant::now();

        // 5. Clear stale errors (>5s)
        self.clear_stale_errors();
    }

    fn push_history(&mut self, snapshot: &MemSnapshot) {
        self.ram_history
            .push_back((snapshot.timestamp, snapshot.ram.used));
        self.swap_history
            .push_back((snapshot.timestamp, snapshot.swap.used));
        while self.ram_history.len() > self.max_history {
            self.ram_history.pop_front();
        }
        while self.swap_history.len() > self.max_history {
            self.swap_history.pop_front();
        }
    }

    fn clear_stale_errors(&mut self) {
        if self
            .error_msg
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed().as_secs() >= 5)
        {
            self.error_msg = None;
        }
    }
}

// ── Callbacks invocados pelo orquestrador ─────────────────────────────────────
// Estes são implementados nos módulos de domínio (processes.rs, devices.rs)

// pub(crate) fn on_processes_updated(&mut self) — em processes.rs
// pub(crate) fn on_devices_updated(&mut self) — em devices.rs

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_helpers::*;
    use crate::actions::Action;
    use crate::platform::SwapDevice;
    use crate::platform::SwapKind;

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
        // Erro fresco (<5s) NÃO deve ser limpo
        state.handle_action(Action::SetError("previous error".to_string()));
        state.handle_action(Action::UpdateSnapshot(make_snapshot()));
        assert!(state.error_msg.is_some());
    }

    #[test]
    fn update_snapshot_stores_current_devices() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.devices = vec![SwapDevice {
            path: "/dev/sda2".into(),
            total: 4 * 1024 * 1024 * 1024,
            used: 1024 * 1024 * 1024,
            priority: -1,
            kind: SwapKind::Partition,
            active: true,
        }];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.devices.len(), 1);
    }

    #[test]
    fn history_values_match_snapshot_used_bytes() {
        let mut state = AppState::new(make_caps());
        let snap = make_snapshot();
        let expected_ram = snap.ram.used;
        let expected_swap = snap.swap.used;
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(
            state.ram_history.back().map(|(_, v)| *v),
            Some(expected_ram)
        );
        assert_eq!(
            state.swap_history.back().map(|(_, v)| *v),
            Some(expected_swap)
        );
    }

    #[test]
    fn update_snapshot_sorts_by_swap_desc_by_default() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![
            make_process(1, "a", 100),
            make_process(2, "b", 500),
            make_process(3, "c", 200),
        ];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.processes[0].swap, 500);
        assert_eq!(state.processes[1].swap, 200);
        assert_eq!(state.processes[2].swap, 100);
    }

    #[test]
    fn update_snapshot_clamps_selected_row_when_list_shrinks() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![
            make_process(1, "a", 0),
            make_process(2, "b", 0),
            make_process(3, "c", 0),
        ];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.selected_row = 2;
        // Snapshot com apenas 1 processo
        let mut snap2 = make_snapshot();
        snap2.processes = vec![make_process(1, "a", 0)];
        state.handle_action(Action::UpdateSnapshot(snap2));
        assert_eq!(state.selected_row, 0);
    }

    #[test]
    fn update_snapshot_clamps_selected_dev_when_list_shrinks() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.selected_dev = 1;
        // Snapshot com apenas 1 device
        let mut snap = make_snapshot();
        snap.devices = vec![make_device("/dev/sda2")];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.selected_dev, 0);
    }
}
```

- [ ] **Step 2: Verificar compilação**

Run: `rtk cargo check`
Expected: FAIL — `on_processes_updated` e `on_devices_updated` ainda não existem

---

## Task 3: Extrair `devices.rs` — Device Handlers

**Files:**
- Create: `src/app/devices.rs`

- [ ] **Step 1: Escrever `src/app/devices.rs`**

```rust
use std::time::Instant;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use crate::app::{AppState, ConfirmOffDelete};

impl AppState {
    // ── Callback para snapshot.rs ─────────────────────────────────────────────

    pub(crate) fn on_devices_updated(&mut self) {
        if !self.devices.is_empty() {
            self.selected_dev = self.selected_dev.min(self.devices.len() - 1);
        }
    }

    // ── Handlers públicos (chamados por handle_action) ────────────────────────

    pub(crate) fn handle_device_up(&mut self) {
        self.selected_dev = self.selected_dev.saturating_sub(1);
    }

    pub(crate) fn handle_device_down(&mut self) {
        if !self.devices.is_empty() {
            self.selected_dev = (self.selected_dev + 1).min(self.devices.len() - 1);
        }
    }

    pub(crate) fn handle_request_confirm(&mut self, kind: DeviceOpKind) {
        self.confirm_action = Some(kind);
    }

    pub(crate) fn handle_cancel_confirm(&mut self) {
        self.confirm_action = None;
    }

    pub(crate) fn handle_execute_device_op(&mut self, path: std::path::PathBuf, kind: DeviceOpKind) {
        self.confirm_action = None;
        self.confirm_off_delete = None;
        self.device_op_started = Some(Instant::now());
        self.device_op = Some(DeviceOp {
            path,
            kind,
            status: OpStatus::Running,
        });
    }

    pub(crate) fn handle_device_op_update(&mut self, op: DeviceOp) {
        if let OpStatus::Error(ref msg) = op.status {
            self.error_msg = Some((msg.clone(), Instant::now()));
        }
        self.device_op = Some(op);
    }

    // ── Phase 6 — ConfirmOffDelete ──────────────────────────────────────────

    pub(crate) fn handle_request_confirm_off_delete(&mut self) {
        if let Some(dev) = self.devices.get(self.selected_dev) {
            self.confirm_off_delete = Some(ConfirmOffDelete {
                path: dev.path.clone(),
                delete_file: false,
                active: dev.active,
            });
        }
    }

    pub(crate) fn handle_toggle_confirm_delete_file(&mut self) {
        if let Some(ref mut modal) = self.confirm_off_delete {
            modal.delete_file = !modal.delete_file;
        }
    }

    pub(crate) fn handle_cancel_confirm_off_delete(&mut self) {
        self.confirm_off_delete = None;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_helpers::*;
    use std::path::PathBuf;
    use crate::platform::SwapKind;

    #[test]
    fn device_up_decrements_selected_dev_with_floor_at_zero() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.selected_dev = 1;
        state.handle_action(Action::DeviceUp);
        assert_eq!(state.selected_dev, 0);
        state.handle_action(Action::DeviceUp); // already at 0
        assert_eq!(state.selected_dev, 0);
    }

    #[test]
    fn device_down_increments_selected_dev_capped_at_last() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.handle_action(Action::DeviceDown);
        assert_eq!(state.selected_dev, 1);
        state.handle_action(Action::DeviceDown); // already at last
        assert_eq!(state.selected_dev, 1);
    }

    #[test]
    fn request_confirm_sets_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::RequestConfirm(DeviceOpKind::Off));
        assert_eq!(state.confirm_action, Some(DeviceOpKind::Off));
    }

    #[test]
    fn cancel_confirm_clears_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::On);
        state.handle_action(Action::CancelConfirm);
        assert!(state.confirm_action.is_none());
    }

    #[test]
    fn execute_device_op_sets_running_and_clears_confirm() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::Off);
        state.handle_action(Action::ExecuteDeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
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
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
            status: OpStatus::Done,
        }));
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Done);
    }

    #[test]
    fn device_op_update_with_error_sets_error_msg() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
            status: OpStatus::Error("swapoff failed: EPERM".to_string()),
        }));
        assert!(matches!(&state.error_msg, Some((msg, _)) if msg == "swapoff failed: EPERM"));
        assert!(state.device_op.is_some());
    }

    #[test]
    fn request_confirm_off_delete_opens_modal() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        let modal = state.confirm_off_delete.as_ref().unwrap();
        assert_eq!(modal.path, PathBuf::from("/swapfile"));
        assert!(!modal.delete_file);
        assert!(modal.active);
    }

    #[test]
    fn toggle_confirm_delete_file_flips() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        assert!(!state.confirm_off_delete.as_ref().unwrap().delete_file);
        state.handle_action(Action::ToggleConfirmDeleteFile);
        assert!(state.confirm_off_delete.as_ref().unwrap().delete_file);
        state.handle_action(Action::ToggleConfirmDeleteFile);
        assert!(!state.confirm_off_delete.as_ref().unwrap().delete_file);
    }

    #[test]
    fn cancel_confirm_off_delete_clears_modal() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        assert!(state.confirm_off_delete.is_some());
        state.handle_action(Action::CancelConfirmOffDelete);
        assert!(state.confirm_off_delete.is_none());
    }
}
```

Run: `rtk cargo check`
Expected: FAIL — processes.rs e create_swap.rs ainda não existem

---

## Task 4: Extrair `processes.rs` — Process Handlers

**Files:**
- Create: `src/app/processes.rs`

- [ ] **Step 1: Escrever `src/app/processes.rs`**

```rust
use crate::actions::{Action, SortColumn, SortDir};
use crate::app::AppState;

impl AppState {
    // ── Callback para snapshot.rs ─────────────────────────────────────────────

    pub(crate) fn on_processes_updated(&mut self) {
        self.sort_processes();
        let len = self.filtered_len();
        self.selected_row = if len > 0 {
            self.selected_row.min(len - 1)
        } else {
            0
        };
    }

    // ── Handlers públicos ───────────────────────────────────────────────────

    pub(crate) fn handle_navigate_up(&mut self) {
        self.selected_row = self.selected_row.saturating_sub(1);
    }

    pub(crate) fn handle_navigate_down(&mut self) {
        let len = self.filtered_len();
        if len > 0 {
            self.selected_row = (self.selected_row + 1).min(len - 1);
        }
    }

    pub(crate) fn handle_sort_by(&mut self, col: SortColumn) {
        if col == self.sort_col {
            self.sort_dir = if self.sort_dir == SortDir::Asc {
                SortDir::Desc
            } else {
                SortDir::Asc
            };
        } else {
            self.sort_col = col;
            self.sort_dir = SortDir::Desc;
        }
        self.sort_processes();
    }

    pub(crate) fn handle_enter_filter_mode(&mut self) {
        self.filter_mode = true;
    }

    pub(crate) fn handle_filter_char(&mut self, c: char) {
        self.filter_text.push(c);
        self.selected_row = 0;
    }

    pub(crate) fn handle_filter_backspace(&mut self) {
        self.filter_text.pop();
        self.selected_row = 0;
    }

    pub(crate) fn handle_exit_filter_mode(&mut self) {
        self.filter_mode = false;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_helpers::*;

    #[test]
    fn sort_col_defaults_to_swap() {
        let state = AppState::new(make_caps());
        assert_eq!(state.sort_col, SortColumn::Swap);
    }

    #[test]
    fn sort_dir_defaults_to_desc() {
        let state = AppState::new(make_caps());
        assert_eq!(state.sort_dir, SortDir::Desc);
    }

    #[test]
    fn sort_by_same_column_toggles_direction() {
        let mut state = AppState::new(make_caps());
        // starts Swap/Desc
        state.handle_action(Action::SortBy(SortColumn::Swap));
        assert_eq!(state.sort_col, SortColumn::Swap);
        assert_eq!(state.sort_dir, SortDir::Asc);
    }

    #[test]
    fn sort_by_different_column_resets_to_desc() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::SortBy(SortColumn::Cpu));
        assert_eq!(state.sort_col, SortColumn::Cpu);
        assert_eq!(state.sort_dir, SortDir::Desc);
    }

    #[test]
    fn navigate_down_increments_selected_row() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.handle_action(Action::NavigateDown);
        assert_eq!(state.selected_row, 1);
    }

    #[test]
    fn navigate_down_clamps_at_last_row() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.handle_action(Action::NavigateDown);
        state.handle_action(Action::NavigateDown); // beyond end
        assert_eq!(state.selected_row, 1);
    }

    #[test]
    fn navigate_up_decrements_selected_row() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.handle_action(Action::NavigateDown);
        state.handle_action(Action::NavigateUp);
        assert_eq!(state.selected_row, 0);
    }

    #[test]
    fn navigate_up_clamps_at_zero() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::NavigateUp);
        assert_eq!(state.selected_row, 0);
    }

    #[test]
    fn enter_filter_mode_sets_flag() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::EnterFilterMode);
        assert!(state.filter_mode);
    }

    #[test]
    fn filter_char_appends_and_resets_selection() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.selected_row = 1;
        state.handle_action(Action::FilterChar('f'));
        assert_eq!(state.filter_text, "f");
        assert_eq!(state.selected_row, 0);
    }

    #[test]
    fn filter_backspace_removes_last_char() {
        let mut state = AppState::new(make_caps());
        state.filter_text = "fi".to_string();
        state.handle_action(Action::FilterBackspace);
        assert_eq!(state.filter_text, "f");
    }

    #[test]
    fn exit_filter_mode_clears_flag_keeps_text() {
        let mut state = AppState::new(make_caps());
        state.filter_mode = true;
        state.filter_text = "fox".to_string();
        state.handle_action(Action::ExitFilterMode);
        assert!(!state.filter_mode);
        assert_eq!(state.filter_text, "fox");
    }

    #[test]
    fn filtered_len_with_empty_filter_returns_all() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "firefox", 0), make_process(2, "bash", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.filtered_len(), 2);
    }

    #[test]
    fn filtered_len_with_filter_returns_matches() {
        let mut state = AppState::new(make_caps());
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "firefox", 0), make_process(2, "bash", 0)];
        state.handle_action(Action::UpdateSnapshot(snap));
        state.filter_text = "fire".to_string();
        assert_eq!(state.filtered_len(), 1);
    }
}
```

Run: `rtk cargo check`
Expected: FAIL — create_swap.rs ainda não existe

---

## Task 5: Extrair `create_swap.rs` — Create Swap Modal Handlers

**Files:**
- Create: `src/app/create_swap.rs`

- [ ] **Step 1: Escrever `src/app/create_swap.rs`**

```rust
use crate::actions::Action;
use crate::app::AppState;
use crate::create_swap::{CreateSwapField, CreateSwapMode};
use crate::platform::CreateSwapProgress;

impl AppState {
    pub(crate) fn handle_open_create_swap(&mut self) {
        self.create_swap_modal = Some(crate::create_swap::CreateSwapModal::default());
    }

    pub(crate) fn handle_close_create_swap(&mut self) {
        self.create_swap_modal = None;
    }

    pub(crate) fn handle_create_swap_return_to_form(&mut self) {
        if let Some(modal) = self.create_swap_modal.as_mut() {
            modal.mode = CreateSwapMode::Form {
                focused_field: CreateSwapField::Submit,
            };
        }
    }

    pub(crate) fn handle_create_swap_focus_field(&mut self, field: CreateSwapField) {
        if let Some(modal) = self.create_swap_modal.as_mut()
            && let CreateSwapMode::Form { focused_field } = &mut modal.mode
        {
            modal.completions.clear();
            modal.completion_sel = None;
            *focused_field = field;
        }
    }

    pub(crate) fn handle_create_swap_input_event(&mut self, event: crossterm::event::Event) {
        if let Some(modal) = self.create_swap_modal.as_mut()
            && let CreateSwapMode::Form { focused_field } = modal.mode
        {
            modal.completions.clear();
            modal.completion_sel = None;
            use tui_input::backend::crossterm::EventHandler;
            let target = match focused_field {
                CreateSwapField::Path => Some(&mut modal.path_input),
                CreateSwapField::Size => Some(&mut modal.size_input),
                CreateSwapField::Priority => Some(&mut modal.priority_input),
                _ => None,
            };
            if let Some(input) = target {
                let _ = input.handle_event(&event);
            }
        }
    }

    pub(crate) fn handle_create_swap_toggle_unit(&mut self) {
        if let Some(modal) = self.create_swap_modal.as_mut() {
            modal.completions.clear();
            modal.completion_sel = None;
            modal.size_unit = modal.size_unit.toggled();
        }
    }

    pub(crate) fn handle_create_swap_toggle_activate(&mut self) {
        if let Some(modal) = self.create_swap_modal.as_mut() {
            modal.completions.clear();
            modal.completion_sel = None;
            modal.activate_after = !modal.activate_after;
        }
    }

    pub(crate) fn handle_create_swap_submit(&mut self, activate_only: bool) {
        if let Some(modal) = self.create_swap_modal.as_mut() {
            if !activate_only {
                let path = modal.path_input.value().trim().to_string();
                if path.is_empty() {
                    modal.validation_error = Some("Path is required".to_string());
                    return;
                }
                if path.chars().any(|c| c.is_whitespace() || c.is_control()) {
                    modal.validation_error = Some(
                        "Path cannot contain spaces, tabs, or control characters".to_string(),
                    );
                    return;
                }
                if !std::path::Path::new(&path).is_absolute() {
                    modal.validation_error = Some("Path must be absolute".to_string());
                    return;
                }
                let size_n: u64 = match modal.size_input.value().trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        modal.validation_error =
                            Some("Size must be a positive integer".to_string());
                        return;
                    }
                };
                if size_n == 0 {
                    modal.validation_error =
                        Some("Size must be greater than zero".to_string());
                    return;
                }
                let prio_n: i32 = match modal.priority_input.value().trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        modal.validation_error = Some(
                            "Priority must be an integer between -1 and 32767".to_string(),
                        );
                        return;
                    }
                };
                if !(-1..=32767).contains(&prio_n) {
                    modal.validation_error = Some(
                        "Priority must be an integer between -1 and 32767".to_string(),
                    );
                    return;
                }
            }
            modal.validation_error = None;
            let mut steps = vec![
                crate::create_swap::CreateSwapStep::pending("Check disk space"),
                crate::create_swap::CreateSwapStep::pending("Check target file"),
                crate::create_swap::CreateSwapStep::pending("Detect filesystem"),
                crate::create_swap::CreateSwapStep::pending("Allocate file"),
                crate::create_swap::CreateSwapStep::pending("chmod 600"),
                crate::create_swap::CreateSwapStep::pending("mkswap"),
                crate::create_swap::CreateSwapStep::pending("swapon"),
            ];
            if activate_only {
                for step in steps.iter_mut().take(6) {
                    step.status = crate::platform::StepStatus::Done;
                }
            }
            modal.mode = CreateSwapMode::Progress { steps };
        }
    }

    pub(crate) fn handle_create_swap_progress(&mut self, progress: CreateSwapProgress) {
        use crate::platform::CreateSwapProgress;
        if let Some(modal) = self.create_swap_modal.as_mut() {
            match progress {
                CreateSwapProgress::StepUpdate { index, status } => {
                    if let CreateSwapMode::Progress { steps } = &mut modal.mode
                        && let Some(step) = steps.get_mut(index)
                    {
                        step.status = status;
                    }
                }
                CreateSwapProgress::ConfirmActivateOnly { path, size_bytes } => {
                    modal.mode = CreateSwapMode::ConfirmActivateOnly { path, size_bytes };
                }
            }
        }
    }

    pub(crate) fn handle_create_swap_set_completions(&mut self, items: Vec<String>) {
        if let Some(ref mut modal) = self.create_swap_modal {
            modal.completion_sel = if items.is_empty() { None } else { Some(0) };
            modal.completions = items;
        }
    }

    pub(crate) fn handle_create_swap_completion_move(&mut self, delta: i16) {
        if let Some(ref mut modal) = self.create_swap_modal
            && !modal.completions.is_empty()
        {
            let len = modal.completions.len() as i16;
            let cur = modal.completion_sel.unwrap_or(0) as i16;
            let next = ((cur + delta) % len + len) % len;
            modal.completion_sel = Some(next as usize);
        }
    }

    pub(crate) fn handle_create_swap_apply_completion(&mut self) {
        if let Some(ref mut modal) = self.create_swap_modal {
            if let Some(sel) = modal.completion_sel
                && let Some(value) = modal.completions.get(sel).cloned()
            {
                modal.path_input = tui_input::Input::from(value);
            }
            modal.completions.clear();
            modal.completion_sel = None;
        }
    }

    pub(crate) fn handle_create_swap_clear_completions(&mut self) {
        if let Some(ref mut modal) = self.create_swap_modal {
            modal.completions.clear();
            modal.completion_sel = None;
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_helpers::*;
    use crate::create_swap::{CreateSwapField, CreateSwapMode, SizeUnit};
    use crate::platform::{CreateSwapProgress, StepStatus};
    use std::path::PathBuf;
    use tui_input::Input;

    #[test]
    fn open_create_swap_initializes_modal() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        assert!(state.create_swap_modal.is_some());
    }

    #[test]
    fn close_create_swap_clears_modal() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CloseCreateSwap);
        assert!(state.create_swap_modal.is_none());
    }

    #[test]
    fn focus_field_updates_form_focus() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapFocusField(CreateSwapField::Size));
        let modal = state.create_swap_modal.as_ref().unwrap();
        match modal.mode {
            CreateSwapMode::Form { focused_field } => {
                assert_eq!(focused_field, CreateSwapField::Size);
            }
            _ => panic!("expected Form mode"),
        }
    }

    #[test]
    fn toggle_unit_flips_mb_gb() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        let before = state.create_swap_modal.as_ref().unwrap().size_unit;
        assert_eq!(before, SizeUnit::Gb);
        state.handle_action(Action::CreateSwapToggleUnit);
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().size_unit,
            SizeUnit::Mb
        );
    }

    #[test]
    fn toggle_activate_flips_boolean() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        assert!(state.create_swap_modal.as_ref().unwrap().activate_after);
        state.handle_action(Action::CreateSwapToggleActivate);
        assert!(!state.create_swap_modal.as_ref().unwrap().activate_after);
    }

    #[test]
    fn submit_transitions_to_progress_with_seven_steps() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::Progress { steps } => assert_eq!(steps.len(), 7),
            _ => panic!("expected Progress mode"),
        }
    }

    #[test]
    fn step_update_replaces_status_at_index() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        state.handle_action(Action::CreateSwapProgress(CreateSwapProgress::StepUpdate {
            index: 0,
            status: StepStatus::Running,
        }));
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::Progress { steps } => {
                assert_eq!(steps[0].status, StepStatus::Running);
            }
            _ => panic!("expected Progress mode"),
        }
    }

    #[test]
    fn return_to_form_preserves_inputs_and_validation_error() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        {
            let modal = state.create_swap_modal.as_mut().unwrap();
            modal.path_input = Input::from("/swapfile");
            modal.validation_error = Some("some prior error".to_string());
        }
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        state.handle_action(Action::CreateSwapReturnToForm);
        let modal = state.create_swap_modal.as_ref().unwrap();
        match modal.mode {
            CreateSwapMode::Form { focused_field } => {
                assert_eq!(focused_field, CreateSwapField::Submit);
            }
            _ => panic!("expected Form mode"),
        }
    }

    #[test]
    fn open_confirm_activate_only_switches_mode() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapProgress(
            CreateSwapProgress::ConfirmActivateOnly {
                path: PathBuf::from("/swapfile"),
                size_bytes: 2_147_483_648,
            },
        ));
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::ConfirmActivateOnly { path, size_bytes } => {
                assert_eq!(path, &PathBuf::from("/swapfile"));
                assert_eq!(*size_bytes, 2_147_483_648);
            }
            _ => panic!("expected ConfirmActivateOnly mode"),
        }
    }

    #[test]
    fn set_completions_stores_and_selects_first() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec![
            "/swapfile".to_string(),
            "/swap.img".to_string(),
        ]));
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert_eq!(modal.completions.len(), 2);
        assert_eq!(modal.completion_sel, Some(0));
    }

    #[test]
    fn set_completions_empty_sets_sel_none() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec![]));
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert!(modal.completions.is_empty());
        assert_eq!(modal.completion_sel, None);
    }

    #[test]
    fn completion_move_wraps_forward() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
        ]));
        state.handle_action(Action::CreateSwapCompletionMove(1));
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().completion_sel,
            Some(1)
        );
        state.handle_action(Action::CreateSwapCompletionMove(1));
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().completion_sel,
            Some(2)
        );
        state.handle_action(Action::CreateSwapCompletionMove(1));
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().completion_sel,
            Some(0)
        ); // wrap
    }

    #[test]
    fn completion_move_wraps_backward() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec![
            "a".to_string(),
            "b".to_string(),
        ]));
        state.handle_action(Action::CreateSwapCompletionMove(-1));
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().completion_sel,
            Some(1)
        ); // wrap
    }

    #[test]
    fn apply_completion_sets_path_and_clears() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec![
            "/swapfile".to_string(),
            "/swap.img".to_string(),
        ]));
        state.handle_action(Action::CreateSwapCompletionMove(1)); // select /swap.img
        state.handle_action(Action::CreateSwapApplyCompletion);
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert_eq!(modal.path_input.value(), "/swap.img");
        assert!(modal.completions.is_empty());
        assert_eq!(modal.completion_sel, None);
    }

    #[test]
    fn clear_completions_resets_state() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSetCompletions(vec!["x".to_string()]));
        state.handle_action(Action::CreateSwapClearCompletions);
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert!(modal.completions.is_empty());
        assert_eq!(modal.completion_sel, None);
    }

    #[test]
    fn create_swap_submit_rejects_empty_path() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert!(modal.validation_error.is_some());
        assert!(matches!(modal.mode, CreateSwapMode::Form { .. }));
    }

    #[test]
    fn create_swap_submit_rejects_relative_path() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("relative/path");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        let modal = state.create_swap_modal.as_ref().unwrap();
        assert_eq!(
            modal.validation_error.as_deref(),
            Some("Path must be absolute")
        );
    }

    #[test]
    fn create_swap_submit_rejects_zero_size() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.create_swap_modal.as_mut().unwrap().size_input = Input::from("0");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().validation_error.as_deref(),
            Some("Size must be greater than zero")
        );
    }

    #[test]
    fn create_swap_submit_rejects_non_numeric_size() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.create_swap_modal.as_mut().unwrap().size_input = Input::from("abc");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        assert_eq!(
            state.create_swap_modal.as_ref().unwrap().validation_error.as_deref(),
            Some("Size must be a positive integer")
        );
    }

    #[test]
    fn create_swap_submit_rejects_out_of_range_priority() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.create_swap_modal.as_mut().unwrap().priority_input = Input::from("99999");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        assert!(
            state.create_swap_modal.as_ref().unwrap().validation_error.is_some()
        );
    }

    #[test]
    fn create_swap_submit_valid_transitions_to_progress() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
        state.create_swap_modal.as_mut().unwrap().size_input = Input::from("2048");
        state.create_swap_modal.as_mut().unwrap().priority_input = Input::from("-1");
        state.handle_action(Action::CreateSwapSubmit {
            activate_only: false,
        });
        assert!(
            matches!(state.create_swap_modal.as_ref().unwrap().mode, CreateSwapMode::Progress { .. })
        );
        assert!(state.create_swap_modal.as_ref().unwrap().validation_error.is_none());
    }
}
```

Run: `rtk cargo check`
Expected: PASS — todos os módulos existem

---

## Task 6: Remover `app.rs` Original e Atualizar `lib.rs`/`main.rs`

**Files:**
- Delete: `src/app.rs`
- Modify: `src/lib.rs` (adicionar declaração `mod app;` se necessário)
- Modify: `src/main.rs` (verificar imports — deve continuar `use swaptop::app::AppState`)

- [ ] **Step 1: Verificar se `src/app.rs` já foi deletado ou renomeado**

```bash
ls src/app.rs 2>/dev/null && echo "AINDA EXISTE — DELETAR" || echo "Já foi removido"
```

Se ainda existir, deletar:
```bash
rm src/app.rs
```

- [ ] **Step 2: Verificar `src/lib.rs`**

Read: `src/lib.rs`

Garantir que contém:
```rust
pub mod app;
// ... outros mods
```

Se não tiver `pub mod app;`, adicionar. Se já tiver, nada muda.

- [ ] **Step 3: Verificar `src/main.rs` imports**

Read: `src/main.rs` — procurar linhas como:
```rust
use swaptop::app::AppState;
```

Se estiver importando `swaptop::app::AppState` ou similar, deve continuar funcionando pois `mod.rs` exporta o mesmo símbolo.

Run: `rtk cargo build`
Expected: PASS

---

## Task 7: Atualizar Documentação

**Files:**
- Modify: `CLAUDE.md`
- Modify: `README.md`

- [ ] **Step 1: Atualizar `CLAUDE.md`**

Edit `CLAUDE.md` — seção "Modules":

```markdown
- **`app/`** — `AppState` struct + `handle_action()` reducer (pure, no I/O). Split by domain:
  - `mod.rs` — struct, dispatcher, test helpers
  - `snapshot.rs` — `UpdateSnapshot` orquestrator with domain callbacks
  - `devices.rs` — device navigation, operations, confirmations
  - `processes.rs` — table navigation, sorting, filtering
  - `create_swap.rs` — swap file wizard modal handlers
```

Também atualizar layer rule:
```markdown
| `app/` (reducer) | mutate `AppState` | do I/O, call platform, send commands |
```

- [ ] **Step 2: Atualizar `README.md`**

Edit `README.md` — seção "File structure", linha `app.rs`:

```
├── app/
│   ├── mod.rs          # AppState + handle_action dispatcher
│   ├── snapshot.rs     # UpdateSnapshot orquestrator
│   ├── devices.rs      # Device ops + confirmation modals
│   ├── processes.rs    # Sortable/filterable process table state
│   └── create_swap.rs  # Wizard form state handlers
```

Remover linha antiga `├── app.rs # AppState + handle_action() reducer`.

- [ ] **Step 3: Commit das mudanças de docs**

```bash
git add CLAUDE.md README.md
git diff --cached
```

---

## Task 8: Validação Final

- [ ] **Step 1: Build zero warnings**

```bash
rtk cargo build
```
Expected: zero warnings, zero errors

- [ ] **Step 2: Clippy passa**

```bash
rtk cargo clippy -- -D warnings
```
Expected: zero warnings

- [ ] **Step 3: Formatação correta**

```bash
rtk cargo fmt --check
```
Expected: zero diffs

- [ ] **Step 4: Todos os tests passam**

```bash
rtk cargo test
```
Expected: all tests pass, mesmo número que antes da refatoração

- [ ] **Step 5: Verificar que `src/app.rs` não existe mais**

```bash
test ! -f src/app.rs && echo "OK: app.rs removido" || echo "ERRO: app.rs ainda existe"
```

- [ ] **Step 6: Commit final**

```bash
git add src/app/
# Se CLAUDE.md/README.md não foram commitados ainda:
git add CLAUDE.md README.md
git status
git commit -m "refactor(app): split app.rs into domain modules

Split monolithic app.rs (~1500 lines) into focused modules mirroring src/ui/:
- mod.rs: AppState struct + handle_action dispatcher + test helpers
- snapshot.rs: UpdateSnapshot orchestrator with domain callbacks
- devices.rs: Device navigation, operations, confirmations
- processes.rs: Table navigation, sorting, filtering
- create_swap.rs: Swap file wizard modal handlers

Each module owns its tests. Zero behavior change."
```

---

## Self-Review Checklist

### Spec coverage
- [x] `UpdateSnapshot` orquestrador em `snapshot.rs` com callbacks — Task 2
- [x] Device handlers em `devices.rs` — Task 3
- [x] Process handlers em `processes.rs` — Task 4
- [x] Create swap handlers em `create_swap.rs` — Task 5
- [x] Dispatcher em `mod.rs` — Task 1
- [x] Testes distribuídos por módulo — Todos as tasks
- [x] Atualização de docs — Task 7

### Placeholder scan
- [x] Zero "TBD"/"TODO" no plano
- [x] Código completo em cada step
- [x] Comandos exatos com expected output

### Type consistency
- [x] `AppState::handle_action` dispatcha para métodos com nomes consistentes: `handle_<action_snake_case>`
- [x] Callbacks: `on_processes_updated`, `on_devices_updated`
- [x] Test helpers: `make_caps`, `make_snapshot`, `make_device`, `make_process` em `test_helpers`
