use std::time::Instant;

use crate::actions::{DeviceOp, DeviceOpKind, OpStatus};
use crate::app::{AppState, ConfirmOffDelete};

impl AppState {
    // ── Callback para snapshot.rs ─────────────────────────────────────────────

    pub(crate) fn on_devices_updated(&mut self) {
        if !self.devices.is_empty() {
            self.selected_dev = self.selected_dev.min(self.devices.len() - 1);
        }
    }

    // ── Handlers ─────────────────────────────────────────────────────────────

    pub(crate) fn handle_device_up(&mut self) {
        self.selected_dev = self.selected_dev.saturating_sub(1);
    }

    pub(crate) fn handle_device_down(&mut self) {
        if !self.devices.is_empty() {
            self.selected_dev = (self.selected_dev + 1).min(self.devices.len() - 1);
        }
    }

    pub(crate) fn handle_request_confirm(&mut self, kind: DeviceOpKind) {
        self.confirm_action = Some(kind);
    }

    pub(crate) fn handle_cancel_confirm(&mut self) {
        self.confirm_action = None;
    }

    pub(crate) fn handle_execute_device_op(
        &mut self,
        path: std::path::PathBuf,
        kind: DeviceOpKind,
    ) {
        self.confirm_action = None;
        self.confirm_off_delete = None;
        self.device_op_started = Some(Instant::now());
        self.device_op = Some(DeviceOp {
            path,
            kind,
            status: OpStatus::Running,
        });
    }

    pub(crate) fn handle_device_op_update(&mut self, op: DeviceOp) {
        if let OpStatus::Error(ref msg) = op.status {
            self.error_msg = Some((msg.clone(), Instant::now()));
        }
        self.device_op = Some(op);
    }

    pub(crate) fn handle_request_confirm_off_delete(&mut self) {
        if let Some(dev) = self.devices.get(self.selected_dev) {
            self.confirm_off_delete = Some(ConfirmOffDelete {
                path: dev.path.clone(),
                delete_file: false,
                active: dev.active,
            });
        }
    }

    pub(crate) fn handle_toggle_confirm_delete_file(&mut self) {
        if let Some(ref mut modal) = self.confirm_off_delete {
            modal.delete_file = !modal.delete_file;
        }
    }

    pub(crate) fn handle_cancel_confirm_off_delete(&mut self) {
        self.confirm_off_delete = None;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;
    use crate::app::test_helpers::*;
    use crate::platform::{SwapDevice, SwapKind};
    use std::path::PathBuf;

    #[test]
    fn device_up_decrements_selected_dev_with_floor_at_zero() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.selected_dev = 1;
        state.handle_action(Action::DeviceUp);
        assert_eq!(state.selected_dev, 0);
        state.handle_action(Action::DeviceUp); // already at 0
        assert_eq!(state.selected_dev, 0);
    }

    #[test]
    fn device_down_increments_selected_dev_capped_at_last() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![make_device("/dev/sda2"), make_device("/swapfile")];
        state.handle_action(Action::DeviceDown);
        assert_eq!(state.selected_dev, 1);
        state.handle_action(Action::DeviceDown); // already at last
        assert_eq!(state.selected_dev, 1);
    }

    #[test]
    fn request_confirm_sets_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::RequestConfirm(DeviceOpKind::Off));
        assert_eq!(state.confirm_action, Some(DeviceOpKind::Off));
    }

    #[test]
    fn cancel_confirm_clears_confirm_action() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::On);
        state.handle_action(Action::CancelConfirm);
        assert!(state.confirm_action.is_none());
    }

    #[test]
    fn execute_device_op_sets_running_and_clears_confirm() {
        let mut state = AppState::new(make_caps());
        state.confirm_action = Some(DeviceOpKind::Off);
        state.handle_action(Action::ExecuteDeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
        });
        assert!(state.confirm_action.is_none());
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Running);
        assert_eq!(op.path, PathBuf::from("/dev/sda2"));
    }

    #[test]
    fn device_op_update_replaces_device_op() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
            status: OpStatus::Done,
        }));
        let op = state.device_op.as_ref().unwrap();
        assert_eq!(op.status, OpStatus::Done);
    }

    #[test]
    fn device_op_update_with_error_sets_error_msg() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::DeviceOpUpdate(DeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::Off,
            status: OpStatus::Error("swapoff failed: EPERM".to_string()),
        }));
        assert!(matches!(&state.error_msg, Some((msg, _)) if msg == "swapoff failed: EPERM"));
        assert!(state.device_op.is_some());
    }

    #[test]
    fn request_confirm_off_delete_opens_modal() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        let modal = state.confirm_off_delete.as_ref().unwrap();
        assert_eq!(modal.path, PathBuf::from("/swapfile"));
        assert!(!modal.delete_file);
        assert!(modal.active);
    }

    #[test]
    fn toggle_confirm_delete_file_flips() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        assert!(!state.confirm_off_delete.as_ref().unwrap().delete_file);
        state.handle_action(Action::ToggleConfirmDeleteFile);
        assert!(state.confirm_off_delete.as_ref().unwrap().delete_file);
        state.handle_action(Action::ToggleConfirmDeleteFile);
        assert!(!state.confirm_off_delete.as_ref().unwrap().delete_file);
    }

    #[test]
    fn cancel_confirm_off_delete_clears_modal() {
        let mut state = AppState::new(make_caps());
        state.devices = vec![SwapDevice {
            path: "/swapfile".into(),
            total: 1024,
            used: 0,
            priority: 0,
            kind: SwapKind::File,
            active: true,
        }];
        state.selected_dev = 0;
        state.handle_action(Action::RequestConfirmOffDelete);
        assert!(state.confirm_off_delete.is_some());
        state.handle_action(Action::CancelConfirmOffDelete);
        assert!(state.confirm_off_delete.is_none());
    }
}
