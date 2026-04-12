use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};

/// Context passed to [`resolve_key`] to avoid an 8-argument signature.
pub struct KeyContext<'a> {
    pub active_tab:     &'a Tab,
    pub confirm_action: Option<&'a DeviceOpKind>,
    pub selected_dev:   usize,
    pub has_devices:    bool,
    pub filter_mode:    bool,
    pub sort_col:       &'a SortColumn,
    pub state:          &'a Arc<Mutex<AppState>>,
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
            KeyCode::Backspace            => Some(Action::FilterBackspace),
            KeyCode::Char(c)              => Some(Action::FilterChar(c)),
            _                             => None,
        };
    }

    // Global keys (always active)
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Action::Quit),
        KeyCode::Tab     => return Some(Action::NextTab),
        KeyCode::BackTab => return Some(Action::PrevTab),
        KeyCode::Char('1') => return Some(Action::SelectTab(1)),
        KeyCode::Char('2') => return Some(Action::SelectTab(2)),
        KeyCode::Char('3') => return Some(Action::SelectTab(3)),
        KeyCode::Char('4') => return Some(Action::SelectTab(4)),
        _ => {}
    }

    // Tab-specific keys
    match active_tab {
        Tab::Processes => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down  => return Some(Action::NavigateDown),
                KeyCode::Char('k') | KeyCode::Up    => return Some(Action::NavigateUp),
                KeyCode::Char('s')                  => {
                    return Some(Action::SortBy(next_sort_column(sort_col)));
                }
                KeyCode::Char('/')                  => return Some(Action::EnterFilterMode),
                KeyCode::Char('r')                  => return Some(Action::Refresh),
                _ => {}
            }
        }
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
    code:           KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    state:          &Arc<Mutex<AppState>>,
) -> Option<Action> {
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
                Some(Action::ExecuteDeviceOp { path, kind: kind.clone() })
            }
            KeyCode::Esc => Some(Action::CancelConfirm),
            _ => None,
        };
    }

    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::DeviceDown),
        KeyCode::Char('k') | KeyCode::Up   => Some(Action::DeviceUp),
        KeyCode::Char('r') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Reset))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('o') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::On))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('f') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Off))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        _ => None,
    }
}

pub fn next_sort_column(current: &SortColumn) -> SortColumn {
    match current {
        SortColumn::Swap => SortColumn::Cpu,
        SortColumn::Cpu  => SortColumn::Rss,
        SortColumn::Rss  => SortColumn::Pid,
        SortColumn::Pid  => SortColumn::Name,
        SortColumn::Name | SortColumn::User => SortColumn::Swap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_column_cycles_through_all_columns() {
        assert_eq!(next_sort_column(&SortColumn::Swap), SortColumn::Cpu);
        assert_eq!(next_sort_column(&SortColumn::Cpu),  SortColumn::Rss);
        assert_eq!(next_sort_column(&SortColumn::Rss),  SortColumn::Pid);
        assert_eq!(next_sort_column(&SortColumn::Pid),  SortColumn::Name);
        assert_eq!(next_sort_column(&SortColumn::Name), SortColumn::Swap);
    }

    #[test]
    fn user_column_falls_back_to_swap() {
        assert_eq!(next_sort_column(&SortColumn::User), SortColumn::Swap);
    }

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::{Arc, Mutex};
    use crate::app::{AppState, Tab};
    use crate::actions::SortColumn;
    use crate::platform::Capabilities;

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
        k:       KeyEvent,
        tab:     &Tab,
        confirm: Option<&DeviceOpKind>,
        sel:     usize,
        devs:    bool,
        filter:  bool,
        sort:    &SortColumn,
        state:   &Arc<Mutex<AppState>>,
    ) -> Option<Action> {
        resolve_key(k, &KeyContext {
            active_tab:     tab,
            confirm_action: confirm,
            selected_dev:   sel,
            has_devices:    devs,
            filter_mode:    filter,
            sort_col:       sort,
            state,
        })
    }

    // ── Filter mode ──────────────────────────────────────────────────────

    #[test]
    fn filter_mode_captures_printable_chars() {
        let state = make_state();
        let action = rk(key(KeyCode::Char('a')), &Tab::Processes, None, 0, false, true, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::FilterChar('a'))));
    }

    #[test]
    fn filter_mode_esc_exits() {
        let state = make_state();
        let action = rk(key(KeyCode::Esc), &Tab::Processes, None, 0, false, true, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_enter_exits() {
        let state = make_state();
        let action = rk(key(KeyCode::Enter), &Tab::Processes, None, 0, false, true, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_backspace_deletes() {
        let state = make_state();
        let action = rk(key(KeyCode::Backspace), &Tab::Processes, None, 0, false, true, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::FilterBackspace)));
    }

    // ── Global keys ──────────────────────────────────────────────────────

    #[test]
    fn global_quit_keys_work_from_any_tab() {
        let state = make_state();
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices, Tab::CreateSwap] {
            let q = rk(key(KeyCode::Char('q')), &tab, None, 0, false, false, &SortColumn::Swap, &state);
            assert!(matches!(q, Some(Action::Quit)), "q should quit from {tab:?}");

            let ctrl_c = rk(ctrl('c'), &tab, None, 0, false, false, &SortColumn::Swap, &state);
            assert!(matches!(ctrl_c, Some(Action::Quit)), "Ctrl+C should quit from {tab:?}");
        }
    }

    #[test]
    fn tab_keys_cycle_correctly() {
        let state = make_state();
        let fwd = rk(key(KeyCode::Tab), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(matches!(fwd, Some(Action::NextTab)));

        let back = rk(key(KeyCode::BackTab), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(matches!(back, Some(Action::PrevTab)));
    }

    #[test]
    fn number_keys_select_tabs() {
        let state = make_state();
        for n in [1_usize, 2, 3, 4] {
            let c = char::from_digit(n as u32, 10).unwrap();
            let action = rk(key(KeyCode::Char(c)), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
            assert!(matches!(action, Some(Action::SelectTab(v)) if v == n));
        }
    }

    // ── Tab-specific keys ────────────────────────────────────────────────

    #[test]
    fn process_tab_keys_only_fire_on_process_tab() {
        let state = make_state();
        let on_proc = rk(key(KeyCode::Char('j')), &Tab::Processes, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(matches!(on_proc, Some(Action::NavigateDown)));

        let on_overview = rk(key(KeyCode::Char('j')), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(on_overview.is_none());
    }

    #[test]
    fn slash_enters_filter_mode_on_processes() {
        let state = make_state();
        let action = rk(key(KeyCode::Char('/')), &Tab::Processes, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::EnterFilterMode)));
    }

    #[test]
    fn refresh_key_works_on_overview_tab() {
        let state = make_state();
        let action = rk(key(KeyCode::Char('r')), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(matches!(action, Some(Action::Refresh)));
    }

    #[test]
    fn unknown_key_returns_none() {
        let state = make_state();
        let action = rk(key(KeyCode::F(5)), &Tab::Overview, None, 0, false, false, &SortColumn::Swap, &state);
        assert!(action.is_none());
    }
}
