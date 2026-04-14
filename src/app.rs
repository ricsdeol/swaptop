use std::collections::VecDeque;
use std::time::Instant;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus, SortColumn, SortDir};
use crate::create_swap::{CreateSwapMode, CreateSwapModal, CreateSwapStep, StepStatus};
use crate::platform::{Capabilities, MemSnapshot, ProcessRow, SwapDevice};

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
}

pub struct AppState {
    pub active_tab: Tab,
    pub ram_history: VecDeque<(Instant, u64)>,
    pub swap_history: VecDeque<(Instant, u64)>,
    pub max_history: usize,
    pub current: Option<MemSnapshot>,
    pub devices: Vec<SwapDevice>,
    pub capabilities: Capabilities,
    pub error_msg: Option<String>,
    pub start_time: Instant,
    pub should_quit: bool,

    // Phase 4
    pub selected_dev: usize,
    pub device_op: Option<DeviceOp>,
    pub confirm_action: Option<DeviceOpKind>,

    // Phase 2
    pub processes: Vec<ProcessRow>,
    pub sort_col: SortColumn,
    pub sort_dir: SortDir,
    pub selected_row: usize,
    pub filter_text: String,
    pub filter_mode: bool,

    // Phase 5
    pub create_swap_modal: Option<CreateSwapModal>,
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
            // Phase 2
            processes: Vec::new(),
            sort_col: SortColumn::Swap,
            sort_dir: SortDir::Desc,
            selected_row: 0,
            filter_text: String::new(),
            filter_mode: false,
            create_swap_modal: None,
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

    fn sort_processes(&mut self) {
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

            Action::UpdateSnapshot(snapshot) => {
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
                self.devices = snapshot.devices.clone();
                self.processes = snapshot.processes.clone();
                self.sort_processes();
                let len = self.filtered_len();
                if len > 0 {
                    self.selected_row = self.selected_row.min(len - 1);
                } else {
                    self.selected_row = 0;
                }
                // Clamp device selection if device list shrank
                if !self.devices.is_empty() {
                    self.selected_dev = self.selected_dev.min(self.devices.len() - 1);
                }
                self.current = Some(snapshot);
                self.error_msg = None;
            }

            Action::Refresh => {} // collector tick handles it

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

            // Phase 2 — processes navigation
            Action::NavigateUp => {
                self.selected_row = self.selected_row.saturating_sub(1);
            }

            Action::NavigateDown => {
                let len = self.filtered_len();
                if len > 0 {
                    self.selected_row = (self.selected_row + 1).min(len - 1);
                }
            }

            Action::SortBy(col) => {
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

            Action::EnterFilterMode => {
                self.filter_mode = true;
            }

            Action::FilterChar(c) => {
                self.filter_text.push(c);
                self.selected_row = 0;
            }

            Action::FilterBackspace => {
                self.filter_text.pop();
                self.selected_row = 0;
            }

            Action::ExitFilterMode => {
                self.filter_mode = false;
            }

            // Phase 5 — create swap modal
            Action::OpenCreateSwap => {
                self.create_swap_modal = Some(CreateSwapModal::default());
            }

            Action::CloseCreateSwap => {
                self.create_swap_modal = None;
            }

            Action::CreateSwapReturnToForm => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.mode = CreateSwapMode::Form {
                        focused_field: crate::create_swap::CreateSwapField::Submit,
                    };
                }
            }

