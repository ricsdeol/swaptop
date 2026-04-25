# Pure Input + PlatformBridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Purify `resolve_key` into a true `(KeyEvent, KeyContext) -> Option<Action>` function with zero mutex locks, and extract all platform I/O into a dedicated `PlatformBridge` thread that communicates via channels.

**Architecture:** Phase 1 replaces `Arc<Mutex<AppState>>` in `KeyContext` with value-only sub-structs extracted via a single lock in `main.rs`. Phase 2 introduces `PlatformBridge` — a dedicated `std::thread` that owns the `SwapBackend`, receives `PlatformCommand`s, and sends `Action`s back through the existing `action_rx` channel.

**Tech Stack:** Rust, Ratatui, crossterm, tokio, nix, std::sync::mpsc

---

## File Map

### Phase 1 — Pure Input

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `src/app.rs` | Add `is_root` field; move validation into `CreateSwapSubmit` handler; auto-clear completions on form actions |
| Modify | `src/input.rs` | New owned `KeyContext` + sub-structs; `from_state` constructor; rewrite all functions to be pure; delete `validate_and_submit`; update tests |
| Modify | `src/main.rs` | Single-lock extraction via `KeyContext::from_state` |

### Phase 2 — PlatformBridge

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/platform_bridge.rs` | `PlatformCommand` enum, `PlatformBridge` struct, dedicated thread loop |
| Modify | `src/main.rs` | Remove `Collector`/`LinuxBackend` imports; use bridge for tick/device-op/create-swap |
| Delete | `src/collector.rs` | Absorbed into `PlatformBridge` |

---

## Task 1: Add `is_root` to AppState

**Files:**
- Modify: `src/app.rs:23-53` (struct + `new()`)
- Modify: `src/main.rs:33-35` (startup)

- [ ] **Step 1: Write failing test**

In `src/app.rs`, add to the test module:

```rust
#[test]
fn is_root_field_exists_on_appstate() {
    let state = AppState::new(make_caps());
    // We're running tests as non-root, so expect false
    assert!(!state.is_root);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test is_root_field_exists_on_appstate -- --nocapture`
Expected: FAIL — `is_root` field doesn't exist

- [ ] **Step 3: Add `is_root` field to AppState**

In `src/app.rs`, add field to `AppState` struct:

```rust
pub struct AppState {
    pub active_tab: Tab,
    pub ram_history: VecDeque<(Instant, u64)>,
    // ... existing fields ...
    pub confirm_off_delete: Option<ConfirmOffDelete>,
    pub is_root: bool,
}
```

Update `AppState::new()`:

```rust
impl AppState {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            // ... existing fields ...
            confirm_off_delete: None,
            is_root: nix::unistd::geteuid().is_root(),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test is_root_field_exists_on_appstate -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src/app.rs
git commit -m "refactor: add is_root field to AppState, computed once at startup"
```

---

## Task 2: Define new value-based KeyContext types

**Files:**
- Modify: `src/input.rs:1-18` (replace old KeyContext)

- [ ] **Step 1: Replace KeyContext and add sub-structs**

Replace the entire `KeyContext` struct and imports at the top of `src/input.rs` with:

```rust
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};
use crate::create_swap::{CreateSwapField, CreateSwapMode, SizeUnit, StepStatus};
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
    pub selected_dev: usize,
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
    pub size_value: String,
    pub priority_value: String,
    pub size_unit: SizeUnit,
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
            selected_dev: s.selected_dev,
            has_devices: !s.devices.is_empty(),
            confirm_action: s.confirm_action.clone(),
            selected_path: s.devices.get(s.selected_dev).map(|d| d.path.clone()),
            selected_active: s.devices.get(s.selected_dev).map(|d| d.active),
            selected_is_file: s
                .devices
                .get(s.selected_dev)
                .map(|d| matches!(d.kind, SwapKind::File)),
            confirm_off_delete: s.confirm_off_delete.as_ref().map(|c| {
                ConfirmOffDeleteContext {
                    path: c.path.clone(),
                    delete_file: c.delete_file,
                    active: c.active,
                }
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
                size_value: modal.size_input.value().to_string(),
                priority_value: modal.priority_input.value().to_string(),
                size_unit: modal.size_unit,
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
```

- [ ] **Step 2: Verify it compiles (will fail — callers not updated yet)**

Run: `cargo check 2>&1 | head -20`
Expected: Compilation errors from `resolve_key` and callers (expected — Task 3 fixes these)

---

## Task 3: Rewrite input functions to be pure

**Files:**
- Modify: `src/input.rs:20-454` (all four functions)

This task rewrites all four input functions simultaneously because they form a tightly coupled call chain: `resolve_key` → `handle_devices_key` / `handle_create_swap_key` → `handle_form_key` → (deleted) `validate_and_submit`.

- [ ] **Step 1: Replace `resolve_key`**

Replace `src/input.rs` function `resolve_key` (currently lines 20-87) with:

```rust
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
```

- [ ] **Step 2: Replace `handle_devices_key`**

Replace `src/input.rs` function `handle_devices_key` (currently lines 89-222) with:

```rust
fn handle_devices_key(
    code: KeyCode,
    dev: &DeviceContext,
    is_root: bool,
) -> Option<Action> {
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
```

- [ ] **Step 3: Replace `handle_create_swap_key`**

Replace `src/input.rs` function `handle_create_swap_key` (currently lines 224-349) with:

```rust
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
                    _ => {
                        handle_form_key(key, focused)
                            .or(Some(Action::CreateSwapClearCompletions))
                    }
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
```

- [ ] **Step 4: Replace `handle_form_key` and delete `validate_and_submit`**

Replace `src/input.rs` function `handle_form_key` (currently lines 351-407) with:

```rust
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
        KeyCode::Enter if focused == CreateSwapField::Submit => {
            Some(Action::CreateSwapSubmit {
                activate_only: false,
            })
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
```

Delete the entire `validate_and_submit` function (currently lines 409-454).

- [ ] **Step 5: Verify input.rs compiles in isolation**

Run: `cargo check 2>&1 | head -30`
Expected: Errors only from `main.rs` (caller not updated yet) and possibly tests — `input.rs` functions should compile

---

## Task 4: Move validation into reducer

**Files:**
- Modify: `src/app.rs:323-345` (`CreateSwapSubmit` handler)

- [ ] **Step 1: Write failing test for validation rejection**

Add to `src/app.rs` test module:

```rust
#[test]
fn create_swap_submit_rejects_empty_path() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    // path_input is empty by default
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: false,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.validation_error.is_some());
    assert!(
        matches!(modal.mode, CreateSwapMode::Form { .. }),
        "should stay in Form mode on validation failure"
    );
}

#[test]
fn create_swap_submit_rejects_relative_path() {
    use tui_input::Input;
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
    use tui_input::Input;
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
    state.create_swap_modal.as_mut().unwrap().size_input = Input::from("0");
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: false,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert_eq!(
        modal.validation_error.as_deref(),
        Some("Size must be greater than zero")
    );
}

#[test]
fn create_swap_submit_rejects_non_numeric_size() {
    use tui_input::Input;
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
    state.create_swap_modal.as_mut().unwrap().size_input = Input::from("abc");
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: false,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert_eq!(
        modal.validation_error.as_deref(),
        Some("Size must be a positive integer")
    );
}

#[test]
fn create_swap_submit_rejects_out_of_range_priority() {
    use tui_input::Input;
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
    state.create_swap_modal.as_mut().unwrap().priority_input = Input::from("99999");
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: false,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.validation_error.is_some());
}

#[test]
fn create_swap_submit_valid_transitions_to_progress() {
    use tui_input::Input;
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.create_swap_modal.as_mut().unwrap().path_input = Input::from("/swapfile");
    state.create_swap_modal.as_mut().unwrap().size_input = Input::from("2");
    state.create_swap_modal.as_mut().unwrap().priority_input = Input::from("0");
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: false,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.validation_error.is_none());
    assert!(matches!(modal.mode, CreateSwapMode::Progress { .. }));
}

#[test]
fn create_swap_submit_activate_only_skips_validation() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    // path_input is empty — would fail validation normally
    state.handle_action(Action::CreateSwapSubmit {
        activate_only: true,
    });
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.validation_error.is_none());
    assert!(matches!(modal.mode, CreateSwapMode::Progress { .. }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test create_swap_submit_rejects -- --nocapture 2>&1 | tail -20`
Expected: Tests like `create_swap_submit_rejects_empty_path` FAIL (validation not in reducer yet)

- [ ] **Step 3: Update `CreateSwapSubmit` handler in reducer**

Replace the `Action::CreateSwapSubmit` arm in `src/app.rs` `handle_action` (currently lines 323-345) with:

```rust
Action::CreateSwapSubmit { activate_only } => {
    if let Some(modal) = self.create_swap_modal.as_mut() {
        if !activate_only {
            let path = modal.path_input.value().trim().to_string();
            if path.is_empty() {
                modal.validation_error = Some("Path is required".to_string());
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
                modal.validation_error =
                    Some("Size must be greater than zero".to_string());
                return;
            }
            let prio_n: i32 = match modal.priority_input.value().trim().parse() {
                Ok(n) => n,
                Err(_) => {
                    modal.validation_error = Some(
                        "Priority must be an integer between -1 and 32767".to_string(),
                    );
                    return;
                }
            };
            if !(-1..=32767).contains(&prio_n) {
                modal.validation_error = Some(
                    "Priority must be an integer between -1 and 32767".to_string(),
                );
                return;
            }
        }
        modal.validation_error = None;
        let mut steps = vec![
            CreateSwapStep::pending("Check disk space"),
            CreateSwapStep::pending("Check target file"),
            CreateSwapStep::pending("Detect filesystem"),
            CreateSwapStep::pending("Allocate file"),
            CreateSwapStep::pending("chmod 600"),
            CreateSwapStep::pending("mkswap"),
            CreateSwapStep::pending("swapon"),
        ];
        if activate_only {
            for step in steps.iter_mut().take(6) {
                step.status = crate::create_swap::StepStatus::Done;
            }
        }
        modal.mode = CreateSwapMode::Progress { steps };
    }
}
```

- [ ] **Step 4: Run validation tests**

Run: `cargo test create_swap_submit_rejects -- --nocapture`
Run: `cargo test create_swap_submit_valid -- --nocapture`
Run: `cargo test create_swap_submit_activate_only -- --nocapture`
Expected: All PASS

---

## Task 5: Auto-clear completions in reducer

**Files:**
- Modify: `src/app.rs:284-309` (form action handlers)

- [ ] **Step 1: Write failing test**

Add to `src/app.rs` test module:

```rust
#[test]
fn form_input_event_clears_completions() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![
        "/a".to_string(),
        "/b".to_string(),
    ]));
    assert!(!state.create_swap_modal.as_ref().unwrap().completions.is_empty());
    // Any form input should clear completions
    state.handle_action(Action::CreateSwapToggleUnit);
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.completions.is_empty());
    assert_eq!(modal.completion_sel, None);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test form_input_event_clears_completions -- --nocapture`
Expected: FAIL — completions not cleared by toggle

- [ ] **Step 3: Add completion clearing to form action handlers**

In `src/app.rs`, add completion clearing at the top of these four action handlers:

For `CreateSwapInputEvent`, `CreateSwapFocusField`, `CreateSwapToggleUnit`, and `CreateSwapToggleActivate` — add these two lines at the start of each handler (inside the `if let Some(modal)` block, before existing logic):

```rust
modal.completions.clear();
modal.completion_sel = None;
```

For `CreateSwapInputEvent` (currently at line 292):

```rust
Action::CreateSwapInputEvent(event) => {
    if let Some(modal) = self.create_swap_modal.as_mut()
        && let CreateSwapMode::Form { focused_field } = modal.mode
    {
        modal.completions.clear();
        modal.completion_sel = None;
        use tui_input::backend::crossterm::EventHandler;
        // ... rest unchanged
    }
}
```

For `CreateSwapFocusField` (currently at line 284):

```rust
Action::CreateSwapFocusField(field) => {
    if let Some(modal) = self.create_swap_modal.as_mut()
        && let CreateSwapMode::Form { focused_field } = &mut modal.mode
    {
        modal.completions.clear();
        modal.completion_sel = None;
        *focused_field = field;
    }
}
```

For `CreateSwapToggleUnit` (currently at line 311):

```rust
Action::CreateSwapToggleUnit => {
    if let Some(modal) = self.create_swap_modal.as_mut() {
        modal.completions.clear();
        modal.completion_sel = None;
        modal.size_unit = modal.size_unit.toggled();
    }
}
```

For `CreateSwapToggleActivate` (currently at line 317):

```rust
Action::CreateSwapToggleActivate => {
    if let Some(modal) = self.create_swap_modal.as_mut() {
        modal.completions.clear();
        modal.completion_sel = None;
        modal.activate_after = !modal.activate_after;
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test form_input_event_clears_completions -- --nocapture`
Expected: PASS

---

## Task 6: Update main.rs event extraction

**Files:**
- Modify: `src/main.rs:107-131` (events arm)

- [ ] **Step 1: Replace multi-field extraction with `KeyContext::from_state`**

Replace the events arm in `src/main.rs` (currently lines 107-209) with:

```rust
Some(Ok(event)) = events.next().fuse() => {
    if let CrosstermEvent::Key(key) = event {
        let ctx = {
            let s = state.lock().expect("state mutex poisoned");
            input::KeyContext::from_state(&s)
        };

        let action = input::resolve_key(key, &ctx);

        // Extract bridge-relevant info before consuming action
        let device_op_cmd = if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
            Some((path.clone(), kind.clone()))
        } else {
            None
        };
        let submit_activate_only = if let Some(Action::CreateSwapSubmit { activate_only }) = &action {
            Some(*activate_only)
        } else {
            None
        };

        // Spawn background task for device ops
        if let Some((path, kind)) = device_op_cmd {
            let tx = action_tx.clone();
            tokio::task::spawn_blocking(move || {
                let backend = LinuxBackend::new();
                let result = match kind {
                    DeviceOpKind::On => backend.swap_on(&path),
                    DeviceOpKind::Off => backend.swap_off(&path),
                    DeviceOpKind::OffAndDelete => {
                        backend.swap_off(&path).and_then(|()| {
                            std::fs::remove_file(&path).map_err(|e| {
                                color_eyre::eyre::eyre!(
                                    "deactivated; delete failed: {e}"
                                )
                            })
                        })
                    }
                    DeviceOpKind::DeleteOnly => {
                        std::fs::remove_file(&path).map_err(|e| {
                            color_eyre::eyre::eyre!("delete failed: {e}")
                        })
                    }
                    DeviceOpKind::Reset => backend.swap_reset(&path),
                };
                let status = match result {
                    Ok(_) => OpStatus::Done,
                    Err(e) => OpStatus::Error(e.to_string()),
                };
                let _ = tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
            });
        }

        // Dispatch to reducer
        if let Some(a) = action {
            let mut s = state.lock().expect("state mutex poisoned");
            s.handle_action(a);

            // After CreateSwapSubmit: spawn background task only if
            // validation passed (mode transitioned to Progress)
            if submit_activate_only.is_some() {
                if let Some(modal) = s.create_swap_modal.as_ref() {
                    if matches!(modal.mode, CreateSwapMode::Progress { .. }) {
                        let activate_only = submit_activate_only.unwrap();
                        let size_n: u64 = modal
                            .size_input
                            .value()
                            .trim()
                            .parse()
                            .expect("validated by reducer");
                        let size_bytes = size_n * modal.size_unit.multiplier();
                        let prio_n: i32 = modal
                            .priority_input
                            .value()
                            .trim()
                            .parse()
                            .expect("validated by reducer");
                        let prio_i16 =
                            prio_n.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                        let path = std::path::PathBuf::from(modal.path_input.value());
                        let activate_after = modal.activate_after;
                        let tx = action_tx.clone();
                        tokio::task::spawn_blocking(move || {
                            run_create_swap_steps(
                                path,
                                size_bytes,
                                prio_i16,
                                activate_after,
                                activate_only,
                                tx,
                            );
                        });
                    }
                }
            }

            processes_active.store(
                s.active_tab == Tab::Processes,
                Ordering::Relaxed,
            );
        }
    }
}
```

- [ ] **Step 2: Remove unused imports from main.rs**

Remove `use std::sync::Arc` duplication if any. The `use crate::create_swap::CreateSwapMode` import is now needed — add it to the imports at the top of `main.rs`:

```rust
use create_swap::CreateSwapMode;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check`
Expected: Compiles (tests may still fail — updated in Task 7)

---

## Task 7: Update input.rs tests

**Files:**
- Modify: `src/input.rs` test module (currently lines 503-907)

- [ ] **Step 1: Replace test helpers and imports**

Replace the entire test module preamble (imports, helpers) with:

```rust
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

    fn default_device() -> DeviceContext {
        DeviceContext {
            selected_dev: 0,
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
        sel: usize,
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
                    selected_dev: sel,
                    has_devices: devs,
                    confirm_action: confirm,
                    selected_path: if devs {
                        Some("/dev/sda2".into())
                    } else {
                        None
                    },
                    selected_active: None,
                    selected_is_file: None,
                    confirm_off_delete: None,
                },
                create_swap: None,
            },
        )
    }
