use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};
use crate::create_swap::CreateSwapMode;

/// Context passed to [`resolve_key`] to avoid an 8-argument signature.
pub struct KeyContext<'a> {
    pub active_tab: &'a Tab,
    pub confirm_action: Option<&'a DeviceOpKind>,
    pub selected_dev: usize,
    pub has_devices: bool,
    pub filter_mode: bool,
    pub sort_col: &'a SortColumn,
    pub state: &'a Arc<Mutex<AppState>>,
}

pub fn resolve_key(key: KeyEvent, ctx: &KeyContext<'_>) -> Option<Action> {
    let KeyContext {
        active_tab,
        confirm_action,
        selected_dev,
        has_devices,
        filter_mode,
        sort_col,
        state,
    } = ctx;
    // Priority 1: filter input captures almost all keys
    if *filter_mode {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::ExitFilterMode),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Char(c) => Some(Action::FilterChar(c)),
            _ => None,
        };
    }

    // Priority 1.5: create-swap modal intercepts keys when open.
    let modal_open = {
        let s = state.lock().expect("state mutex poisoned");
        s.create_swap_modal.is_some()
    };
    if modal_open {
        return handle_create_swap_key(key, state);
    }

    // Global keys (always active)
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return Some(Action::Quit);
        }
        KeyCode::Tab => return Some(Action::NextTab),
        KeyCode::BackTab => return Some(Action::PrevTab),
        KeyCode::Char('1') => return Some(Action::SelectTab(1)),
        KeyCode::Char('2') => return Some(Action::SelectTab(2)),
        KeyCode::Char('3') => return Some(Action::SelectTab(3)),
        _ => {}
    }

    // Tab-specific keys
    match active_tab {
        Tab::Processes => match key.code {
            KeyCode::Char('j') | KeyCode::Down => return Some(Action::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => return Some(Action::NavigateUp),
            KeyCode::Char('s') => {
                return Some(Action::SortBy(next_sort_column(sort_col)));
            }
            KeyCode::Char('/') => return Some(Action::EnterFilterMode),
            KeyCode::Char('r') => return Some(Action::Refresh),
            _ => {}
        },
        Tab::Devices => {
            return handle_devices_key(
                key.code,
                *confirm_action,
                *selected_dev,
                *has_devices,
                state,
            );
        }
        _ => {
            if let KeyCode::Char('r') = key.code {
                return Some(Action::Refresh);
            }
        }
    }

    None
}

fn handle_devices_key(
    code: KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev: usize,
    has_devices: bool,
    state: &Arc<Mutex<AppState>>,
) -> Option<Action> {
    // Off-delete modal takes priority when open
    {
        let off_delete_open = {
            let s = state.lock().expect("state mutex poisoned");
            s.confirm_off_delete.is_some()
        };
        if off_delete_open {
            return match code {
                KeyCode::Char(' ') => Some(Action::ToggleConfirmDeleteFile),
                KeyCode::Char('s') | KeyCode::Enter => {
                    let (path, delete_file) = {
                        let s = state.lock().expect("state mutex poisoned");
                        let modal = s.confirm_off_delete.as_ref()?;
                        (modal.path.clone(), modal.delete_file)
                    };
                    let kind = if delete_file {
                        DeviceOpKind::OffAndDelete
                    } else {
                        DeviceOpKind::Off
                    };
                    Some(Action::ExecuteDeviceOp { path, kind })
                }
                KeyCode::Esc => Some(Action::CancelConfirmOffDelete),
                _ => None,
            };
        }
    }

    if let Some(kind) = confirm_action {
        // Modal is open — only 's'/Enter and Esc are active
        return match code {
            KeyCode::Char('s') | KeyCode::Enter => {
                let path = state
                    .lock()
                    .expect("state mutex poisoned")
                    .devices
                    .get(selected_dev)?
                    .path
                    .clone();
                Some(Action::ExecuteDeviceOp {
                    path,
                    kind: kind.clone(),
                })
            }
            KeyCode::Esc => Some(Action::CancelConfirm),
            _ => None,
        };
    }

    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::DeviceDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::DeviceUp),
        KeyCode::Char('r') if has_devices => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Reset))
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        KeyCode::Char('o') if has_devices => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::On))
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        KeyCode::Char('f') if has_devices => {
            if nix::unistd::geteuid().is_root() {
                let is_file_type = {
                    let s = state.lock().expect("state mutex poisoned");
                    s.devices
                        .get(selected_dev)
                        .map(|d| matches!(d.kind, crate::platform::SwapKind::File))
                        .unwrap_or(false)
                };
                if is_file_type {
                    Some(Action::RequestConfirmOffDelete)
                } else {
                    Some(Action::RequestConfirm(DeviceOpKind::Off))
                }
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        KeyCode::Char('n') => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::OpenCreateSwap)
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        _ => None,
    }
}

