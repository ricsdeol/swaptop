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

    // ── Handlers ─────────────────────────────────────────────────────────────

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
    use crate::actions::Action;
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
