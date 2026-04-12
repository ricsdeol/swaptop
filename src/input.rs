use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};

pub fn resolve_key(
    key:            KeyEvent,
    active_tab:     &Tab,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    filter_mode:    bool,
    sort_col:       &SortColumn,
    state:          &Arc<Mutex<AppState>>,
) -> Option<Action> {
    // Priority 1: filter input captures almost all keys
    if filter_mode {
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
                confirm_action,
                selected_dev,
                has_devices,
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
}
