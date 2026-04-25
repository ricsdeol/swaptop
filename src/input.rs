use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};
use crate::create_swap::{CreateSwapField, CreateSwapMode};
use crate::platform::StepStatus;
use crate::platform::SwapKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateSwapModeKind {
    Form,
    Progress,
    ConfirmActivateOnly,
}

pub struct ConfirmOffDeleteContext {
    pub path: PathBuf,
    pub delete_file: bool,
    pub active: bool,
}

pub struct DeviceContext {
    pub has_devices: bool,
    pub confirm_action: Option<DeviceOpKind>,
    pub selected_path: Option<PathBuf>,
    pub selected_active: Option<bool>,
    pub selected_is_file: Option<bool>,
    pub confirm_off_delete: Option<ConfirmOffDeleteContext>,
}

pub struct CreateSwapContext {
    pub mode: CreateSwapModeKind,
    pub focused_field: Option<CreateSwapField>,
    pub path_value: String,
    pub completions_showing: bool,
    pub has_error_step: bool,
}

pub struct KeyContext {
    pub active_tab: Tab,
    pub filter_mode: bool,
    pub sort_col: SortColumn,
    pub is_root: bool,
    pub device: DeviceContext,
    pub create_swap: Option<CreateSwapContext>,
}

impl KeyContext {
    pub fn from_state(s: &AppState) -> Self {
        let device = DeviceContext {
            has_devices: !s.devices.is_empty(),
            confirm_action: s.confirm_action.clone(),
            selected_path: s.devices.get(s.selected_dev).map(|d| d.path.clone()),
            selected_active: s.devices.get(s.selected_dev).map(|d| d.active),
            selected_is_file: s
                .devices
                .get(s.selected_dev)
                .map(|d| matches!(d.kind, SwapKind::File)),
            confirm_off_delete: s
                .confirm_off_delete
                .as_ref()
                .map(|c| ConfirmOffDeleteContext {
                    path: c.path.clone(),
                    delete_file: c.delete_file,
                    active: c.active,
                }),
        };

        let create_swap = s.create_swap_modal.as_ref().map(|modal| {
            let (mode, focused_field) = match &modal.mode {
                CreateSwapMode::Form { focused_field } => {
                    (CreateSwapModeKind::Form, Some(*focused_field))
                }
                CreateSwapMode::Progress { .. } => (CreateSwapModeKind::Progress, None),
                CreateSwapMode::ConfirmActivateOnly { .. } => {
                    (CreateSwapModeKind::ConfirmActivateOnly, None)
                }
            };
            let has_error_step = match &modal.mode {
                CreateSwapMode::Progress { steps } => steps
                    .iter()
                    .any(|s| matches!(s.status, StepStatus::Error(_))),
                _ => false,
            };
            CreateSwapContext {
                mode,
                focused_field,
                path_value: modal.path_input.value().to_string(),
                completions_showing: !modal.completions.is_empty(),
                has_error_step,
            }
        });

        Self {
            active_tab: s.active_tab.clone(),
            filter_mode: s.filter_mode,
            sort_col: s.sort_col,
            is_root: s.is_root,
            device,
            create_swap,
        }
    }
}

pub fn resolve_key(key: KeyEvent, ctx: &KeyContext) -> Option<Action> {
    if ctx.filter_mode {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::ExitFilterMode),
            KeyCode::Backspace => Some(Action::FilterBackspace),
            KeyCode::Char(c) => Some(Action::FilterChar(c)),
            _ => None,
        };
    }

    if let Some(ref cs) = ctx.create_swap {
        return handle_create_swap_key(key, cs);
    }

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

    match ctx.active_tab {
        Tab::Processes => match key.code {
            KeyCode::Char('j') | KeyCode::Down => return Some(Action::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up => return Some(Action::NavigateUp),
            KeyCode::Char('s') => {
                return Some(Action::SortBy(next_sort_column(&ctx.sort_col)));
            }
            KeyCode::Char('/') => return Some(Action::EnterFilterMode),
            _ => {}
        },
        Tab::Devices => {
            return handle_devices_key(key.code, &ctx.device, ctx.is_root);
        }
        _ => {}
    }

    None
}