fn handle_create_swap_key(key: KeyEvent, state: &Arc<Mutex<AppState>>) -> Option<Action> {
    use crate::create_swap::CreateSwapField;

    let (
        mode_variant,
        focused_field,
        path_value,
        size_value,
        priority_value,
        size_unit,
        completions_showing,
    ) = {
        let s = state.lock().expect("state mutex poisoned");
        let modal = s.create_swap_modal.as_ref()?;
        let focused = match &modal.mode {
            CreateSwapMode::Form { focused_field } => Some(*focused_field),
            _ => None,
        };
        let mode_variant = match &modal.mode {
            CreateSwapMode::Form { .. } => "form",
            CreateSwapMode::Progress { .. } => "progress",
            CreateSwapMode::ConfirmActivateOnly { .. } => "confirm_activate",
        };
        (
            mode_variant,
            focused,
            modal.path_input.value().to_string(),
            modal.size_input.value().to_string(),
            modal.priority_input.value().to_string(),
            modal.size_unit,
            !modal.completions.is_empty(),
        )
    };

    match mode_variant {
        "form" => {
            let focused = focused_field?;

            // Completions popup is showing — intercept navigation keys
            if completions_showing {
                return match key.code {
                    KeyCode::Down | KeyCode::Tab => Some(Action::CreateSwapCompletionMove(1)),
                    KeyCode::Up => Some(Action::CreateSwapCompletionMove(-1)),
                    KeyCode::Enter => Some(Action::CreateSwapApplyCompletion),
                    KeyCode::Esc => Some(Action::CreateSwapClearCompletions),
                    _ => {
                        // Any other key: clear completions inline, then forward to normal handler
                        {
                            let mut s = state.lock().expect("state mutex poisoned");
                            if let Some(m) = s.create_swap_modal.as_mut() {
                                m.completions.clear();
                                m.completion_sel = None;
                            }
                        }
                        handle_form_key(
                            key,
                            focused,
                            state,
                            &path_value,
                            &size_value,
                            &priority_value,
                            size_unit,
                        )
                    }
                };
            }

            // Tab on Path field triggers completion
            if key.code == KeyCode::Tab && focused == CreateSwapField::Path {
                let completions = compute_path_completions(&path_value);
                return if completions.is_empty() {
                    None
                } else {
                    Some(Action::CreateSwapSetCompletions(completions))
                };
            }

            handle_form_key(
                key,
                focused,
                state,
                &path_value,
                &size_value,
                &priority_value,
                size_unit,
            )
        }
        "progress" => match key.code {
            // TODO: disallow Esc once step 3 (file allocation) has started, to avoid
            // leaving a partial pre-allocated file behind when the user cancels.
            // Today, the background task keeps running after CloseCreateSwap and its
            // StepUpdate actions are silently dropped, so the user has no indication
            // that a partial file exists on disk.
            KeyCode::Esc => {
                let return_to_form = {
                    let s = state.lock().expect("state mutex poisoned");
                    if let Some(modal) = s.create_swap_modal.as_ref() {
                        if let CreateSwapMode::Progress { steps } = &modal.mode {
                            steps.iter().any(|s| {
                                matches!(s.status, crate::create_swap::StepStatus::Error(_))
                            })
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };
                if return_to_form {
                    Some(Action::CreateSwapReturnToForm)
                } else {
                    Some(Action::CloseCreateSwap)
                }
            }
            _ => None,
        },
        "confirm_activate" => match key.code {
            KeyCode::Char('s') | KeyCode::Enter => Some(Action::CreateSwapSubmit {
                activate_only: true,
            }),
            KeyCode::Esc => Some(Action::CloseCreateSwap),
            _ => None,
        },
        _ => None,
    }
}

fn handle_form_key(
    key: KeyEvent,
    focused: crate::create_swap::CreateSwapField,
    state: &Arc<Mutex<AppState>>,
    path_value: &str,
    size_value: &str,
    priority_value: &str,
    size_unit: crate::create_swap::SizeUnit,
) -> Option<Action> {
    use crate::create_swap::CreateSwapField;
    match key.code {
        KeyCode::Esc => Some(Action::CloseCreateSwap),
        KeyCode::Up | KeyCode::Char('k')
            if matches!(
                focused,
                CreateSwapField::SizeUnit
                    | CreateSwapField::ActivateAfter
                    | CreateSwapField::Submit
            ) =>
        {
            Some(Action::CreateSwapFocusField(focused.prev()))
        }
        KeyCode::Down | KeyCode::Char('j')
            if matches!(
                focused,
                CreateSwapField::SizeUnit
                    | CreateSwapField::ActivateAfter
                    | CreateSwapField::Submit
            ) =>
        {
            Some(Action::CreateSwapFocusField(focused.next()))
        }
        KeyCode::Up => Some(Action::CreateSwapFocusField(focused.prev())),
        KeyCode::Down => Some(Action::CreateSwapFocusField(focused.next())),
        KeyCode::Char(' ') if focused == CreateSwapField::SizeUnit => {
            Some(Action::CreateSwapToggleUnit)
        }
        KeyCode::Char(' ') if focused == CreateSwapField::ActivateAfter => {
            Some(Action::CreateSwapToggleActivate)
        }
        KeyCode::Enter if focused == CreateSwapField::Submit => {
            validate_and_submit(state, path_value, size_value, priority_value, size_unit)
        }
        _ => {
            if matches!(
                focused,
                CreateSwapField::Path | CreateSwapField::Size | CreateSwapField::Priority
            ) {
                Some(Action::CreateSwapInputEvent(crossterm::event::Event::Key(
                    key,
                )))
            } else {
                None
            }
        }
    }
}

fn validate_and_submit(
    state: &Arc<Mutex<AppState>>,
    path: &str,
    size: &str,
    priority: &str,
    _unit: crate::create_swap::SizeUnit,
) -> Option<Action> {
    let err = |msg: &str| -> Option<Action> {
        let mut s = state.lock().expect("state mutex poisoned");
        if let Some(m) = s.create_swap_modal.as_mut() {
            m.validation_error = Some(msg.to_string());
        }
        None
    };

    if path.trim().is_empty() {
        return err("Path is required");
    }
    if !std::path::Path::new(path).is_absolute() {
        return err("Path must be absolute");
    }
    let size_n: u64 = match size.trim().parse() {
        Ok(n) => n,
        Err(_) => return err("Size must be a positive integer"),
    };
    if size_n == 0 {
        return err("Size must be greater than zero");
    }
    let prio_n: i32 = match priority.trim().parse() {
        Ok(n) => n,
        Err(_) => return err("Priority must be an integer between -1 and 32767"),
    };
    if !(-1..=32767).contains(&prio_n) {
        return err("Priority must be an integer between -1 and 32767");
    }

    {
        let mut s = state.lock().expect("state mutex poisoned");
        if let Some(m) = s.create_swap_modal.as_mut() {
            m.validation_error = None;
        }
    }
    Some(Action::CreateSwapSubmit {
        activate_only: false,
    })
}

fn compute_path_completions(partial: &str) -> Vec<String> {
    // TODO: this runs std::fs::read_dir on the Tokio executor thread. Local
    // filesystems are sub-millisecond, but network mounts (NFS/CIFS on /home)
    // can block the event loop for hundreds of milliseconds. Move to
    // spawn_blocking if slow filesystems are reported.
    let path = std::path::Path::new(partial);
    let (dir, prefix) = if partial.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or(std::path::Path::new("/")),
            path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        )
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut results: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with(prefix))
                .unwrap_or(false)
        })
        .map(|e| {
            let mut p = e.path().to_string_lossy().to_string();
            if e.path().is_dir() {
                p.push('/');
            }
            p
        })
        .collect();
    results.sort();
    results
}