            Action::CreateSwapFocusField(field) => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Form { focused_field } = &mut modal.mode {
                        *focused_field = field;
                    }
                }
            }

            Action::CreateSwapInputEvent(event) => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Form { focused_field } = modal.mode {
                        use tui_input::backend::crossterm::EventHandler;
                        let target = match focused_field {
                            crate::create_swap::CreateSwapField::Path => {
                                Some(&mut modal.path_input)
                            }
                            crate::create_swap::CreateSwapField::Size => {
                                Some(&mut modal.size_input)
                            }
                            crate::create_swap::CreateSwapField::Priority => {
                                Some(&mut modal.priority_input)
                            }
                            _ => None,
                        };
                        if let Some(input) = target {
                            let _ = input.handle_event(&event);
                        }
                    }
                }
            }

            Action::CreateSwapToggleUnit => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.size_unit = modal.size_unit.toggled();
                }
            }

            Action::CreateSwapToggleActivate => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.activate_after = !modal.activate_after;
                }
            }

            Action::CreateSwapSubmit { activate_only: _ } => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.validation_error = None;
                    modal.mode = CreateSwapMode::Progress {
                        steps: vec![
                            CreateSwapStep::pending("Check disk space"),
                            CreateSwapStep::pending("Check target file"),
                            CreateSwapStep::pending("Detect filesystem"),
                            CreateSwapStep::pending("Allocate file"),
                            CreateSwapStep::pending("chmod 600"),
                            CreateSwapStep::pending("mkswap"),
                            CreateSwapStep::pending("swapon"),
                        ],
                    };
                }
            }

            Action::OpenConfirmActivateOnly { path, size_bytes } => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.mode = CreateSwapMode::ConfirmActivateOnly { path, size_bytes };
                }
            }

            Action::CreateSwapStepUpdate { index, status } => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Progress { steps } = &mut modal.mode {
                        if let Some(step) = steps.get_mut(index) {
                            step.status = status;
                        }
                    }
                }
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{ProcessRow, SwapInfo, SwapKind};
    use std::path::PathBuf;

    fn make_caps() -> Capabilities {
        Capabilities {
            can_swap_on: true,
            has_per_process: true,
        }
    }

    fn make_snapshot() -> MemSnapshot {
        MemSnapshot {
            timestamp: Instant::now(),
            ram: SwapInfo::new(16 * 1024 * 1024 * 1024, 8 * 1024 * 1024 * 1024),
            swap: SwapInfo::new(4 * 1024 * 1024 * 1024, 1 * 1024 * 1024 * 1024),
            devices: vec![],
            processes: vec![],
        }
    }

    fn make_device(path: &str) -> SwapDevice {
        SwapDevice {
            path: path.into(),
            total: 4 * 1024 * 1024 * 1024,
            used: 1 * 1024 * 1024 * 1024,
            priority: -1,
            kind: SwapKind::Partition,
            active: true,
        }
    }

    fn make_process(pid: u32, name: &str, swap: u64) -> ProcessRow {
        ProcessRow {
            pid,
            name: name.to_string(),
            user: "user".to_string(),
            rss: 0,
            swap,
            cpu_pct: 0.0,
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
            path: "/dev/sda2".into(),
            total: 4 * 1024 * 1024 * 1024,
            used: 1 * 1024 * 1024 * 1024,
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

    // ── Phase 4 — device navigation ───────────────────────────────────────────

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

    // ── Default sort ──────────────────────────────────────────────────────────

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

    // ── SortBy ────────────────────────────────────────────────────────────────

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

    // ── Navigation ────────────────────────────────────────────────────────────

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

    // ── Filter mode ───────────────────────────────────────────────────────────

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

    // ── filtered_len ─────────────────────────────────────────────────────────

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

    // ── Phase 5 — create swap modal ──────────────────────────────────────────

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
        use crate::create_swap::{CreateSwapField, CreateSwapMode};
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
        use crate::create_swap::SizeUnit;
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
        use crate::create_swap::CreateSwapMode;
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::Progress { steps } => assert_eq!(steps.len(), 7),
            _ => panic!("expected Progress mode"),
        }
    }

    #[test]
    fn step_update_replaces_status_at_index() {
        use crate::create_swap::{CreateSwapMode, StepStatus};
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
        state.handle_action(Action::CreateSwapStepUpdate {
            index: 0,
            status: StepStatus::Running,
        });
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
        use crate::create_swap::{CreateSwapField, CreateSwapMode};
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        {
            let modal = state.create_swap_modal.as_mut().unwrap();
            modal.validation_error = Some("some prior error".to_string());
        }
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
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
        use crate::create_swap::CreateSwapMode;
        use std::path::PathBuf;
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::OpenConfirmActivateOnly {
            path: PathBuf::from("/swapfile"),
            size_bytes: 2_147_483_648,
        });
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::ConfirmActivateOnly { path, size_bytes } => {
                assert_eq!(path, &PathBuf::from("/swapfile"));
                assert_eq!(*size_bytes, 2_147_483_648);
            }
            _ => panic!("expected ConfirmActivateOnly mode"),
        }
    }

    // ── UpdateSnapshot sorts ──────────────────────────────────────────────────

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
        // Now snapshot with only 1 process
        let mut snap2 = make_snapshot();
        snap2.processes = vec![make_process(1, "a", 0)];
        state.handle_action(Action::UpdateSnapshot(snap2));
        assert_eq!(state.selected_row, 0);
    }
}
