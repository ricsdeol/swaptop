use std::path::PathBuf;

use crate::create_swap::{CreateSwapField, StepStatus};
use crate::platform::MemSnapshot;

// ── Phase 4 types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceOpKind {
    On,
    Off,
    OffAndDelete,
    Reset,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OpStatus {
    Running,
    Done,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct DeviceOp {
    pub path: PathBuf,
    #[allow(dead_code)] // stored for completeness; UI may display this in a future pass
    pub kind: DeviceOpKind,
    pub status: OpStatus,
}

// ── Phase 2 types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortColumn {
    Pid,
    Name,
    User,
    Rss,
    Swap,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortDir {
    Asc,
    Desc,
}

// ── Actions ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Action {
    // Global
    Quit,
    Refresh,
    NextTab,
    PrevTab,
    SelectTab(usize),
    UpdateSnapshot(MemSnapshot),
    SetError(String),

    // Phase 4 — device navigation
    DeviceUp,
    DeviceDown,

    // Phase 4 — operation flow
    RequestConfirm(DeviceOpKind),
    CancelConfirm,
    ExecuteDeviceOp { path: PathBuf, kind: DeviceOpKind },
    DeviceOpUpdate(DeviceOp),

    // Phase 2 — processes navigation
    NavigateUp,
    NavigateDown,
    SortBy(SortColumn),
    EnterFilterMode,
    FilterChar(char),
    FilterBackspace,
    ExitFilterMode,

    // Phase 5 — create swap modal
    OpenCreateSwap,
    CloseCreateSwap,
    CreateSwapReturnToForm,
    CreateSwapFocusField(CreateSwapField),
    CreateSwapInputEvent(crossterm::event::Event),
    CreateSwapToggleUnit,
    CreateSwapToggleActivate,
    CreateSwapSubmit { activate_only: bool },
    OpenConfirmActivateOnly { path: PathBuf, size_bytes: u64 },
    CreateSwapStepUpdate { index: usize, status: StepStatus },

    // Phase 6 — path autocomplete
    CreateSwapSetCompletions(Vec<String>),
    CreateSwapCompletionMove(i16),
    CreateSwapApplyCompletion,
    CreateSwapClearCompletions,

    // Phase 6 — delete file on swapoff
    RequestConfirmOffDelete,
    ToggleConfirmDeleteFile,
    CancelConfirmOffDelete,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn device_op_kind_is_clone_and_partialeq() {
        let a = DeviceOpKind::On;
        assert_eq!(a.clone(), DeviceOpKind::On);
        assert_ne!(a, DeviceOpKind::Off);
    }

    #[test]
    fn op_status_error_holds_message() {
        let s = OpStatus::Error("swapon failed: EPERM".to_string());
        assert!(matches!(s, OpStatus::Error(ref m) if m == "swapon failed: EPERM"));
    }

    #[test]
    fn device_op_fields_are_accessible() {
        let op = DeviceOp {
            path: PathBuf::from("/dev/sda2"),
            kind: DeviceOpKind::Off,
            status: OpStatus::Running,
        };
        assert_eq!(op.path, PathBuf::from("/dev/sda2"));
        assert_eq!(op.kind, DeviceOpKind::Off);
        assert_eq!(op.status, OpStatus::Running);
    }

    #[test]
    fn open_create_swap_is_constructible() {
        let a = Action::OpenCreateSwap;
        assert!(matches!(a, Action::OpenCreateSwap));
    }

    #[test]
    fn create_swap_submit_carries_activate_only_flag() {
        let a = Action::CreateSwapSubmit {
            activate_only: true,
        };
        match a {
            Action::CreateSwapSubmit { activate_only } => assert!(activate_only),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn create_swap_step_update_carries_index_and_status() {
        use crate::create_swap::StepStatus;
        let a = Action::CreateSwapStepUpdate {
            index: 3,
            status: StepStatus::Done,
        };
        match a {
            Action::CreateSwapStepUpdate { index, status } => {
                assert_eq!(index, 3);
                assert_eq!(status, StepStatus::Done);
            }
            _ => panic!("wrong variant"),
        }
    }
}
