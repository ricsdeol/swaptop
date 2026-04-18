# Phase 5 — Create Swap File: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a modal wizard on the Devices tab (`n` key) that creates a new swap file through a step-by-step background task, detecting pre-existing swap files and reusing them when possible. Remove the placeholder `Tab::CreateSwap`.

**Architecture:** A `CreateSwapModal` lives inside `AppState` as `Option<CreateSwapModal>`. When opened via `Action::OpenCreateSwap`, it intercepts key events on the Devices tab. On submit, `main.rs` spawns a `tokio::task::spawn_blocking` task that runs 7 ordered steps (disk check, file check, filesystem detection, allocate, chmod, mkswap, swapon), each sending `Action::CreateSwapStepUpdate` back through the existing `action_tx` channel. Text input uses `tui-input` 0.15. Reducers stay pure; all I/O happens inside the background task.

**Tech Stack:** Rust 2024, Ratatui 0.30, crossterm 0.29, tokio 1.51 (`spawn_blocking`), nix 0.31 (`sys::statvfs`, `unistd::geteuid`), `tui-input` 0.15, external commands (`mkswap`, `fallocate`, `dd`).

**Spec:** `docs/superpowers/specs/2026-04-13-phase5-create-swap-design.md` (source of truth).

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `tui-input = { version = "0.15", features = ["crossterm"] }` |
| `src/create_swap.rs` | **Create** | All Phase 5 domain types + pure helpers + background step runner |
| `src/actions.rs` | Modify | Add 10 new `Action` variants for the modal + `use` for new types |
| `src/app.rs` | Modify | Remove `Tab::CreateSwap`; add `create_swap_modal` field; handle new actions |
| `src/input.rs` | Modify | Remove `SelectTab(4)` / `'4'`; add `n` key on Devices; route modal keys |
| `src/main.rs` | Modify | Remove `Tab::CreateSwap` import use; spawn background task on `CreateSwapSubmit` |
| `src/ui/mod.rs` | Modify | Remove `Tab::CreateSwap` arm + `render_coming_soon`; shrink tab bar to 3 |
| `src/ui/create_swap.rs` | **Create** | Render Form / Progress / ConfirmActivateOnly modal |
| `src/ui/devices.rs` | Modify | Add `n` key hint in footer; overlay create-swap modal when open |
| `docs/devices.md` | Modify | Add `n` row; update `Tab` / `1-4` to `1-3` |

---

## Task 1 — Add `tui-input` dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1.1: Add dependency**

Edit `Cargo.toml`. In `[dependencies]`, add this line after `glob`:

```toml
tui-input  = { version = "0.15", features = ["crossterm"] }
```

- [ ] **Step 1.2: Verify it compiles**

Run: `rtk cargo build`
Expected: Clean build. (`tui-input` 0.15.x is compatible with `crossterm 0.29`.)

- [ ] **Step 1.3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add tui-input 0.15 for Phase 5 create-swap form"
```

---

## Task 2 — Scaffold `src/create_swap.rs` with types (TDD)

**Files:**
- Create: `src/create_swap.rs`

- [ ] **Step 2.1: Create the file with types + failing tests**

Create `src/create_swap.rs`:

```rust
//! Phase 5 — Create Swap File modal state.
//!
//! All types for the create-swap wizard live here. The background step runner
//! that performs the actual file operations also lives in this module but in a
//! separate submodule so pure reducer code never imports std::fs.

use std::path::PathBuf;

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

/// Status of each step. Cannot be `Copy` due to `Error(String)` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Error(String),
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
        assert_eq!(CreateSwapField::Submit.prev(), CreateSwapField::ActivateAfter);
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
```

- [ ] **Step 2.2: Wire module into the crate**

Edit `src/main.rs`. Find the line `mod collector;` and immediately after it add:

```rust
mod create_swap;
```

- [ ] **Step 2.3: Run the tests**

Run: `rtk cargo test create_swap::tests`
Expected: All 8 tests PASS.

- [ ] **Step 2.4: Commit**

```bash
git add src/create_swap.rs src/main.rs
git commit -m "feat(phase5): scaffold create_swap module with form state types"
```

---

## Task 3 — Add `Action` variants for the modal

**Files:**
- Modify: `src/actions.rs`

- [ ] **Step 3.1: Add new variants to the `Action` enum**

Edit `src/actions.rs`. At the top, under `use std::path::PathBuf;` add:

```rust
use crate::create_swap::{CreateSwapField, StepStatus};
```

Then, inside the `Action` enum, append these variants after `ExitFilterMode,` (right before the closing brace):

```rust
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
```

- [ ] **Step 3.2: Add tests for the new variants**

In the existing `tests` module of `src/actions.rs`, append:

```rust
    #[test]
    fn open_create_swap_is_constructible() {
        let a = Action::OpenCreateSwap;
        assert!(matches!(a, Action::OpenCreateSwap));
    }

    #[test]
    fn create_swap_submit_carries_activate_only_flag() {
        let a = Action::CreateSwapSubmit { activate_only: true };
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
```

- [ ] **Step 3.3: Run tests**

Run: `rtk cargo test actions::tests`
Expected: All tests PASS (including the 3 new ones).

- [ ] **Step 3.4: Commit**

```bash
git add src/actions.rs
git commit -m "feat(phase5): add Action variants for create-swap modal"
```

---

## Task 4 — Remove `Tab::CreateSwap` from `AppState` + add modal field

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 4.1: Remove the `CreateSwap` variant from the `Tab` enum**

Edit `src/app.rs`. Replace:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
    CreateSwap,
}
```

with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Overview,
    Processes,
    Devices,
}
```

- [ ] **Step 4.2: Add import for the new modal type**

Near the top of `src/app.rs`, add after the existing `use crate::actions::...` line:

```rust
use crate::create_swap::{CreateSwapMode, CreateSwapModal, CreateSwapStep, StepStatus};
```

- [ ] **Step 4.3: Add `create_swap_modal` field to `AppState`**

In the `AppState` struct, after `pub filter_mode: bool,` add:

```rust

    // Phase 5
    pub create_swap_modal: Option<CreateSwapModal>,