fn handle_devices_key(code: KeyCode, dev: &DeviceContext, is_root: bool) -> Option<Action> {
    if let Some(ref off_del) = dev.confirm_off_delete {
        return match code {
            KeyCode::Char(' ') => Some(Action::ToggleConfirmDeleteFile),
            KeyCode::Char('c') | KeyCode::Enter => {
                let kind = match (off_del.delete_file, off_del.active) {
                    (false, false) => return Some(Action::CancelConfirmOffDelete),
                    (false, true) => DeviceOpKind::Off,
                    (true, false) => DeviceOpKind::DeleteOnly,
                    (true, true) => DeviceOpKind::OffAndDelete,
                };
                Some(Action::ExecuteDeviceOp {
                    path: off_del.path.clone(),
                    kind,
                })
            }
            KeyCode::Esc => Some(Action::CancelConfirmOffDelete),
            _ => None,
        };
    }

    if let Some(ref kind) = dev.confirm_action {
        return match code {
            KeyCode::Char('c') | KeyCode::Enter => {
                let path = dev.selected_path.as_ref()?.clone();
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
        KeyCode::Char('r') if dev.has_devices => {
            if is_root {
                if dev.selected_active == Some(true) {
                    Some(Action::RequestConfirm(DeviceOpKind::Reset))
                } else {
                    Some(Action::SetError(
                        "Swap is not active — activate it first".to_string(),
                    ))
                }
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        KeyCode::Char('a') if dev.has_devices => {
            if is_root {
                if dev.selected_active == Some(true) {
                    Some(Action::SetError("Swap is already active".to_string()))
                } else {
                    Some(Action::RequestConfirm(DeviceOpKind::On))
                }
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
        KeyCode::Char('d') if dev.has_devices => {
            if is_root {
                if dev.selected_is_file == Some(true) {
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
            if is_root {
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

fn handle_create_swap_key(key: KeyEvent, cs: &CreateSwapContext) -> Option<Action> {
    match cs.mode {
        CreateSwapModeKind::Form => {
            let focused = cs.focused_field?;

            if cs.completions_showing {
                return match key.code {
                    KeyCode::Down | KeyCode::Tab => Some(Action::CreateSwapCompletionMove(1)),
                    KeyCode::Up => Some(Action::CreateSwapCompletionMove(-1)),
                    KeyCode::Enter => Some(Action::CreateSwapApplyCompletion),
                    KeyCode::Esc => Some(Action::CreateSwapClearCompletions),
                    _ => handle_form_key(key, focused).or(Some(Action::CreateSwapClearCompletions)),
                };
            }

            if key.code == KeyCode::Tab && focused == CreateSwapField::Path {
                let completions = compute_path_completions(&cs.path_value);
                return if completions.is_empty() {
                    None
                } else {
                    Some(Action::CreateSwapSetCompletions(completions))
                };
            }

            handle_form_key(key, focused)
        }
        CreateSwapModeKind::Progress => match key.code {
            KeyCode::Esc => {
                if cs.has_error_step {
                    Some(Action::CreateSwapReturnToForm)
                } else {
                    Some(Action::CloseCreateSwap)
                }
            }
            _ => None,
        },
        CreateSwapModeKind::ConfirmActivateOnly => match key.code {
            KeyCode::Char('c') | KeyCode::Enter => Some(Action::CreateSwapSubmit {
                activate_only: true,
            }),
            KeyCode::Esc => Some(Action::CloseCreateSwap),
            _ => None,
        },
    }
}

fn handle_form_key(key: KeyEvent, focused: CreateSwapField) -> Option<Action> {
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
        KeyCode::Enter if focused == CreateSwapField::Submit => Some(Action::CreateSwapSubmit {
            activate_only: false,
        }),
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
        SortColumn::Cpu => SortColumn::Pid,
        SortColumn::Pid => SortColumn::Name,
        SortColumn::Name => SortColumn::User,
        SortColumn::User => SortColumn::Rss,
        SortColumn::Rss => SortColumn::Swap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[allow(dead_code)]
    fn default_device() -> DeviceContext {
        DeviceContext {
            has_devices: false,
            confirm_action: None,
            selected_path: None,
            selected_active: None,
            selected_is_file: None,
            confirm_off_delete: None,
        }
    }

    fn rk(
        k: KeyEvent,
        tab: Tab,
        confirm: Option<DeviceOpKind>,
        devs: bool,
        filter: bool,
        sort: SortColumn,
    ) -> Option<Action> {
        resolve_key(
            k,
            &KeyContext {
                active_tab: tab,
                filter_mode: filter,
                sort_col: sort,
                is_root: false,
                device: DeviceContext {
                    has_devices: devs,
                    confirm_action: confirm,
                    selected_path: if devs { Some("/dev/sda2".into()) } else { None },
                    selected_active: None,
                    selected_is_file: None,
                    confirm_off_delete: None,
                },
                create_swap: None,
            },
        )
    }

    // ── Sort column ──────────────────────────────────────────────────────

    #[test]
    fn sort_column_cycles_through_all_columns() {
        assert_eq!(next_sort_column(&SortColumn::Swap), SortColumn::Cpu);
        assert_eq!(next_sort_column(&SortColumn::Cpu), SortColumn::Pid);
        assert_eq!(next_sort_column(&SortColumn::Pid), SortColumn::Name);
        assert_eq!(next_sort_column(&SortColumn::Name), SortColumn::User);
        assert_eq!(next_sort_column(&SortColumn::User), SortColumn::Rss);
        assert_eq!(next_sort_column(&SortColumn::Rss), SortColumn::Swap);
    }

    // ── Filter mode ──────────────────────────────────────────────────────

    #[test]
    fn filter_mode_captures_printable_chars() {
        let action = rk(
            key(KeyCode::Char('a')),
            Tab::Processes,
            None,
            false,
            true,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::FilterChar('a'))));
    }

    #[test]
    fn filter_mode_esc_exits() {
        let action = rk(
            key(KeyCode::Esc),
            Tab::Processes,
            None,
            false,
            true,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_enter_exits() {
        let action = rk(
            key(KeyCode::Enter),
            Tab::Processes,
            None,
            false,
            true,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_backspace_deletes() {
        let action = rk(
            key(KeyCode::Backspace),
            Tab::Processes,
            None,
            false,
            true,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::FilterBackspace)));
    }

    // ── Global keys ──────────────────────────────────────────────────────

    #[test]
    fn global_quit_keys_work_from_any_tab() {
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices] {
            let q = rk(
                key(KeyCode::Char('q')),
                tab.clone(),
                None,
                false,
                false,
                SortColumn::Swap,
            );
            assert!(
                matches!(q, Some(Action::Quit)),
                "q should quit from {tab:?}"
            );

            let ctrl_c = rk(ctrl('c'), tab.clone(), None, false, false, SortColumn::Swap);
            assert!(
                matches!(ctrl_c, Some(Action::Quit)),
                "Ctrl+C should quit from {tab:?}"
            );
        }
    }

    #[test]
    fn tab_keys_cycle_correctly() {
        let fwd = rk(
            key(KeyCode::Tab),
            Tab::Overview,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(matches!(fwd, Some(Action::NextTab)));
        let back = rk(
            key(KeyCode::BackTab),
            Tab::Overview,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(matches!(back, Some(Action::PrevTab)));
    }

    #[test]
    fn number_keys_select_tabs() {
        for n in [1_usize, 2, 3] {
            let c = char::from_digit(n as u32, 10).unwrap();
            let action = rk(
                key(KeyCode::Char(c)),
                Tab::Overview,
                None,
                false,
                false,
                SortColumn::Swap,
            );
            assert!(matches!(action, Some(Action::SelectTab(v)) if v == n));
        }
    }

    // ── Tab-specific keys ────────────────────────────────────────────────

    #[test]
    fn process_tab_keys_only_fire_on_process_tab() {
        let on_proc = rk(
            key(KeyCode::Char('j')),
            Tab::Processes,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(matches!(on_proc, Some(Action::NavigateDown)));
        let on_overview = rk(
            key(KeyCode::Char('j')),
            Tab::Overview,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(on_overview.is_none());
    }

    #[test]
    fn slash_enters_filter_mode_on_processes() {
        let action = rk(
            key(KeyCode::Char('/')),
            Tab::Processes,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::EnterFilterMode)));
    }

    // ── ConfirmOffDelete kind selection ──────────────────────────────────

    fn make_off_delete_ctx(active: bool, delete_file: bool) -> KeyContext {
        KeyContext {
            active_tab: Tab::Devices,
            filter_mode: false,
            sort_col: SortColumn::Swap,
            is_root: false,
            device: DeviceContext {
                has_devices: true,
                confirm_action: None,
                selected_path: None,
                selected_active: None,
                selected_is_file: None,
                confirm_off_delete: Some(ConfirmOffDeleteContext {
                    path: "/swapfile".into(),
                    delete_file,
                    active,
                }),
            },
            create_swap: None,
        }
    }

    #[test]
    fn off_delete_inactive_delete_true_dispatches_delete_only() {
        let ctx = make_off_delete_ctx(false, true);
        let action = resolve_key(key(KeyCode::Char('c')), &ctx);
        assert!(
            matches!(action, Some(Action::ExecuteDeviceOp { ref kind, .. }) if *kind == DeviceOpKind::DeleteOnly),
            "expected DeleteOnly, got {action:?}"
        );
    }

    #[test]
    fn off_delete_active_delete_true_dispatches_off_and_delete() {
        let ctx = make_off_delete_ctx(true, true);
        let action = resolve_key(key(KeyCode::Char('c')), &ctx);
        assert!(
            matches!(action, Some(Action::ExecuteDeviceOp { ref kind, .. }) if *kind == DeviceOpKind::OffAndDelete),
            "expected OffAndDelete, got {action:?}"
        );
    }

    #[test]
    fn off_delete_active_delete_false_dispatches_off() {
        let ctx = make_off_delete_ctx(true, false);
        let action = resolve_key(key(KeyCode::Char('c')), &ctx);
        assert!(
            matches!(action, Some(Action::ExecuteDeviceOp { ref kind, .. }) if *kind == DeviceOpKind::Off),
            "expected Off, got {action:?}"
        );
    }

    #[test]
    fn off_delete_inactive_delete_false_cancels() {
        let ctx = make_off_delete_ctx(false, false);
        let action = resolve_key(key(KeyCode::Char('c')), &ctx);
        assert!(
            matches!(action, Some(Action::CancelConfirmOffDelete)),
            "expected CancelConfirmOffDelete, got {action:?}"
        );
    }

    #[test]
    fn unknown_key_returns_none() {
        let action = rk(
            key(KeyCode::F(5)),
            Tab::Overview,
            None,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(action.is_none());
    }

    // ── Path completions ─────────────────────────────────────────────────

    #[test]
    fn completions_for_root_contains_entries() {
        let results = compute_path_completions("/tmp");
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
