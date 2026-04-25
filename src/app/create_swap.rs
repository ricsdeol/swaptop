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
                    modal.validation_error =
                        Some("Path cannot contain spaces, tabs, or control characters".to_string());
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
                    modal.validation_error = Some("Size must be greater than zero".to_string());
                    return;
                }
                let prio_n: i32 = match modal.priority_input.value().trim().parse() {
                    Ok(n) => n,
                    Err(_) => {
                        modal.validation_error =
                            Some("Priority must be an integer between -1 and 32767".to_string());
                        return;
                    }
                };
                if !(-1..=32767).contains(&prio_n) {
                    modal.validation_error =
                        Some("Priority must be an integer between -1 and 32767".to_string());
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
    use crate::actions::Action;
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
            state
                .create_swap_modal
                .as_ref()
                .unwrap()
                .validation_error
                .as_deref(),
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
            state
                .create_swap_modal
                .as_ref()
                .unwrap()
                .validation_error
                .as_deref(),
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
            state
                .create_swap_modal
                .as_ref()
                .unwrap()
                .validation_error
                .is_some()
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
        assert!(matches!(
            state.create_swap_modal.as_ref().unwrap().mode,
            CreateSwapMode::Progress { .. }
        ));
        assert!(
            state
                .create_swap_modal
                .as_ref()
                .unwrap()
                .validation_error
                .is_none()
        );
    }
}