```

In `AppState::new`, after `filter_mode: false,` add:

```rust
            create_swap_modal: None,
```

- [ ] **Step 4.4: Update `NextTab` / `PrevTab` / `SelectTab` to cycle 3 tabs**

Replace the `Action::NextTab` arm:

```rust
            Action::NextTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview => Tab::Processes,
                    Tab::Processes => Tab::Devices,
                    Tab::Devices => Tab::Overview,
                };
            }
```

Replace the `Action::PrevTab` arm:

```rust
            Action::PrevTab => {
                self.active_tab = match self.active_tab {
                    Tab::Overview => Tab::Devices,
                    Tab::Processes => Tab::Overview,
                    Tab::Devices => Tab::Processes,
                };
            }
```

Replace the `Action::SelectTab(n)` arm:

```rust
            Action::SelectTab(n) => {
                self.active_tab = match n {
                    1 => Tab::Overview,
                    2 => Tab::Processes,
                    3 => Tab::Devices,
                    _ => return,
                };
            }
```

- [ ] **Step 4.5: Add handlers for the 10 new Phase 5 actions**

Inside the `handle_action` `match`, right before the closing brace (after `Action::ExitFilterMode => { ... }`), add:

```rust

            // Phase 5 — create swap modal
            Action::OpenCreateSwap => {
                self.create_swap_modal = Some(CreateSwapModal::default());
            }

            Action::CloseCreateSwap => {
                self.create_swap_modal = None;
            }

            Action::CreateSwapReturnToForm => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.mode = CreateSwapMode::Form {
                        focused_field: crate::create_swap::CreateSwapField::Submit,
                    };
                    // validation_error preserved so the user sees what failed
                }
            }

            Action::CreateSwapFocusField(field) => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Form { focused_field } = &mut modal.mode {
                        *focused_field = field;
                    }
                }
            }

            Action::CreateSwapInputEvent(event) => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Form { focused_field } = modal.mode {
                        use tui_input::backend::crossterm::EventHandler;
                        let target = match focused_field {
                            crate::create_swap::CreateSwapField::Path => {
                                Some(&mut modal.path_input)
                            }
                            crate::create_swap::CreateSwapField::Size => {
                                Some(&mut modal.size_input)
                            }
                            crate::create_swap::CreateSwapField::Priority => {
                                Some(&mut modal.priority_input)
                            }
                            _ => None,
                        };
                        if let Some(input) = target {
                            let _ = input.handle_event(&event);
                        }
                    }
                }
            }

            Action::CreateSwapToggleUnit => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.size_unit = modal.size_unit.toggled();
                }
            }

            Action::CreateSwapToggleActivate => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.activate_after = !modal.activate_after;
                }
            }

            Action::CreateSwapSubmit { activate_only: _ } => {
                // Reducer only transitions Form -> Progress; the background task
                // is spawned in main.rs before the action reaches the reducer.
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.validation_error = None;
                    modal.mode = CreateSwapMode::Progress {
                        steps: vec![
                            CreateSwapStep::pending("Check disk space"),
                            CreateSwapStep::pending("Check target file"),
                            CreateSwapStep::pending("Detect filesystem"),
                            CreateSwapStep::pending("Allocate file"),
                            CreateSwapStep::pending("chmod 600"),
                            CreateSwapStep::pending("mkswap"),
                            CreateSwapStep::pending("swapon"),
                        ],
                    };
                }
            }

            Action::OpenConfirmActivateOnly { path, size_bytes } => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    modal.mode = CreateSwapMode::ConfirmActivateOnly { path, size_bytes };
                }
            }

            Action::CreateSwapStepUpdate { index, status } => {
                if let Some(modal) = self.create_swap_modal.as_mut() {
                    if let CreateSwapMode::Progress { steps } = &mut modal.mode {
                        if let Some(step) = steps.get_mut(index) {
                            step.status = status;
                        }
                    }
                }
            }
```

- [ ] **Step 4.6: Update the existing `Tab::CreateSwap` tests**

Two tests in the `tests` module reference `Tab::CreateSwap`. Replace:

```rust
    #[test]
    fn next_tab_cycles_forward_through_all_tabs() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Processes);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Devices);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::CreateSwap);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Overview);
    }
```

with:

```rust
    #[test]
    fn next_tab_cycles_forward_through_all_tabs() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Processes);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Devices);
        state.handle_action(Action::NextTab);
        assert_eq!(state.active_tab, Tab::Overview);
    }
```

Replace:

```rust
    #[test]
    fn prev_tab_wraps_backward_from_overview() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::PrevTab);
        assert_eq!(state.active_tab, Tab::CreateSwap);
    }
```

with:

```rust
    #[test]
    fn prev_tab_wraps_backward_from_overview() {
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::PrevTab);
        assert_eq!(state.active_tab, Tab::Devices);
    }
```

- [ ] **Step 4.7: Add reducer tests for the Phase 5 actions**

At the end of the `tests` module (before the closing brace), add:

```rust

    // ── Phase 5 — create swap modal ──────────────────────────────────────────

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
        use crate::create_swap::{CreateSwapField, CreateSwapMode};
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
        use crate::create_swap::SizeUnit;
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
        use crate::create_swap::CreateSwapMode;
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::Progress { steps } => assert_eq!(steps.len(), 7),
            _ => panic!("expected Progress mode"),
        }
    }

    #[test]
    fn step_update_replaces_status_at_index() {
        use crate::create_swap::{CreateSwapMode, StepStatus};
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
        state.handle_action(Action::CreateSwapStepUpdate {
            index: 0,
            status: StepStatus::Running,
        });
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
        use crate::create_swap::{CreateSwapField, CreateSwapMode};
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        // simulate: user typed something, submitted, Progress step errored
        {
            let modal = state.create_swap_modal.as_mut().unwrap();
            modal.validation_error = Some("some prior error".to_string());
        }
        state.handle_action(Action::CreateSwapSubmit { activate_only: false });
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
        use crate::create_swap::CreateSwapMode;
        use std::path::PathBuf;
        let mut state = AppState::new(make_caps());
        state.handle_action(Action::OpenCreateSwap);
        state.handle_action(Action::OpenConfirmActivateOnly {
            path: PathBuf::from("/swapfile"),
            size_bytes: 2_147_483_648,
        });
        let modal = state.create_swap_modal.as_ref().unwrap();
        match &modal.mode {
            CreateSwapMode::ConfirmActivateOnly { path, size_bytes } => {
                assert_eq!(path, &PathBuf::from("/swapfile"));
                assert_eq!(*size_bytes, 2_147_483_648);
            }
            _ => panic!("expected ConfirmActivateOnly mode"),
        }
    }