```

- [ ] **Step 2: Update filter mode tests**

Remove `let state = make_state();` from all filter mode tests. Update `rk()` calls to remove the `&state` argument and use owned types:

```rust
#[test]
fn filter_mode_captures_printable_chars() {
    let action = rk(
        key(KeyCode::Char('a')),
        Tab::Processes,
        None,
        0,
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
        0,
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
        0,
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
        0,
        false,
        true,
        SortColumn::Swap,
    );
    assert!(matches!(action, Some(Action::FilterBackspace)));
}
```

- [ ] **Step 3: Update global key tests**

```rust
#[test]
fn global_quit_keys_work_from_any_tab() {
    for tab in [Tab::Overview, Tab::Processes, Tab::Devices] {
        let q = rk(
            key(KeyCode::Char('q')),
            tab.clone(),
            None,
            0,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(
            matches!(q, Some(Action::Quit)),
            "q should quit from {tab:?}"
        );

        let ctrl_c = rk(
            ctrl('c'),
            tab,
            None,
            0,
            false,
            false,
            SortColumn::Swap,
        );
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
        0,
        false,
        false,
        SortColumn::Swap,
    );
    assert!(matches!(fwd, Some(Action::NextTab)));

    let back = rk(
        key(KeyCode::BackTab),
        Tab::Overview,
        None,
        0,
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
            0,
            false,
            false,
            SortColumn::Swap,
        );
        assert!(matches!(action, Some(Action::SelectTab(v)) if v == n));
    }
}
```

- [ ] **Step 4: Update tab-specific key tests**

```rust
#[test]
fn process_tab_keys_only_fire_on_process_tab() {
    let on_proc = rk(
        key(KeyCode::Char('j')),
        Tab::Processes,
        None,
        0,
        false,
        false,
        SortColumn::Swap,
    );
    assert!(matches!(on_proc, Some(Action::NavigateDown)));

    let on_overview = rk(
        key(KeyCode::Char('j')),
        Tab::Overview,
        None,
        0,
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
        0,
        false,
        false,
        SortColumn::Swap,
    );
    assert!(matches!(action, Some(Action::EnterFilterMode)));
}
```

- [ ] **Step 5: Update ConfirmOffDelete tests**

Replace `make_off_delete_state` and its tests:

```rust
fn make_off_delete_ctx(active: bool, delete_file: bool) -> KeyContext {
    KeyContext {
        active_tab: Tab::Devices,
        filter_mode: false,
        sort_col: SortColumn::Swap,
        is_root: false,
        device: DeviceContext {
            selected_dev: 0,
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
```

- [ ] **Step 6: Update remaining tests**

```rust
#[test]
fn unknown_key_returns_none() {
    let action = rk(
        key(KeyCode::F(5)),
        Tab::Overview,
        None,
        0,
        false,
        false,
        SortColumn::Swap,
    );
    assert!(action.is_none());
}
```

Keep the `completions_*` and `sort_column_*` tests as-is (they don't use `rk()` or state).

- [ ] **Step 7: Remove unused imports from test module**

Delete these lines from the test module:
```rust
use crate::app::{AppState, Tab};
use crate::platform::Capabilities;
use std::sync::{Arc, Mutex};
```

Replace with (if not already imported from `super::*`):
```rust
use crate::app::Tab;
```

Remove `make_caps()`, `make_state()` functions.

- [ ] **Step 8: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 9: Run clippy and format**

Run: `cargo clippy -- -D warnings`
Run: `cargo fmt --check`
Expected: Clean

- [ ] **Step 10: Commit Phase 1**

```bash
git add src/input.rs src/app.rs src/main.rs
git commit -m "refactor: purify resolve_key — zero mutex locks in input resolver

Remove Arc<Mutex<AppState>> from KeyContext. All state extracted via a
single lock in main.rs through KeyContext::from_state(). Validation
moved to reducer (CreateSwapSubmit validates inline). Completions
auto-cleared by reducer on form actions. No runtime behavior change."
```

---

## Task 8: Create PlatformBridge

**Files:**
- Create: `src/platform_bridge.rs`

- [ ] **Step 1: Create the file with PlatformCommand and PlatformBridge**

Create `src/platform_bridge.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::mpsc::UnboundedSender;

use crate::actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use crate::platform::{MemSnapshot, SwapBackend};

pub enum PlatformCommand {
    Collect,
    DeviceOp { path: PathBuf, kind: DeviceOpKind },
    CreateSwap {
        path: PathBuf,
        size_bytes: u64,
        priority: i16,
        activate_after: bool,
        activate_only: bool,
    },
    Shutdown,
}

pub struct PlatformBridge {
    cmd_tx: std::sync::mpsc::Sender<PlatformCommand>,
}

impl PlatformBridge {
    pub fn spawn_with_backend(
        mut backend: Box<dyn SwapBackend>,
        action_tx: UnboundedSender<Action>,
        processes_active: Arc<AtomicBool>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    PlatformCommand::Collect => {
                        Self::handle_collect(
                            &mut *backend,
                            &action_tx,
                            &processes_active,
                        );
                    }
                    PlatformCommand::DeviceOp { path, kind } => {
                        Self::handle_device_op(&*backend, &action_tx, path, kind);
                    }
                    PlatformCommand::CreateSwap {
                        path,
                        size_bytes,
                        priority,
                        activate_after,
                        activate_only,
                    } => {
                        crate::platform::linux::create_swap::run_create_swap_steps(
                            path,
                            size_bytes,
                            priority,
                            activate_after,
                            activate_only,
                            action_tx.clone(),
                        );
                    }
                    PlatformCommand::Shutdown => break,
                }
            }
        });
        Self { cmd_tx }
    }

    pub fn send(&self, cmd: PlatformCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn handle_collect(
        backend: &mut dyn SwapBackend,
        action_tx: &UnboundedSender<Action>,
        processes_active: &AtomicBool,
    ) {
        let result: color_eyre::Result<MemSnapshot> = (|| {
            let ram = backend.system_ram()?;
            let swap = backend.system_swap()?;
            let devices = backend.swap_devices()?;
            let processes = if processes_active.load(Ordering::Relaxed) {
                backend.process_list()?
            } else {
                vec![]
            };
            Ok(MemSnapshot {
                timestamp: std::time::Instant::now(),
                ram,
                swap,
                devices,
                processes,
            })
        })();
        match result {
            Ok(snap) => {
                let _ = action_tx.send(Action::UpdateSnapshot(snap));
            }
            Err(e) => {
                let _ = action_tx.send(Action::SetError(e.to_string()));
            }
        }
    }

    fn handle_device_op(
        backend: &dyn SwapBackend,
        action_tx: &UnboundedSender<Action>,
        path: PathBuf,
        kind: DeviceOpKind,
    ) {
        let result = match kind {
            DeviceOpKind::On => backend.swap_on(&path),
            DeviceOpKind::Off => backend.swap_off(&path),
            DeviceOpKind::OffAndDelete => backend.swap_off(&path).and_then(|()| {
                std::fs::remove_file(&path).map_err(|e| {
                    color_eyre::eyre::eyre!("deactivated; delete failed: {e}")
                })
            }),
            DeviceOpKind::DeleteOnly => std::fs::remove_file(&path)
                .map_err(|e| color_eyre::eyre::eyre!("delete failed: {e}")),
            DeviceOpKind::Reset => backend.swap_reset(&path),
        };
        let status = match result {
            Ok(_) => OpStatus::Done,
            Err(e) => OpStatus::Error(e.to_string()),
        };
        let _ = action_tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    struct MockBackend {
        ram: SwapInfo,
        swap: SwapInfo,
        devices: Vec<SwapDevice>,
        processes: Vec<ProcessRow>,
        fail: bool,
    }

    impl MockBackend {
        fn healthy() -> Self {
            Self {
                ram: SwapInfo::new(16_000_000, 8_000_000),
                swap: SwapInfo::new(4_000_000, 1_000_000),
                devices: vec![],
                processes: vec![ProcessRow {
                    pid: 1,
                    name: "init".into(),
                    user: "root".into(),
                    rss: 1024,
                    swap: 512,
                    cpu_pct: 0.5,
                }],
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::healthy()
            }
        }
    }

    impl SwapBackend for MockBackend {
        fn system_ram(&mut self) -> color_eyre::Result<SwapInfo> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock ram error"));
            }
            Ok(self.ram.clone())
        }
        fn system_swap(&mut self) -> color_eyre::Result<SwapInfo> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock swap error"));
            }
            Ok(self.swap.clone())
        }
        fn swap_devices(&mut self) -> color_eyre::Result<Vec<SwapDevice>> {
            if self.fail {
                return Err(color_eyre::eyre::eyre!("mock devices error"));
            }
            Ok(self.devices.clone())
        }
        fn process_list(&mut self) -> color_eyre::Result<Vec<ProcessRow>> {
            Ok(self.processes.clone())
        }
        fn swap_on(&self, _device: &Path) -> color_eyre::Result<()> {
            Ok(())
        }
        fn swap_off(&self, _device: &Path) -> color_eyre::Result<()> {
            Ok(())
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                can_swap_on: true,
                has_per_process: true,
            }
        }
    }

    fn recv_action(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Action>) -> Action {
        rx.blocking_recv().expect("channel closed before action received")
    }

    #[test]
    fn collect_sends_update_snapshot() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        assert!(matches!(action, Action::UpdateSnapshot(_)));
    }

    #[test]
    fn collect_includes_processes_when_active() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(true));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        if let Action::UpdateSnapshot(snap) = action {
            assert_eq!(snap.processes.len(), 1);
        } else {
            panic!("expected UpdateSnapshot");
        }
    }

    #[test]
    fn collect_skips_processes_when_inactive() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        if let Action::UpdateSnapshot(snap) = action {
            assert!(snap.processes.is_empty());
        } else {
            panic!("expected UpdateSnapshot");
        }
    }

    #[test]
    fn collect_error_sends_set_error() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::failing()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::Collect);
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        assert!(matches!(action, Action::SetError(_)));
    }

    #[test]
    fn device_op_sends_update() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::DeviceOp {
            path: "/dev/sda2".into(),
            kind: DeviceOpKind::On,
        });
        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);

        let action = recv_action(&mut action_rx);
        if let Action::DeviceOpUpdate(op) = action {
            assert_eq!(op.status, OpStatus::Done);
            assert_eq!(op.kind, DeviceOpKind::On);
        } else {
            panic!("expected DeviceOpUpdate, got {action:?}");
        }
    }

    #[test]
    fn shutdown_exits_thread() {
        let (action_tx, _action_rx) = tokio::sync::mpsc::unbounded_channel();
        let processes_active = Arc::new(AtomicBool::new(false));
        let bridge = PlatformBridge::spawn_with_backend(
            Box::new(MockBackend::healthy()),
            action_tx,
            processes_active,
        );

        bridge.send(PlatformCommand::Shutdown);
        drop(bridge);
        // Channel closed — no panic, no hang
    }
}
```

- [ ] **Step 2: Register module in main.rs**

Add to `src/main.rs` module declarations (after `mod input;`):

```rust
mod platform_bridge;
```

- [ ] **Step 3: Verify compilation and tests**

Run: `cargo test platform_bridge -- --nocapture`
Expected: All bridge tests pass

---

## Task 9: Wire PlatformBridge into main.rs

**Files:**
- Modify: `src/main.rs` (replace Collector, remove LinuxBackend, use bridge)

- [ ] **Step 1: Update imports**

Replace imports at top of `src/main.rs`:

```rust
// Remove these:
use collector::Collector;
use platform::linux::LinuxBackend;
use platform::linux::create_swap::run_create_swap_steps;

