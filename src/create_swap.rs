//! Create-swap wizard state types.
//!
//! All types for the create-swap modal live here. The background step runner
//! that performs the actual file operations lives in `platform::linux::create_swap`.

use std::path::PathBuf;

use crate::platform::StepStatus;

/// Operating mode of the create-swap modal.
#[derive(Debug)]
pub enum CreateSwapMode {
    Form {
        focused_field: CreateSwapField,
    },
    Progress {
        steps: Vec<CreateSwapStep>,
    },
    /// File exists and already has swap magic — ask user to just activate it.
    ConfirmActivateOnly {
        path: PathBuf,
        size_bytes: u64,
    },
}

/// Focusable fields in the form. Ordered to match navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateSwapField {
    Path,
    Size,
    SizeUnit,
    Priority,
    ActivateAfter,
    Submit,
}

impl CreateSwapField {
    pub fn next(self) -> Self {
        match self {
            Self::Path => Self::Size,
            Self::Size => Self::SizeUnit,
            Self::SizeUnit => Self::Priority,
            Self::Priority => Self::ActivateAfter,
            Self::ActivateAfter => Self::Submit,
            Self::Submit => Self::Path,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Path => Self::Submit,
            Self::Size => Self::Path,
            Self::SizeUnit => Self::Size,
            Self::Priority => Self::SizeUnit,
            Self::ActivateAfter => Self::Priority,
            Self::Submit => Self::ActivateAfter,
        }
    }
}

/// Size unit selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeUnit {
    Mb,
    Gb,
}

impl SizeUnit {
    pub fn label(self) -> &'static str {
        match self {
            Self::Mb => "MB",
            Self::Gb => "GB",
        }
    }

    pub fn multiplier(self) -> u64 {
        match self {
            Self::Mb => 1024 * 1024,
            Self::Gb => 1024 * 1024 * 1024,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Mb => Self::Gb,
            Self::Gb => Self::Mb,
        }
    }
}

/// A single wizard step.
#[derive(Debug, Clone)]
pub struct CreateSwapStep {
    pub label: String,
    pub status: StepStatus,
}

impl CreateSwapStep {
    pub fn pending(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            status: StepStatus::Pending,
        }
    }
}

/// Full modal state stored in `AppState`.
///
/// Not `Clone` — contains `tui_input::Input` state that should never be
/// duplicated. The reducer updates it in place.
pub struct CreateSwapModal {
    pub mode: CreateSwapMode,
    pub path_input: tui_input::Input,
    pub size_input: tui_input::Input,
    pub priority_input: tui_input::Input,
    pub size_unit: SizeUnit,
    pub activate_after: bool,
    pub validation_error: Option<String>,
    pub completions: Vec<String>,
    pub completion_sel: Option<usize>,
}

impl Default for CreateSwapModal {
    fn default() -> Self {
        Self {
            mode: CreateSwapMode::Form {
                focused_field: CreateSwapField::Path,
            },
            path_input: tui_input::Input::default(),
            size_input: tui_input::Input::from("2"),
            priority_input: tui_input::Input::from("0"),
            size_unit: SizeUnit::Gb,
            activate_after: true,
            validation_error: None,
            completions: Vec::new(),
            completion_sel: None,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_modal_starts_in_form_mode_focused_on_path() {
        let m = CreateSwapModal::default();
        match m.mode {
            CreateSwapMode::Form { focused_field } => {
                assert_eq!(focused_field, CreateSwapField::Path);
            }
            _ => panic!("expected Form mode"),
        }
        assert_eq!(m.size_unit, SizeUnit::Gb);
        assert!(m.activate_after);
        assert!(m.validation_error.is_none());
    }

    #[test]
    fn default_inputs_have_reasonable_values() {
        let m = CreateSwapModal::default();
        assert_eq!(m.path_input.value(), "");
        assert_eq!(m.size_input.value(), "2");
        assert_eq!(m.priority_input.value(), "0");
    }

    #[test]
    fn field_next_wraps_from_submit_to_path() {
        assert_eq!(CreateSwapField::Path.next(), CreateSwapField::Size);
        assert_eq!(CreateSwapField::Size.next(), CreateSwapField::SizeUnit);
        assert_eq!(CreateSwapField::SizeUnit.next(), CreateSwapField::Priority);
        assert_eq!(
            CreateSwapField::Priority.next(),
            CreateSwapField::ActivateAfter
        );
        assert_eq!(
            CreateSwapField::ActivateAfter.next(),
            CreateSwapField::Submit
        );
        assert_eq!(CreateSwapField::Submit.next(), CreateSwapField::Path);
    }

    #[test]
    fn field_prev_wraps_from_path_to_submit() {
        assert_eq!(CreateSwapField::Path.prev(), CreateSwapField::Submit);
        assert_eq!(
            CreateSwapField::Submit.prev(),
            CreateSwapField::ActivateAfter
        );
    }

    #[test]
    fn size_unit_toggled_flips_between_mb_and_gb() {
        assert_eq!(SizeUnit::Mb.toggled(), SizeUnit::Gb);
        assert_eq!(SizeUnit::Gb.toggled(), SizeUnit::Mb);
    }

    #[test]
    fn size_unit_label_matches_variant() {
        assert_eq!(SizeUnit::Mb.label(), "MB");
        assert_eq!(SizeUnit::Gb.label(), "GB");
    }

    #[test]
    fn size_unit_multiplier_matches_variant() {
        assert_eq!(SizeUnit::Mb.multiplier(), 1024 * 1024);
        assert_eq!(SizeUnit::Gb.multiplier(), 1024 * 1024 * 1024);
    }

    #[test]
    fn pending_step_has_correct_label_and_status() {
        let s = CreateSwapStep::pending("Check disk space");
        assert_eq!(s.label, "Check disk space");
        assert_eq!(s.status, StepStatus::Pending);
    }
}