```

- [ ] **Step 4.8: Build and run the reducer tests**

Run: `rtk cargo build`
Expected: May fail because `main.rs`, `input.rs`, and `ui/mod.rs` still reference `Tab::CreateSwap`. That's fine — we handle those in the next tasks. But **FIRST** confirm `src/app.rs` compiles in isolation by running only its tests:

Run: `rtk cargo test --lib app::tests 2>&1 | tail -40`

If the whole crate fails to compile because of other files still referencing `Tab::CreateSwap`, proceed to Task 5 without committing yet — we'll commit once the crate compiles.

- [ ] **Step 4.9: Commit (deferred until crate compiles — after Task 6)**

Do not commit yet.

---

## Task 5 — Shrink tab bar in `src/ui/mod.rs`

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 5.1: Remove the `CreateSwap` tab title and arm**

Edit `src/ui/mod.rs`. Replace the `render_tabbar` `titles` vec:

```rust
    let titles = vec![
        Line::from(vec![
            Span::styled(
                "1",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":Overview", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                "2",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":Processes", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled(
                "3",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(":Devices", Style::default().fg(Color::White)),
        ]),
    ];
```

Replace the `selected` match:

```rust
    let selected = match state.active_tab {
        Tab::Overview => 0,
        Tab::Processes => 1,
        Tab::Devices => 2,
    };
```

- [ ] **Step 5.2: Remove the catch-all tab dispatch arm**

In the `render` function, replace:

```rust
    match state.active_tab {
        Tab::Overview => overview::render(f, layout[1], state),
        Tab::Processes => processes::render(f, layout[1], state),
        Tab::Devices => devices::render(f, layout[1], state),
        _ => render_coming_soon(f, layout[1]),
    }
```

with:

```rust
    match state.active_tab {
        Tab::Overview => overview::render(f, layout[1], state),
        Tab::Processes => processes::render(f, layout[1], state),
        Tab::Devices => devices::render(f, layout[1], state),
    }
```

- [ ] **Step 5.3: Remove the now-unused `render_coming_soon` function**

Delete the entire `fn render_coming_soon(...)` function (lines 118–132 in the original file). Also remove the `Paragraph` import from the `use ratatui::widgets::{...}` line if it's no longer referenced elsewhere in the file (search the file for other uses of `Paragraph`).

- [ ] **Step 5.4: Add `create_swap` submodule declaration**

At the top of `src/ui/mod.rs`, add after `mod devices;`:

```rust
mod create_swap;
```

(The file `src/ui/create_swap.rs` does not exist yet — that's Task 8. A stub will be created in the next step so this compiles.)

- [ ] **Step 5.5: Create an empty stub for `src/ui/create_swap.rs`**

Create `src/ui/create_swap.rs` with exactly:

```rust
//! Phase 5 — Create swap modal renderer. Real implementation lands in Task 8.

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::app::AppState;

#[allow(dead_code)]
pub fn render(_f: &mut Frame, _area: Rect, _state: &AppState) {
    // Stub — replaced in Task 8.
}
```

- [ ] **Step 5.6: Do not commit yet**

The crate still won't compile (input.rs/main.rs). Continue to Task 6.

---

## Task 6 — Update `src/input.rs` for the modal

**Files:**
- Modify: `src/input.rs`

- [ ] **Step 6.1: Remove the `'4'` tab shortcut**

Edit `src/input.rs`. In the global key section, delete this line:

```rust
        KeyCode::Char('4') => return Some(Action::SelectTab(4)),
```

- [ ] **Step 6.2: Add a modal-intercept check at the top of `resolve_key`**

In `resolve_key`, **after** the `filter_mode` block but **before** the global keys `match`, add:

```rust
    // Priority 1.5: create-swap modal intercepts keys when open.
    let modal_open = {
        let s = state.lock().expect("state mutex poisoned");
        s.create_swap_modal.is_some()
    };
    if modal_open {
        return handle_create_swap_key(key, state);
    }
```

You will need to import the new helper function below.

- [ ] **Step 6.3: Add `n` key handling inside `handle_devices_key`**

In `handle_devices_key`, after the `Tab::Devices`-arm that handles `'f'`, add a new arm for `'n'`:

```rust
        KeyCode::Char('n') if has_devices || !has_devices => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::OpenCreateSwap)
            } else {
                Some(Action::SetError(
                    "Requires root — run: sudo swaptop".to_string(),
                ))
            }
        }
```

(`n` is available even when no devices are listed — this lets the user create the very first swap file.)

- [ ] **Step 6.4: Add `handle_create_swap_key` helper**

At the bottom of `src/input.rs`, before the `#[cfg(test)]` module, add:

```rust
fn handle_create_swap_key(
    key: KeyEvent,
    state: &Arc<Mutex<AppState>>,
) -> Option<Action> {
    use crate::create_swap::{CreateSwapField, CreateSwapMode};

    // Snapshot of everything we need from the modal — released before dispatching.
    let (mode_variant, focused_field, path_value, size_value, priority_value, size_unit) = {
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
        )
    };

    match mode_variant {
        "form" => {
            let focused = focused_field?;
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
                KeyCode::Enter if focused == CreateSwapField::Submit => validate_and_submit(
                    state,
                    &path_value,
                    &size_value,
                    &priority_value,
                    size_unit,
                ),
                _ => {
                    // Route everything else to the active tui-input for text fields.
                    if matches!(
                        focused,
                        CreateSwapField::Path | CreateSwapField::Size | CreateSwapField::Priority
                    ) {
                        Some(Action::CreateSwapInputEvent(
                            crossterm::event::Event::Key(key),
                        ))
                    } else {
                        None
                    }
                }
            }
        }
        "progress" => match key.code {
            KeyCode::Esc => {
                // Inspect whether any step has errored to decide between
                // CloseCreateSwap and CreateSwapReturnToForm.
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
            KeyCode::Char('s') | KeyCode::Enter => {
                Some(Action::CreateSwapSubmit { activate_only: true })
            }
            KeyCode::Esc => Some(Action::CloseCreateSwap),
            _ => None,
        },
        _ => None,
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

    // Validation passed — clear any prior error and submit.
    {
        let mut s = state.lock().expect("state mutex poisoned");
        if let Some(m) = s.create_swap_modal.as_mut() {
            m.validation_error = None;
        }
    }
    Some(Action::CreateSwapSubmit { activate_only: false })
}
```

- [ ] **Step 6.5: Fix the `Tab::CreateSwap` reference in the existing tests**

In `src/input.rs`, in the `global_quit_keys_work_from_any_tab` test, replace:

```rust
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices, Tab::CreateSwap] {
```

with:

```rust
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices] {
```

Also in the `number_keys_select_tabs` test, replace:

```rust
        for n in [1_usize, 2, 3, 4] {
```

with:

```rust
        for n in [1_usize, 2, 3] {
```

- [ ] **Step 6.6: Build**

Run: `rtk cargo build 2>&1 | tail -20`
Expected: Still failing — `main.rs` imports `Tab::CreateSwap` via `use app::{AppState, Tab}` and references the mpsc dispatch. Fix in Task 7.

- [ ] **Step 6.7: Do not commit yet**

Continue to Task 7.

---

## Task 7 — Wire background task in `src/main.rs`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 7.1: Simplify the `Tab` import**

In `src/main.rs`, the line:

```rust
use app::{AppState, Tab};
```