pub fn next_sort_column(current: &SortColumn) -> SortColumn {
    match current {
        SortColumn::Swap => SortColumn::Cpu,
        SortColumn::Cpu => SortColumn::Rss,
        SortColumn::Rss => SortColumn::Pid,
        SortColumn::Pid => SortColumn::Name,
        SortColumn::Name | SortColumn::User => SortColumn::Swap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_column_cycles_through_all_columns() {
        assert_eq!(next_sort_column(&SortColumn::Swap), SortColumn::Cpu);
        assert_eq!(next_sort_column(&SortColumn::Cpu), SortColumn::Rss);
        assert_eq!(next_sort_column(&SortColumn::Rss), SortColumn::Pid);
        assert_eq!(next_sort_column(&SortColumn::Pid), SortColumn::Name);
        assert_eq!(next_sort_column(&SortColumn::Name), SortColumn::Swap);
    }

    #[test]
    fn user_column_falls_back_to_swap() {
        assert_eq!(next_sort_column(&SortColumn::User), SortColumn::Swap);
    }

    use crate::actions::SortColumn;
    use crate::app::{AppState, Tab};
    use crate::platform::Capabilities;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::{Arc, Mutex};

    fn make_caps() -> Capabilities {
        Capabilities {
            can_swap_on: true,
            has_per_process: true,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::new(make_caps())))
    }

    /// Convenience wrapper for tests: mirrors the old 8-arg signature.
    #[allow(clippy::too_many_arguments)]
    fn rk(
        k: KeyEvent,
        tab: &Tab,
        confirm: Option<&DeviceOpKind>,
        sel: usize,
        devs: bool,
        filter: bool,
        sort: &SortColumn,
        state: &Arc<Mutex<AppState>>,
    ) -> Option<Action> {
        resolve_key(
            k,
            &KeyContext {
                active_tab: tab,
                confirm_action: confirm,
                selected_dev: sel,
                has_devices: devs,
                filter_mode: filter,
                sort_col: sort,
                state,
            },
        )
    }

    // ── Filter mode ──────────────────────────────────────────────────────

    #[test]
    fn filter_mode_captures_printable_chars() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Char('a')),
            &Tab::Processes,
            None,
            0,
            false,
            true,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::FilterChar('a'))));
    }

    #[test]
    fn filter_mode_esc_exits() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Esc),
            &Tab::Processes,
            None,
            0,
            false,
            true,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_enter_exits() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Enter),
            &Tab::Processes,
            None,
            0,
            false,
            true,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_backspace_deletes() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Backspace),
            &Tab::Processes,
            None,
            0,
            false,
            true,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::FilterBackspace)));
    }

    // ── Global keys ──────────────────────────────────────────────────────

    #[test]
    fn global_quit_keys_work_from_any_tab() {
        let state = make_state();
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices] {
            let q = rk(
                key(KeyCode::Char('q')),
                &tab,
                None,
                0,
                false,
                false,
                &SortColumn::Swap,
                &state,
            );
            assert!(
                matches!(q, Some(Action::Quit)),
                "q should quit from {tab:?}"
            );

            let ctrl_c = rk(
                ctrl('c'),
                &tab,
                None,
                0,
                false,
                false,
                &SortColumn::Swap,
                &state,
            );
            assert!(
                matches!(ctrl_c, Some(Action::Quit)),
                "Ctrl+C should quit from {tab:?}"
            );
        }
    }

    #[test]
    fn tab_keys_cycle_correctly() {
        let state = make_state();
        let fwd = rk(
            key(KeyCode::Tab),
            &Tab::Overview,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(fwd, Some(Action::NextTab)));

        let back = rk(
            key(KeyCode::BackTab),
            &Tab::Overview,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(back, Some(Action::PrevTab)));
    }

    #[test]
    fn number_keys_select_tabs() {
        let state = make_state();
        for n in [1_usize, 2, 3] {
            let c = char::from_digit(n as u32, 10).unwrap();
            let action = rk(
                key(KeyCode::Char(c)),
                &Tab::Overview,
                None,
                0,
                false,
                false,
                &SortColumn::Swap,
                &state,
            );
            assert!(matches!(action, Some(Action::SelectTab(v)) if v == n));
        }
    }

    // ── Tab-specific keys ────────────────────────────────────────────────

    #[test]
    fn process_tab_keys_only_fire_on_process_tab() {
        let state = make_state();
        let on_proc = rk(
            key(KeyCode::Char('j')),
            &Tab::Processes,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(on_proc, Some(Action::NavigateDown)));

        let on_overview = rk(
            key(KeyCode::Char('j')),
            &Tab::Overview,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(on_overview.is_none());
    }

    #[test]
    fn slash_enters_filter_mode_on_processes() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Char('/')),
            &Tab::Processes,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::EnterFilterMode)));
    }

    #[test]
    fn refresh_key_works_on_overview_tab() {
        let state = make_state();
        let action = rk(
            key(KeyCode::Char('r')),
            &Tab::Overview,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(matches!(action, Some(Action::Refresh)));
    }

    #[test]
    fn unknown_key_returns_none() {
        let state = make_state();
        let action = rk(
            key(KeyCode::F(5)),
            &Tab::Overview,
            None,
            0,
            false,
            false,
            &SortColumn::Swap,
            &state,
        );
        assert!(action.is_none());
    }

    #[test]
    fn completions_for_root_contains_entries() {
        // /tmp always exists on Linux
        let results = compute_path_completions("/tmp");
        // Should find entries starting with /tmp
        assert!(results.iter().all(|p| p.starts_with("/tmp")));
    }

    #[test]
    fn completions_for_nonexistent_dir_returns_empty() {
        let results = compute_path_completions("/definitely_not_a_real_path_xyz/");
        assert!(results.is_empty());
    }

    #[test]
    fn completions_for_slash_returns_root_entries() {
        let results = compute_path_completions("/");
        assert!(!results.is_empty());
        assert!(results.iter().all(|p| p.starts_with('/')));
    }

    #[test]
    fn completions_dirs_end_with_slash() {
        let results = compute_path_completions("/");
        let dirs: Vec<_> = results.iter().filter(|p| p.ends_with('/')).collect();
        assert!(
            !dirs.is_empty(),
            "root should contain at least one directory"
        );
    }
}
