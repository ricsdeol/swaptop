use std::collections::VecDeque;
use std::time::Instant;

use crate::actions::Action;
use crate::platform::{Capabilities, MemSnapshot, SwapDevice};

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
    CreateSwap,
}

pub struct AppState {
    pub active_tab:   Tab,
    pub ram_history:  VecDeque<(Instant, u64)>,
    pub swap_history: VecDeque<(Instant, u64)>,
    pub max_history:  usize,
    pub current:      Option<MemSnapshot>,
    pub devices:      Vec<SwapDevice>,
    #[allow(dead_code)]
    pub capabilities: Capabilities,
    pub error_msg:    Option<String>,
    pub start_time:   Instant,
    pub should_quit:  bool,
}

impl AppState {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            active_tab:   Tab::Overview,
            ram_history:  VecDeque::new(),
            swap_history: VecDeque::new(),
            max_history:  3600,
            current:      None,
            devices:      Vec::new(),
            capabilities,
            error_msg:    None,
            start_time:   Instant::now(),
            should_quit:  false,
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
            }

            Action::Refresh => {} // collector tick handles it
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
        state.handle_action(Action::NextTab); // wraps
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
        snap.devices = vec![crate::platform::SwapDevice {
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
}