stays the same — `Tab` is still used (it's imported for `processes_active.store(... == Tab::Processes ...)`).

- [ ] **Step 7.2: Add Phase 5 imports**

In the existing `use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};` line, add imports for the background step runner:

```rust
use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use create_swap::run_create_swap_steps;
```

(The `run_create_swap_steps` function is added in Task 9 — for now this will fail to compile, but we add it here to keep the task ordering logical. We'll stub it in Task 7.5 below.)

- [ ] **Step 7.3: Add `spawn_blocking` dispatch for `CreateSwapSubmit`**

In `run`, inside the `tokio::select!` input-event branch, immediately after the existing `if let Some(Action::ExecuteDeviceOp { ... })` block, add:

```rust
                    // Phase 5 — spawn background create-swap task
                    if let Some(Action::CreateSwapSubmit { activate_only }) = action {
                        // Snapshot all form values under a short-lived lock.
                        let submit = {
                            let s = state.lock().expect("state mutex poisoned");
                            s.create_swap_modal.as_ref().map(|m| {
                                let size_n: u64 = m.size_input.value().trim().parse().unwrap_or(0);
                                let size_bytes = size_n * m.size_unit.multiplier();
                                let prio_n: i32 = m.priority_input.value().trim().parse().unwrap_or(0);
                                (
                                    std::path::PathBuf::from(m.path_input.value()),
                                    size_bytes,
                                    prio_n as i16,
                                    m.activate_after,
                                )
                            })
                        };
                        if let Some((path, size_bytes, priority, activate_after)) = submit {
                            let tx = action_tx.clone();
                            tokio::task::spawn_blocking(move || {
                                run_create_swap_steps(
                                    path,
                                    size_bytes,
                                    priority,
                                    activate_after,
                                    activate_only,
                                    tx,
                                );
                            });
                        }
                    }
```

- [ ] **Step 7.4: Add a stub for `run_create_swap_steps` in `create_swap.rs`**

Edit `src/create_swap.rs`. At the bottom of the file (before `#[cfg(test)]`), add:

```rust
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;

use crate::actions::Action;

/// Stub — replaced with the real implementation in Task 9.
#[allow(clippy::too_many_arguments)]
pub fn run_create_swap_steps(
    _path: PathBuf,
    _size_bytes: u64,
    _priority: i16,
    _activate_after: bool,
    _activate_only: bool,
    _tx: UnboundedSender<Action>,
) {
    // Implemented in Task 9.
}
```

Note: `PathBuf` and `UnboundedSender` imports — `PathBuf` may already be used; only add if not. Ensure the imports at the top of the file (or near this function) are consistent. If `use std::path::PathBuf;` is already at the top of `create_swap.rs`, do not duplicate it.

- [ ] **Step 7.5: Build the whole crate**

Run: `rtk cargo build 2>&1 | tail -30`
Expected: Clean build. Warnings about unused code are OK at this point but not errors.

- [ ] **Step 7.6: Run all tests**

Run: `rtk cargo test 2>&1 | tail -40`
Expected: All tests PASS (actions, app, create_swap, input, linux parser, processes, ui layout).

- [ ] **Step 7.7: Commit the whole scaffolding**

```bash
git add src/create_swap.rs src/actions.rs src/app.rs src/input.rs src/main.rs src/ui/mod.rs src/ui/create_swap.rs
git commit -m "feat(phase5): remove Tab::CreateSwap, add modal state + reducers + key routing"
```

---

## Task 8 — Render the create-swap modal (`src/ui/create_swap.rs`)

**Files:**
- Modify: `src/ui/create_swap.rs` (replaces stub from Task 5)
- Modify: `src/ui/devices.rs`

- [ ] **Step 8.1: Replace the stub with the real renderer**

Replace the full contents of `src/ui/create_swap.rs` with:

```rust
//! Phase 5 — Create swap modal renderer. Overlays the Devices tab area.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::AppState;
use crate::create_swap::{CreateSwapField, CreateSwapMode, CreateSwapModal, StepStatus};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(modal) = state.create_swap_modal.as_ref() else {
        return;
    };

    let modal_rect = centered_rect(area, 64, 18);
    f.render_widget(Clear, modal_rect);

    match &modal.mode {
        CreateSwapMode::Form { focused_field } => {
            render_form(f, modal_rect, modal, *focused_field);
        }
        CreateSwapMode::Progress { steps } => {
            render_progress(f, modal_rect, modal, steps);
        }
        CreateSwapMode::ConfirmActivateOnly { path, size_bytes } => {
            render_confirm_activate(f, modal_rect, path, *size_bytes);
        }
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn render_form(f: &mut Frame, area: Rect, modal: &CreateSwapModal, focused: CreateSwapField) {
    let block = Block::default()
        .title(Span::styled(
            " New Swap File ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Path
            Constraint::Length(1), // Size + unit
            Constraint::Length(1), // Priority
            Constraint::Length(1), // Activate
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Submit
            Constraint::Length(1), // spacer
            Constraint::Length(1), // validation error or hint
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(field_line(
            "Path:    ",
            modal.path_input.value(),
            focused == CreateSwapField::Path,
        )),
        rows[0],
    );
    let size_line = Line::from(vec![
        label_span("Size:    "),
        value_span(modal.size_input.value(), focused == CreateSwapField::Size),
        Span::raw(" "),
        unit_span(modal.size_unit.label(), focused == CreateSwapField::SizeUnit),
    ]);
    f.render_widget(Paragraph::new(size_line), rows[1]);
    f.render_widget(
        Paragraph::new(field_line(
            "Priority:",
            modal.priority_input.value(),
            focused == CreateSwapField::Priority,
        )),
        rows[2],
    );

    let checkbox = if modal.activate_after { "[x]" } else { "[ ]" };
    let activate_line = Line::from(vec![
        label_span("Activate:"),
        Span::raw(" "),
        Span::styled(
            format!("{checkbox} activate after create"),
            if focused == CreateSwapField::ActivateAfter {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        ),
    ]);
    f.render_widget(Paragraph::new(activate_line), rows[3]);

    let submit_label = if focused == CreateSwapField::Submit {
        Span::styled(
            " ▶ [  Create  ] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            "   [  Create  ] ",
            Style::default().fg(Color::White),
        )
    };
    f.render_widget(Paragraph::new(Line::from(submit_label)), rows[5]);

    let hint_or_error = if let Some(err) = &modal.validation_error {
        Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::from(Span::styled(
            "  ↑/↓ navigate · Space toggle · Enter submit · Esc cancel",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(hint_or_error), rows[7]);
}

fn field_line<'a>(label: &'a str, value: &'a str, focused: bool) -> Line<'a> {
    Line::from(vec![label_span(label), Span::raw(" "), value_span(value, focused)])
}

fn label_span(s: &str) -> Span<'_> {
    Span::styled(s.to_string(), Style::default().fg(Color::DarkGray))
}

fn value_span<'a>(s: &'a str, focused: bool) -> Span<'a> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let padded = format!("[{:<30}]", s);
    Span::styled(padded, style)
}

fn unit_span(s: &str, focused: bool) -> Span<'_> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Span::styled(format!(" {s} "), style)
}

fn render_progress(
    f: &mut Frame,
    area: Rect,
    modal: &CreateSwapModal,
    steps: &[crate::create_swap::CreateSwapStep],
) {
    let title = format!(" Creating {} ", modal.path_input.value());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = steps
        .iter()
        .map(|s| {
            let (icon, color) = match &s.status {
                StepStatus::Pending => ("·", Color::DarkGray),
                StepStatus::Running => ("⏳", Color::Yellow),
                StepStatus::Done => ("✓", Color::Green),
                StepStatus::Error(_) => ("✗", Color::Red),
            };
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(icon.to_string(), Style::default().fg(color)),
                Span::raw("  "),
                Span::styled(s.label.clone(), Style::default().fg(Color::White)),
            ];
            if let StepStatus::Error(msg) = &s.status {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(msg.clone(), Style::default().fg(Color::Red)));
            }
            Line::from(spans)
        })
        .collect();

    let has_error = steps
        .iter()
        .any(|s| matches!(s.status, StepStatus::Error(_)));
    let footer = if has_error {
        Line::from(Span::styled(
            "  Esc return to form",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            "  Esc cancel (before file write)",
            Style::default().fg(Color::DarkGray),
        ))
    };

    let mut full = lines;
    full.push(Line::from(""));
    full.push(footer);

    f.render_widget(Paragraph::new(full), inner);
}

fn render_confirm_activate(
    f: &mut Frame,
    area: Rect,
    path: &std::path::Path,
    size_bytes: u64,
) {
    let block = Block::default()
        .title(Span::styled(
            " Already a swap file ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pretty_size = human_bytes::human_bytes(size_bytes as f64);
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", path.display()),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("  already contains a {pretty_size} swap area."),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " s ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" activate    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn centered_rect_stays_within_bounds() {
        let area = Rect::new(0, 0, 100, 40);
        let r = centered_rect(area, 60, 20);
        assert_eq!(r.width, 60);
        assert_eq!(r.height, 20);
        assert!(r.x + r.width <= area.x + area.width);
        assert!(r.y + r.height <= area.y + area.height);
    }

    #[test]
    fn centered_rect_clamps_when_area_smaller_than_requested() {
        let area = Rect::new(0, 0, 30, 10);
        let r = centered_rect(area, 60, 20);
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
    }
}
```

- [ ] **Step 8.2: Overlay the modal from the Devices renderer**

Edit `src/ui/devices.rs`. At the end of the existing `render` function (after `render_footer(...)` and the `confirm_action` check), add:

```rust

    if state.create_swap_modal.is_some() {
        crate::ui::create_swap::render(f, area, state);
    }
```

Also expose `create_swap` to this sibling. In `src/ui/mod.rs`, change:

```rust
mod create_swap;
```

to:

```rust
pub(crate) mod create_swap;
```

- [ ] **Step 8.3: Add `n` key hint to the devices footer**

In `src/ui/devices.rs`, inside `render_footer`, replace the existing `hint_line`:

```rust
    let hint_line = Line::from(vec![
        key_span("o"),
        desc_span(" activate  "),
        key_span("f"),
        desc_span(" deactivate  "),
        key_span("r"),
        desc_span(" reset  "),
        key_span("n"),
        desc_span(" new swap  "),
        key_span("j/k"),
        desc_span(" navigate"),
    ]);
```

- [ ] **Step 8.4: Build and run UI tests**

Run: `rtk cargo build 2>&1 | tail -15`
Expected: Clean build.

Run: `rtk cargo test ui:: 2>&1 | tail -20`
Expected: All UI tests PASS (including the 2 new `centered_rect_*` tests).

- [ ] **Step 8.5: Commit**

```bash
git add src/ui/create_swap.rs src/ui/devices.rs src/ui/mod.rs
git commit -m "feat(phase5): render create-swap modal (form/progress/confirm-activate)"
```

---

## Task 9 — Pure helpers for the background task

**Files:**
- Modify: `src/create_swap.rs`

- [ ] **Step 9.1: Add swap-magic detection helper + tests**

Edit `src/create_swap.rs`. At the bottom of the file, before the `#[cfg(test)]` block, add:

```rust
/// Swap header magic lives at bytes 4086..4096 of the first page.
///
/// Returns `Some(size_bytes)` if `buf` is ≥4096 bytes AND the magic matches.
/// `size_bytes` is supplied by the caller (from `fs::metadata().len()`).
pub fn detect_swap_magic(buf: &[u8], size_bytes: u64) -> Option<u64> {
    if buf.len() < 4096 {
        return None;
    }
    let magic = &buf[4086..4096];
    if magic == b"SWAPSPACE2" || magic == b"SWAP-SPACE" {
        Some(size_bytes)
    } else {
        None
    }
}

/// Parse `/proc/mounts` content and return the filesystem type covering `target`.
///
/// Picks the longest mount-point prefix that is a parent of `target`.
pub fn detect_fs_type(mounts_content: &str, target: &std::path::Path) -> Option<String> {
    let target_str = target.to_string_lossy();
    let mut best: Option<(usize, String)> = None;
    for line in mounts_content.lines() {
        let mut parts = line.split_whitespace();
        let _dev = parts.next()?;
        let mount_point = parts.next()?;
        let fs_type = parts.next()?;
        if target_str.starts_with(mount_point)
            && (target_str.len() == mount_point.len()
                || target_str.as_bytes().get(mount_point.len()) == Some(&b'/')
                || mount_point == "/")
        {
            let len = mount_point.len();
            if best.as_ref().map(|(l, _)| len > *l).unwrap_or(true) {
                best = Some((len, fs_type.to_string()));
            }
        }
    }
    best.map(|(_, fs)| fs)
}

/// Given the filesystem type, decide whether to use `fallocate` or `dd`.
pub fn allocator_for_fs(fs_type: &str) -> Allocator {
    match fs_type {
        "ext2" | "ext3" | "ext4" | "xfs" | "f2fs" | "btrfs" => Allocator::Fallocate,
        _ => Allocator::Dd,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Allocator {
    Fallocate,
    Dd,
}

impl Allocator {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fallocate => "fallocate",
            Self::Dd => "dd",
        }
    }
}
```

Append to the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn detect_swap_magic_returns_size_on_swapspace2() {
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
        assert_eq!(detect_swap_magic(&buf, 2_147_483_648), Some(2_147_483_648));
    }

    #[test]
    fn detect_swap_magic_returns_size_on_swap_space() {
        let mut buf = vec![0u8; 4096];
        buf[4086..4096].copy_from_slice(b"SWAP-SPACE");
        assert_eq!(detect_swap_magic(&buf, 1024), Some(1024));
    }

    #[test]
    fn detect_swap_magic_returns_none_on_unknown_bytes() {
        let buf = vec![0u8; 4096];
        assert_eq!(detect_swap_magic(&buf, 4096), None);
    }

    #[test]
    fn detect_swap_magic_returns_none_on_short_buffer() {
        let buf = vec![0u8; 100];
        assert_eq!(detect_swap_magic(&buf, 100), None);
    }

    #[test]
    fn detect_fs_type_matches_root_mount() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
proc /proc proc rw 0 0
tmpfs /tmp tmpfs rw 0 0
";
        let fs = detect_fs_type(mounts, std::path::Path::new("/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn detect_fs_type_prefers_longest_match() {
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
tmpfs /home/user/ramdisk tmpfs rw 0 0
";
        let fs = detect_fs_type(
            mounts,
            std::path::Path::new("/home/user/ramdisk/swapfile"),
        );
        assert_eq!(fs.as_deref(), Some("tmpfs"));
    }

    #[test]
    fn detect_fs_type_ignores_unrelated_mount_point() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\ntmpfs /tmp tmpfs rw 0 0\n";
        let fs = detect_fs_type(mounts, std::path::Path::new("/var/swapfile"));
        assert_eq!(fs.as_deref(), Some("ext4"));
    }

    #[test]
    fn allocator_for_fs_picks_fallocate_on_ext4() {
        assert_eq!(allocator_for_fs("ext4"), Allocator::Fallocate);
        assert_eq!(allocator_for_fs("xfs"), Allocator::Fallocate);
        assert_eq!(allocator_for_fs("btrfs"), Allocator::Fallocate);
    }

    #[test]
    fn allocator_for_fs_falls_back_to_dd_on_tmpfs_or_unknown() {
        assert_eq!(allocator_for_fs("tmpfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("ramfs"), Allocator::Dd);
        assert_eq!(allocator_for_fs("whatever"), Allocator::Dd);
    }
```

- [ ] **Step 9.2: Implement the real `run_create_swap_steps`**

Replace the stub `run_create_swap_steps` from Task 7.4 with:

```rust
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::process::Command as StdCommand;

/// Sends `Action::CreateSwapStepUpdate { index, status }` through `tx`.
/// The background task runs in `spawn_blocking`, so this is a synchronous function.
#[allow(clippy::too_many_arguments)]
pub fn run_create_swap_steps(
    path: PathBuf,
    size_bytes: u64,
    priority: i16,
    activate_after: bool,
    activate_only: bool,
    tx: UnboundedSender<Action>,
) {
    let send = |idx: usize, status: StepStatus| {
        let _ = tx.send(Action::CreateSwapStepUpdate { index: idx, status });
    };

    // activate_only path: go straight to Step 6 (swapon).
    if activate_only {
        send(6, StepStatus::Running);
        match do_swapon(&path) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => send(6, StepStatus::Error(e)),
        }
        return;
    }

    // Step 0 — Disk space
    send(0, StepStatus::Running);
    let parent = path.parent().unwrap_or(std::path::Path::new("/"));
    match check_disk_space(parent, size_bytes) {
        Ok(()) => send(0, StepStatus::Done),
        Err(e) => {
            send(0, StepStatus::Error(e));
            return;
        }
    }

    // Step 1 — File existence / magic
    send(1, StepStatus::Running);
    match check_target_file(&path) {
        TargetFileCheck::DoesNotExist => send(1, StepStatus::Done),
        TargetFileCheck::AlreadySwap { size } => {
            send(1, StepStatus::Done);
            let _ = tx.send(Action::OpenConfirmActivateOnly {
                path: path.clone(),
                size_bytes: size,
            });
            return;
        }
        TargetFileCheck::ExistsNotSwap => {
            send(
                1,
                StepStatus::Error(
                    "file exists and is not a swap file — refusing to overwrite".to_string(),
                ),
            );
            return;
        }
        TargetFileCheck::IoError(e) => {
            send(1, StepStatus::Error(format!("cannot inspect target: {e}")));
            return;
        }
    }

    // Step 2 — Filesystem detection
    send(2, StepStatus::Running);
    let fs_type = match fs::read_to_string("/proc/mounts") {
        Ok(content) => detect_fs_type(&content, &path).unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unknown".to_string(),
    };
    let allocator = allocator_for_fs(&fs_type);
    send(2, StepStatus::Done);

    // Step 3 — Allocate
    send(3, StepStatus::Running);
    let alloc_result = match allocator {
        Allocator::Fallocate => run_cmd(
            StdCommand::new("fallocate")
                .arg("-l")
                .arg(size_bytes.to_string())
                .arg(&path),
        ),
        Allocator::Dd => {
            let mb = size_bytes / (1024 * 1024);
            let mb = if mb == 0 { 1 } else { mb };
            run_cmd(
                StdCommand::new("dd")
                    .arg("if=/dev/zero")
                    .arg(format!("of={}", path.display()))
                    .arg("bs=1M")
                    .arg(format!("count={mb}")),
            )
        }
    };
    match alloc_result {
        Ok(()) => send(3, StepStatus::Done),
        Err(e) => {
            send(3, StepStatus::Error(format!("{} failed: {e}", allocator.label())));
            return;
        }
    }

    // Step 4 — chmod 600
    send(4, StepStatus::Running);
    match fs::set_permissions(&path, fs::Permissions::from_mode(0o600)) {
        Ok(()) => send(4, StepStatus::Done),
        Err(e) => {
            send(4, StepStatus::Error(format!("chmod failed: {e}")));
            return;
        }
    }

    // Step 5 — mkswap
    send(5, StepStatus::Running);
    match run_cmd(StdCommand::new("mkswap").arg(&path)) {
        Ok(()) => send(5, StepStatus::Done),
        Err(e) => {
            send(5, StepStatus::Error(format!("mkswap failed: {e}")));
            return;
        }
    }

    // Step 6 — swapon (skipped if activate_after is false)
    if activate_after {
        send(6, StepStatus::Running);
        match do_swapon_with_priority(&path, priority) {
            Ok(()) => send(6, StepStatus::Done),
            Err(e) => {
                send(6, StepStatus::Error(e));
                return;
            }
        }
    } else {
        send(6, StepStatus::Done);
    }
}

enum TargetFileCheck {
    DoesNotExist,
    AlreadySwap { size: u64 },
    ExistsNotSwap,
    IoError(String),
}

fn check_target_file(path: &std::path::Path) -> TargetFileCheck {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return TargetFileCheck::DoesNotExist,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    if !meta.is_file() {
        return TargetFileCheck::ExistsNotSwap;
    }
    let size = meta.len();
    let mut f = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return TargetFileCheck::IoError(e.to_string()),
    };
    let mut buf = vec![0u8; 4096];
    match f.read_exact(&mut buf) {
        Ok(()) => {
            if let Some(size) = detect_swap_magic(&buf, size) {
                TargetFileCheck::AlreadySwap { size }
            } else {
                TargetFileCheck::ExistsNotSwap
            }
        }
        Err(_) => TargetFileCheck::ExistsNotSwap,
    }
}

fn check_disk_space(parent: &std::path::Path, needed: u64) -> Result<(), String> {
    let stat = nix::sys::statvfs::statvfs(parent).map_err(|e| e.to_string())?;
    let available = stat.blocks_available() as u64 * stat.fragment_size() as u64;
    let required = needed + needed / 10;
    if available >= required {
        Ok(())
    } else {
        Err(format!(
            "not enough space: need {} (incl. 10% margin), have {}",
            human_bytes::human_bytes(required as f64),
            human_bytes::human_bytes(available as f64),
        ))
    }
}

fn run_cmd(cmd: &mut StdCommand) -> Result<(), String> {
    let output = cmd.output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr.trim().to_string())
    }
}