// Add these:
use platform_bridge::{PlatformBridge, PlatformCommand};
```

Keep existing: `use platform::SwapBackend;`, `use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};`

Also add:

```rust
use create_swap::CreateSwapMode;
use platform::MemSnapshot;
```

- [ ] **Step 2: Update `main()` function for initial collection + bridge**

Replace `main()` body (currently lines 30-66) with:

```rust
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let mut backend = platform::factory::detect();
    let caps = backend.capabilities();
    let state = Arc::new(Mutex::new(AppState::new(caps)));
    let processes_active = Arc::new(AtomicBool::new(false));

    // Initial collection before entering the TUI so the first frame is not blank.
    {
        let ram = backend.system_ram()?;
        let swap = backend.system_swap()?;
        let devices = backend.swap_devices()?;
        let snap = MemSnapshot {
            timestamp: Instant::now(),
            ram,
            swap,
            devices,
            processes: vec![],
        };
        state
            .lock()
            .expect("state mutex poisoned")
            .handle_action(Action::UpdateSnapshot(snap));
    }

    let mut terminal = tui::init()?;

    let shutdown = CancellationToken::new();
    {
        let token = shutdown.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                token.cancel();
            }
        });
    }

    let (action_tx, action_rx) = mpsc::unbounded_channel::<Action>();
    let bridge = PlatformBridge::spawn_with_backend(
        backend,
        action_tx.clone(),
        Arc::clone(&processes_active),
    );

    // Keep action_tx alive for the duration of run() so the channel stays open.
    let _action_tx = action_tx;
    let result = run(
        &mut terminal,
        state,
        &bridge,
        processes_active,
        shutdown,
        action_rx,
    )
    .await;
    bridge.send(PlatformCommand::Shutdown);
    tui::restore()?;
    result
}
```

- [ ] **Step 3: Update `run()` signature and body**

Replace entire `run()` function:

```rust
async fn run(
    terminal: &mut tui::Tui,
    state: Arc<Mutex<AppState>>,
    bridge: &PlatformBridge,
    processes_active: Arc<AtomicBool>,
    shutdown: CancellationToken,
    mut action_rx: mpsc::UnboundedReceiver<Action>,
) -> Result<()> {
    let mut tick = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events = EventStream::new();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            Some(action) = action_rx.recv() => {
                state.lock().expect("state mutex poisoned").handle_action(action);
            }

            _ = tick.tick() => {
                bridge.send(PlatformCommand::Collect);
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    let ctx = {
                        let s = state.lock().expect("state mutex poisoned");
                        input::KeyContext::from_state(&s)
                    };

                    let action = input::resolve_key(key, &ctx);

                    // Extract info before consuming action
                    let device_op_cmd = if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        Some((path.clone(), kind.clone()))
                    } else {
                        None
                    };
                    let submit_activate_only = if let Some(Action::CreateSwapSubmit { activate_only }) = &action {
                        Some(*activate_only)
                    } else {
                        None
                    };

                    // Send device op to bridge
                    if let Some((path, kind)) = device_op_cmd {
                        bridge.send(PlatformCommand::DeviceOp { path, kind });
                    }

                    // Dispatch to reducer
                    if let Some(a) = action {
                        let mut s = state.lock().expect("state mutex poisoned");
                        s.handle_action(a);

                        // After CreateSwapSubmit: send to bridge only if
                        // validation passed (mode transitioned to Progress)
                        if let Some(activate_only) = submit_activate_only {
                            if let Some(modal) = s.create_swap_modal.as_ref() {
                                if matches!(modal.mode, CreateSwapMode::Progress { .. }) {
                                    let size_n: u64 =
                                        modal.size_input.value().trim().parse().unwrap_or(0);
                                    let size_bytes = size_n * modal.size_unit.multiplier();
                                    let prio_n: i32 = modal
                                        .priority_input
                                        .value()
                                        .trim()
                                        .parse()
                                        .unwrap_or(0);
                                    let prio_i16 =
                                        prio_n.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
                                    bridge.send(PlatformCommand::CreateSwap {
                                        path: std::path::PathBuf::from(
                                            modal.path_input.value(),
                                        ),
                                        size_bytes,
                                        priority: prio_i16,
                                        activate_after: modal.activate_after,
                                        activate_only,
                                    });
                                }
                            }
                        }

                        processes_active.store(
                            s.active_tab == Tab::Processes,
                            Ordering::Relaxed,
                        );
                    }
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: Compiles clean (may warn about unused `action_tx` — it's used by bridge internally now; or about `Collector` module)

---

## Task 10: Delete collector.rs

**Files:**
- Delete: `src/collector.rs`
- Modify: `src/main.rs:15` (remove `mod collector`)

- [ ] **Step 1: Remove module declaration**

In `src/main.rs`, remove the line:

```rust
mod collector;
```

- [ ] **Step 2: Delete the file**

```bash
rm src/collector.rs
```

- [ ] **Step 3: Clean up any remaining unused imports in main.rs**

Remove any imports that are no longer used after the refactor. The following should remain:

```rust
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod create_swap;
mod input;
mod platform;
mod platform_bridge;
mod tui;
mod ui;

use actions::Action;
use app::{AppState, Tab};
use create_swap::CreateSwapMode;
use platform::MemSnapshot;
use platform::SwapBackend;
use platform_bridge::{PlatformBridge, PlatformCommand};
```

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Run clippy and format**

Run: `cargo clippy -- -D warnings`
Run: `cargo fmt --check`
Expected: Clean

- [ ] **Step 6: Commit Phase 2**

```bash
git add src/platform_bridge.rs src/main.rs
git rm src/collector.rs
git commit -m "refactor: introduce PlatformBridge — dedicated thread for all platform I/O

All backend calls (collect, device ops, create swap) run on a dedicated
std::thread owned by PlatformBridge. Tokio executor is never blocked.
Collector module absorbed into PlatformBridge. LinuxBackend import
removed from main.rs. Communication via std::sync::mpsc (commands) and
tokio::sync::mpsc (actions)."
```

---

## Verification Checklist

After all tasks complete:

- [ ] `cargo build` — zero warnings
- [ ] `cargo clippy -- -D warnings` — clean
- [ ] `cargo fmt --check` — formatted
- [ ] `cargo test` — all tests pass
- [ ] `resolve_key` has zero `state.lock()` calls
- [ ] `main.rs` has zero `platform::linux::*` imports
- [ ] `collector.rs` is deleted
- [ ] `validate_and_submit` is deleted
- [ ] Tick arm in main loop is non-blocking (`bridge.send`)
- [ ] Device ops go through bridge, not `spawn_blocking`
- [ ] Create swap goes through bridge after reducer validation
