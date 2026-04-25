use std::path::PathBuf;

use crate::create_swap::CreateSwapField;
use crate::platform::{CreateSwapProgress, MemSnapshot};

#[derive(Debug, Clone, PartialEq)]
pub enum DeviceOpKind {
    On,
    Off,
    OffAndDelete,
    DeleteOnly,
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
    pub kind: DeviceOpKind,
    pub status: OpStatus,
}

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
    NextTab,
    PrevTab,
    SelectTab(usize),
    UpdateSnapshot(MemSnapshot),
    SetError(String),
    CollectStarted,
    CollectFinished,
    DeviceUp,
    DeviceDown,
    RequestConfirm(DeviceOpKind),
    CancelConfirm,
    ExecuteDeviceOp { path: PathBuf, kind: DeviceOpKind },
    DeviceOpUpdate(DeviceOp),
    NavigateUp,
    NavigateDown,
    SortBy(SortColumn),
    EnterFilterMode,
    FilterChar(char),
    FilterBackspace,
    ExitFilterMode,
    OpenCreateSwap,
    CloseCreateSwap,
    CreateSwapReturnToForm,
    CreateSwapFocusField(CreateSwapField),
    CreateSwapInputEvent(crossterm::event::Event),
    CreateSwapToggleUnit,
    CreateSwapToggleActivate,
    CreateSwapSubmit { activate_only: bool },
    CreateSwapProgress(CreateSwapProgress),
    CreateSwapSetCompletions(Vec<String>),
    CreateSwapCompletionMove(i16),
    CreateSwapApplyCompletion,
    CreateSwapClearCompletions,
    RequestConfirmOffDelete,
    ToggleConfirmDeleteFile,
    CancelConfirmOffDelete,
    #[allow(dead_code)] // wired in Task 8
    OpenProcessDetail { pid: u32 },
    #[allow(dead_code)] // wired in Task 8
    CloseProcessDetail,
    #[allow(dead_code)] // wired in Task 8
    ConfirmKillProcess { pid: u32 },
    #[allow(dead_code)] // intercepted by main.rs, never reaches reducer
    KillProcess { pid: u32 },
    #[allow(dead_code)] // wired in Task 8
    KillProcessResult { pid: u32, success: bool, msg: Option<String> },
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
    fn create_swap_progress_carries_step_update() {
        use crate::platform::StepStatus;
        let a = Action::CreateSwapProgress(CreateSwapProgress::StepUpdate {
            index: 3,
            status: StepStatus::Done,
        });
        match a {
            Action::CreateSwapProgress(CreateSwapProgress::StepUpdate { index, status }) => {
                assert_eq!(index, 3);
                assert_eq!(status, StepStatus::Done);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn open_process_detail_carries_pid() {
        let a = Action::OpenProcessDetail { pid: 42 };
        assert!(matches!(a, Action::OpenProcessDetail { pid: 42 }));
    }

    #[test]
    fn close_process_detail_is_constructible() {
        assert!(matches!(Action::CloseProcessDetail, Action::CloseProcessDetail));
    }

    #[test]
    fn confirm_kill_process_carries_pid() {
        let a = Action::ConfirmKillProcess { pid: 99 };
        assert!(matches!(a, Action::ConfirmKillProcess { pid: 99 }));
    }

    #[test]
    fn kill_process_result_has_fields() {
        let a = Action::KillProcessResult { pid: 1, success: false, msg: Some("err".into()) };
        match a {
            Action::KillProcessResult { pid, success, msg } => {
                assert_eq!(pid, 1);
                assert!(!success);
                assert_eq!(msg, Some("err".into()));
            }
            _ => panic!("wrong variant"),
        }
    }
}