fn do_swapon(path: &std::path::Path) -> Result<(), String> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    // SAFETY: c is a valid NUL-terminated C string.
    let ret = unsafe { nix::libc::swapon(c.as_ptr(), 0) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!("swapon failed: {}", std::io::Error::last_os_error()))
    }
}

fn do_swapon_with_priority(path: &std::path::Path, priority: i16) -> Result<(), String> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    // SYS_swapon flags: SWAP_FLAG_PREFER(=0x8000) | (priority & SWAP_FLAG_PRIO_MASK=0x7fff)
    // If priority == -1, omit the flag (kernel-default priority).
    let flags: i32 = if priority < 0 {
        0
    } else {
        0x8000 | (priority as i32 & 0x7fff)
    };
    // SAFETY: c is a valid NUL-terminated C string.
    let ret = unsafe { nix::libc::swapon(c.as_ptr(), flags) };
    if ret == 0 {
        Ok(())
    } else {
        Err(format!("swapon failed: {}", std::io::Error::last_os_error()))
    }
}
```

- [ ] **Step 9.3: Build and run tests**

Run: `rtk cargo build 2>&1 | tail -15`
Expected: Clean build.

Run: `rtk cargo test create_swap:: 2>&1 | tail -30`
Expected: All `create_swap` tests PASS (8 from Task 2 + 9 new).

- [ ] **Step 9.4: Commit**

```bash
git add src/create_swap.rs
git commit -m "feat(phase5): implement create-swap background step runner"
```

---

## Task 10 — Update docs and run the full quality pass

**Files:**
- Modify: `docs/devices.md`

- [ ] **Step 10.1: Add the `n` row and update the tab range**

Edit `docs/devices.md`. In the keybindings table, right before the row `| `s` | Confirm action (when modal is open) |`, add:

```markdown
| `n`         | Create new swap file (modal wizard, requires root) |
```

In the same table, replace:

```markdown
| `Tab` / `1-4` | Switch tabs |
```

with:

```markdown
| `Tab` / `1-3` | Switch tabs |
```

Also in the opening paragraph, replace "press `3` or navigate with `Tab`" with itself — no change needed, but verify that nothing in the doc still mentions a 4th tab.

Run: `rtk grep -n "Tab 4\|CreateSwap\|1-4" docs/devices.md`
Expected: no output (or only a match inside a code fence that we just updated).

- [ ] **Step 10.2: Run the full quality gate**

Run: `rtk cargo build 2>&1 | tail -10`
Expected: Clean build, zero warnings.

Run: `rtk cargo clippy --all-targets -- -D warnings 2>&1 | tail -30`
Expected: No warnings.

Run: `rtk cargo test 2>&1 | tail -20`
Expected: All tests PASS.

- [ ] **Step 10.3: Commit docs**

```bash
git add docs/devices.md
git commit -m "docs(phase5): document n keybinding and 1-3 tab range"
```

---

## Task 11 — Manual smoke test

- [ ] **Step 11.1: Build release binary**

Run: `rtk cargo build --release`

- [ ] **Step 11.2: Non-root flow**

Run: `./target/release/swaptop`
- Press `3` → Devices tab appears.
- Press `n` → statusbar shows `Requires root — run: sudo swaptop`.
- Press `q` to quit.

- [ ] **Step 11.3: Root flow, new file**

Run: `sudo ./target/release/swaptop`
- Press `3`, press `n`. Modal opens on Path field.
- Type `/tmp/swaptest1`.
- Press `↓`, type `100` (replacing default `2`).
- Press `↓`, press `Space` → unit becomes `MB`.
- Press `↓`, `↓` (Activate toggles acceptable), `↓` to Submit.
- Press `Enter`. Watch 7 steps turn ✓. Modal auto-closes after 2s (if you add the delay — optional).
- Verify with `cat /proc/swaps` — `/tmp/swaptest1` should be listed.
- Back in the TUI, press `f` to swapoff, confirm. `rm /tmp/swaptest1` from another terminal.

- [ ] **Step 11.4: Root flow, already-a-swap file**

Create a swap file outside swaptop: `sudo dd if=/dev/zero of=/tmp/swaptest2 bs=1M count=64 && sudo chmod 600 /tmp/swaptest2 && sudo mkswap /tmp/swaptest2`
- In swaptop (root), press `n`, type `/tmp/swaptest2`, any size, submit.
- Modal should show `ConfirmActivateOnly` with the detected size.
- Press `s` → swapon runs. Verify via `cat /proc/swaps`.
- Press `f` + confirm to deactivate. `sudo rm /tmp/swaptest2`.

- [ ] **Step 11.5: Error flow**

Create a regular file: `echo hello > /tmp/not_a_swap && sudo chmod 600 /tmp/not_a_swap`
- `n`, path `/tmp/not_a_swap`, submit.
- Step 1 should show ✗ with "file exists and is not a swap file — refusing to overwrite".
- Press `Esc` → returns to Form (inputs preserved).
- Press `Esc` again → closes modal.
- `rm /tmp/not_a_swap`.

If any of these flows fails, diagnose the root cause and add a follow-up task. Do not mark the plan complete until all five flows pass.

---

## Self-review notes (for the implementer)

- **Spec coverage:**
  - Tab::CreateSwap removal → Task 4 (enum), Task 5 (UI), Task 6 (input), Task 10 (docs). ✓
  - tui-input → Task 1. ✓
  - Modal state machine (3 modes) → Task 4 (reducer) + Task 8 (UI). ✓
  - Pre-existence check with activate-only flow → Task 9 (`check_target_file`) + Task 4 (mode) + Task 8 (render). ✓
  - Filesystem detection → Task 9 (`detect_fs_type` + `allocator_for_fs`). ✓
  - 7-step sequence → Task 9. ✓
  - Form validation → Task 6 (`validate_and_submit`). ✓
  - Error recovery Progress → Form → Task 4 (`CreateSwapReturnToForm`) + Task 6 (Esc dispatch). ✓
  - docs/devices.md → Task 10. ✓
- **No placeholders:** every step above contains either code or an exact command.
- **Type consistency:** `CreateSwapField`, `CreateSwapMode`, `CreateSwapModal`, `StepStatus`, `SizeUnit`, `Allocator`, and all function signatures are defined once and referenced with matching spelling throughout.
- **Commit cadence:** 6 commits (one per task group: 1, 2, 3, 7, 8, 9, 10) keeps the history reviewable.
