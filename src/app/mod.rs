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
