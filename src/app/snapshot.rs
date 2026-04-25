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

        // 3. Domain-specific reactions (delegated to other modules)
        self.on_processes_updated();
        self.on_devices_updated();

        // 4. Snapshot metadata
        self.current = Some(snapshot);
        self.last_collect_completed = Instant::now();

        // 5. Clear stale errors
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_helpers::*;
    use crate::platform::{SwapDevice, SwapKind};

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
        // A fresh error (< 5 s old) must NOT be cleared by UpdateSnapshot.
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
        // Now snapshot with only 1 process
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
        // Snapshot with only 1 device
        let mut snap = make_snapshot();
        snap.devices = vec![make_device("/dev/sda2")];
        state.handle_action(Action::UpdateSnapshot(snap));
        assert_eq!(state.selected_dev, 0);
    }
}
