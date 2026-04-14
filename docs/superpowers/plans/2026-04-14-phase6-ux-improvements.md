# Phase 6 — UX Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve the create-swap modal and device listing with cursor visibility, path autocomplete, delete-file-on-swapoff, and inactive swap discovery.

**Architecture:** Four independent features sharing one commit history. Each task compiles and tests independently. No new crate dependencies — everything builds on `std::fs`, `nix`, `tui_input`, and `ratatui` already present.

**Tech Stack:** Rust 2021 · Ratatui 0.30 · crossterm 0.29 · tui-input 0.15 · nix 0.31 · tokio 1.51

**Validation after every task:**
```bash
rtk cargo build
rtk cargo clippy -- -D warnings
rtk cargo test
cargo fmt --check
```

---

## Task 1: Cursor visibility in text fields

**Files:**
- Modify: `src/ui/create_swap.rs:41-77` (`render_form` function)

This task adds the real terminal cursor (`f.set_cursor_position`) to whichever
text field (Path, Size, Priority) is currently focused in the create-swap form.

- [ ] **Step 1: Write the test for cursor column computation**

Add a helper function `cursor_visual_col` and its test at the bottom of `src/ui/create_swap.rs`:

```rust
// In the main module (not inside tests), add:
fn cursor_visual_col(value: &str, byte_cursor: usize) -> u16 {
    value[..byte_cursor].chars().count() as u16
}

// In #[cfg(test)] mod tests:
#[test]
fn cursor_visual_col_ascii() {
    assert_eq!(cursor_visual_col("/var/swap", 4), 4);
    assert_eq!(cursor_visual_col("/var/swap", 0), 0);
    assert_eq!(cursor_visual_col("/var/swap", 9), 9);
}

#[test]
fn cursor_visual_col_empty() {
    assert_eq!(cursor_visual_col("", 0), 0);
}
```

- [ ] **Step 2: Run tests — expect PASS (helper is pure)**

```bash
rtk cargo test cursor_visual_col
```

- [ ] **Step 3: Add cursor placement to `render_form`**

In `src/ui/create_swap.rs`, inside `render_form`, after all `f.render_widget`
calls and before the closing `}`, add the cursor placement block:

```rust
    // Place the real terminal cursor on the focused text field.
    let cursor_input: Option<(&tui_input::Input, u16)> = match focused {
        CreateSwapField::Path => Some((&modal.path_input, rows[0].y)),
        CreateSwapField::Size => Some((&modal.size_input, rows[1].y)),
        CreateSwapField::Priority => Some((&modal.priority_input, rows[2].y)),
        _ => None,
    };
    if let Some((input, row_y)) = cursor_input {
        // label is 9 chars ("Path:    "), then "[", then visual cursor offset
        let label_width = 9_u16 + 1; // label + space
        let bracket = 1_u16; // opening "["
        let vis_col = cursor_visual_col(input.value(), input.cursor());
        let cursor_x = inner.x + label_width + bracket + vis_col;
        f.set_cursor_position((cursor_x, row_y));
    }
```

The label widths are all 9 chars (`"Path:    "`, `"Size:    "`, `"Priority:"`)
and the value_span starts with `"["`, so `label_width + bracket` = 11.

- [ ] **Step 4: Run full test suite + clippy**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 5: Commit**

```bash
git add src/ui/create_swap.rs
git commit -m "feat(phase6): show terminal cursor on focused text field in create-swap form"
```

---

## Task 2: Autocomplete state — new fields, actions, and reducer

**Files:**
- Modify: `src/create_swap.rs:119-143` (add fields to `CreateSwapModal`, update `Default`)
- Modify: `src/actions.rs:80-91` (add 4 new Action variants)
- Modify: `src/app.rs` (add reducer arms for new actions)

This task adds the completion data model and pure reducer logic. No input
handling or rendering yet.

- [ ] **Step 1: Add fields to `CreateSwapModal` and update Default**

In `src/create_swap.rs`, add two fields to `CreateSwapModal`:

```rust
pub struct CreateSwapModal {
    pub mode: CreateSwapMode,
    pub path_input: tui_input::Input,
    pub size_input: tui_input::Input,
    pub priority_input: tui_input::Input,
    pub size_unit: SizeUnit,
    pub activate_after: bool,
    pub validation_error: Option<String>,
    // Phase 6 — path autocomplete
    pub completions: Vec<String>,
    pub completion_sel: Option<usize>,
}
```

Update `Default`:

```rust
impl Default for CreateSwapModal {
    fn default() -> Self {
        Self {
            // ... existing fields unchanged ...
            completions: Vec::new(),
            completion_sel: None,
        }
    }
}
```

- [ ] **Step 2: Add Action variants**

In `src/actions.rs`, after the existing Phase 5 actions, add:

```rust
    // Phase 6 — path autocomplete
    CreateSwapSetCompletions(Vec<String>),
    CreateSwapCompletionMove(i16),
    CreateSwapApplyCompletion,
    CreateSwapClearCompletions,
```

- [ ] **Step 3: Write reducer tests**

In `src/app.rs` test module, add:

```rust
#[test]
fn set_completions_stores_and_selects_first() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![
        "/swapfile".to_string(),
        "/swap.img".to_string(),
    ]));
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert_eq!(modal.completions.len(), 2);
    assert_eq!(modal.completion_sel, Some(0));
}

#[test]
fn set_completions_empty_sets_sel_none() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![]));
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.completions.is_empty());
    assert_eq!(modal.completion_sel, None);
}

#[test]
fn completion_move_wraps_forward() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
    ]));
    state.handle_action(Action::CreateSwapCompletionMove(1));
    assert_eq!(state.create_swap_modal.as_ref().unwrap().completion_sel, Some(1));
    state.handle_action(Action::CreateSwapCompletionMove(1));
    assert_eq!(state.create_swap_modal.as_ref().unwrap().completion_sel, Some(2));
    state.handle_action(Action::CreateSwapCompletionMove(1));
    assert_eq!(state.create_swap_modal.as_ref().unwrap().completion_sel, Some(0)); // wrap
}

#[test]
fn completion_move_wraps_backward() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![
        "a".to_string(),
        "b".to_string(),
    ]));
    state.handle_action(Action::CreateSwapCompletionMove(-1));
    assert_eq!(state.create_swap_modal.as_ref().unwrap().completion_sel, Some(1)); // wrap
}

#[test]
fn apply_completion_sets_path_and_clears() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec![
        "/swapfile".to_string(),
        "/swap.img".to_string(),
    ]));
    state.handle_action(Action::CreateSwapCompletionMove(1)); // select /swap.img
    state.handle_action(Action::CreateSwapApplyCompletion);
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert_eq!(modal.path_input.value(), "/swap.img");
    assert!(modal.completions.is_empty());
    assert_eq!(modal.completion_sel, None);
}

#[test]
fn clear_completions_resets_state() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::OpenCreateSwap);
    state.handle_action(Action::CreateSwapSetCompletions(vec!["x".to_string()]));
    state.handle_action(Action::CreateSwapClearCompletions);
    let modal = state.create_swap_modal.as_ref().unwrap();
    assert!(modal.completions.is_empty());
    assert_eq!(modal.completion_sel, None);
}
```

- [ ] **Step 4: Run tests — expect FAIL (reducer arms missing)**

```bash
rtk cargo test set_completions_stores
```

- [ ] **Step 5: Implement reducer arms**

In `src/app.rs`, inside `handle_action`, add arms for the 4 new actions:

```rust
Action::CreateSwapSetCompletions(items) => {
    if let Some(ref mut modal) = self.create_swap_modal {
        modal.completion_sel = if items.is_empty() { None } else { Some(0) };
        modal.completions = items;
    }
}

Action::CreateSwapCompletionMove(delta) => {
    if let Some(ref mut modal) = self.create_swap_modal {
        if !modal.completions.is_empty() {
            let len = modal.completions.len() as i16;
            let cur = modal.completion_sel.unwrap_or(0) as i16;
            let next = ((cur + delta) % len + len) % len;
            modal.completion_sel = Some(next as usize);
        }
    }
}

Action::CreateSwapApplyCompletion => {
    if let Some(ref mut modal) = self.create_swap_modal {
        if let Some(sel) = modal.completion_sel {
            if let Some(value) = modal.completions.get(sel).cloned() {
                modal.path_input = tui_input::Input::from(value);
            }
        }
        modal.completions.clear();
        modal.completion_sel = None;
    }
}

Action::CreateSwapClearCompletions => {
    if let Some(ref mut modal) = self.create_swap_modal {
        modal.completions.clear();
        modal.completion_sel = None;
    }
}
```

- [ ] **Step 6: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 7: Commit**

```bash
git add src/create_swap.rs src/actions.rs src/app.rs
git commit -m "feat(phase6): add autocomplete state, actions, and reducer logic"
```

---

## Task 3: Autocomplete computation and key handling

**Files:**
- Modify: `src/input.rs` (add `compute_path_completions` + key handling in `handle_create_swap_key`)

This task wires Tab, Up/Down, Enter, and Esc to the autocomplete actions.

- [ ] **Step 1: Write tests for `compute_path_completions`**

In `src/input.rs` test module, add:

```rust
#[test]
fn completions_for_root_contains_entries() {
    // /tmp always exists on Linux
    let results = compute_path_completions("/tmp");
    // Should find /tmp/ at minimum (since /tmp is a directory, it should include /tmp/)
    // Or it might find /tmpfiles.d etc. Just verify it doesn't panic and returns a vec.
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
    // All entries should start with /
    assert!(results.iter().all(|p| p.starts_with('/')));
}

#[test]
fn completions_dirs_end_with_slash() {
    let results = compute_path_completions("/");
    // At least one directory should exist in root
    let dirs: Vec<_> = results.iter().filter(|p| p.ends_with('/')).collect();
    assert!(!dirs.is_empty(), "root should contain at least one directory");
}
```

- [ ] **Step 2: Implement `compute_path_completions`**

Add this function in `src/input.rs` (outside any test module):

```rust
fn compute_path_completions(partial: &str) -> Vec<String> {
    let path = std::path::Path::new(partial);
    let (dir, prefix) = if partial.ends_with('/') {
        (path, "")
    } else {
        (
            path.parent().unwrap_or(std::path::Path::new("/")),
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""),
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
```

- [ ] **Step 3: Run completion tests**

```bash
rtk cargo test completions_for
```

- [ ] **Step 4: Wire key handling in `handle_create_swap_key`**

In `src/input.rs`, modify `handle_create_swap_key`. The state extraction at
the top must also read `completions_showing`:

```rust
let (mode_variant, focused_field, path_value, size_value, priority_value, size_unit, completions_showing) = {
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
```

Then in the `"form"` match arm, add completion-aware key handling **before**
the existing match on `key.code`:

```rust
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
                // Any other key: clear completions, then forward to normal handler.
                // We use a two-step approach: dispatch clear, let next frame handle the char.
                // But since we can only return one action, we clear inline and re-dispatch.
                {
                    let mut s = state.lock().expect("state mutex poisoned");
                    if let Some(m) = s.create_swap_modal.as_mut() {
                        m.completions.clear();
                        m.completion_sel = None;
                    }
                }
                // Fall through to normal form key handling below
                handle_form_key(key, focused, state, &path_value, &size_value, &priority_value, size_unit)
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

    handle_form_key(key, focused, state, &path_value, &size_value, &priority_value, size_unit)
}
```

Extract the existing form key handling into a separate function `handle_form_key`
to avoid deep nesting. Move the existing `match key.code { ... }` block that
is currently inside `"form" => { ... }` into:

```rust
fn handle_form_key(
    key: KeyEvent,
    focused: CreateSwapField,
    state: &Arc<Mutex<AppState>>,
    path_value: &str,
    size_value: &str,
    priority_value: &str,
    size_unit: crate::create_swap::SizeUnit,
) -> Option<Action> {
    use crate::create_swap::CreateSwapField;
    match key.code {
        KeyCode::Esc => Some(Action::CloseCreateSwap),
        // ... (move all existing match arms here unchanged)
    }
}
```

- [ ] **Step 5: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 6: Commit**

```bash
git add src/input.rs
git commit -m "feat(phase6): add path autocomplete computation and key handling"
```

---

## Task 4: Autocomplete popup rendering

**Files:**
- Modify: `src/ui/create_swap.rs` (add popup overlay in `render_form`)

- [ ] **Step 1: Add `render_completions_popup` function**

In `src/ui/create_swap.rs`, add after `render_confirm_activate`:

```rust
fn render_completions_popup(f: &mut Frame, anchor: Rect, completions: &[String], sel: Option<usize>) {
    if completions.is_empty() {
        return;
    }
    let visible = completions.len().min(6);
    let popup_width = 32_u16; // matches value span width
    let popup_height = visible as u16 + 2; // +2 for border
    let popup_x = anchor.x;
    let popup_y = anchor.y + 1; // directly below the path row
    let popup_rect = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    f.render_widget(Clear, popup_rect);

    let items: Vec<Line> = completions
        .iter()
        .take(visible)
        .enumerate()
        .map(|(i, path)| {
            let style = if Some(i) == sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            // Truncate to fit within popup width minus borders
            let max_chars = (popup_width - 2) as usize;
            let display: String = if path.len() > max_chars {
                format!("..{}", &path[path.len() - max_chars + 2..])
            } else {
                path.clone()
            };
            Line::styled(display, style)
        })
        .collect();

    f.render_widget(Paragraph::new(items).block(block), popup_rect);
}
```

- [ ] **Step 2: Call the popup from `render_form`**

In `render_form`, after the cursor placement block (added in Task 1) and
before the closing `}`, add:

```rust
    // Render autocomplete popup if completions are showing.
    if !modal.completions.is_empty() {
        // Anchor = the Path value span area (row 0), offset for label
        let popup_anchor = Rect::new(inner.x + 10, rows[0].y, 32, 1);
        render_completions_popup(f, popup_anchor, &modal.completions, modal.completion_sel);
    }
```

- [ ] **Step 3: Write test for popup truncation**

```rust
#[test]
fn completion_display_truncates_long_paths() {
    let long_path = "/very/long/path/that/exceeds/thirty/characters/swapfile";
    let max_chars = 30_usize;
    let display: String = if long_path.len() > max_chars {
        format!("..{}", &long_path[long_path.len() - max_chars + 2..])
    } else {
        long_path.to_string()
    };
    assert!(display.len() <= max_chars);
    assert!(display.starts_with(".."));
}
```

- [ ] **Step 4: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 5: Commit**

```bash
git add src/ui/create_swap.rs
git commit -m "feat(phase6): render autocomplete popup below path field"
```

---

## Task 5: Delete-file-on-swapoff — types, actions, and reducer

**Files:**
- Modify: `src/actions.rs` (add `OffAndDelete` variant, 3 new actions)
- Modify: `src/app.rs` (add `ConfirmOffDelete` struct, field, and reducer arms)

- [ ] **Step 1: Add `ConfirmOffDelete` struct to `app.rs`**

Near the top of `src/app.rs`, after the `Tab` enum:

```rust
#[derive(Debug, Clone)]
pub struct ConfirmOffDelete {
    pub path: PathBuf,
    pub delete_file: bool,
}
```

Add the import `use std::path::PathBuf;` at the top if not already present.

Add the field to `AppState`:

```rust
pub struct AppState {
    // ... existing fields ...
    // Phase 6 — delete file on swapoff
    pub confirm_off_delete: Option<ConfirmOffDelete>,
}
```

Initialize in `new()`:

```rust
confirm_off_delete: None,
```

- [ ] **Step 2: Add Action variants**

In `src/actions.rs`, add `OffAndDelete` to `DeviceOpKind`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceOpKind {
    On,
    Off,
    OffAndDelete,
    Reset,
}
```

Add 3 new actions after the Phase 6 autocomplete actions:

```rust
    // Phase 6 — delete file on swapoff
    RequestConfirmOffDelete,
    ToggleConfirmDeleteFile,
    CancelConfirmOffDelete,
```

- [ ] **Step 3: Write reducer tests**

In `src/app.rs` test module:

```rust
#[test]
fn request_confirm_off_delete_opens_modal() {
    use crate::platform::SwapKind;
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
}

#[test]
fn toggle_confirm_delete_file_flips() {
    use crate::platform::SwapKind;
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
    use crate::platform::SwapKind;
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
```

- [ ] **Step 4: Run tests — expect FAIL**

```bash
rtk cargo test request_confirm_off_delete
```

- [ ] **Step 5: Implement reducer arms**

In `src/app.rs` `handle_action`:

```rust
Action::RequestConfirmOffDelete => {
    if let Some(dev) = self.devices.get(self.selected_dev) {
        self.confirm_off_delete = Some(ConfirmOffDelete {
            path: dev.path.clone(),
            delete_file: false,
        });
    }
}

Action::ToggleConfirmDeleteFile => {
    if let Some(ref mut modal) = self.confirm_off_delete {
        modal.delete_file = !modal.delete_file;
    }
}

Action::CancelConfirmOffDelete => {
    self.confirm_off_delete = None;
}
```

- [ ] **Step 6: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 7: Commit**

```bash
git add src/actions.rs src/app.rs
git commit -m "feat(phase6): add ConfirmOffDelete state, actions, and reducer"
```

---

## Task 6: Delete-file-on-swapoff — input routing and modal render

**Files:**
- Modify: `src/input.rs` (`handle_devices_key` — route `f` for File type, add off-delete modal key handler)
- Modify: `src/ui/devices.rs` (add `render_off_delete_modal`)
- Modify: `src/main.rs` (add `OffAndDelete` arm in `spawn_blocking`)

- [ ] **Step 1: Route `f` key differently for File type**

In `src/input.rs`, in `handle_devices_key`, the existing `KeyCode::Char('f')` arm
needs to check the device kind. Change it to:

```rust
KeyCode::Char('f') if has_devices => {
    if nix::unistd::geteuid().is_root() {
        // Check if this is a file-type device for the delete modal
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
```

- [ ] **Step 2: Add key handler for the off-delete modal**

In `src/input.rs`, in `handle_devices_key`, add at the very top (before the
existing confirm modal check):

```rust
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
```

- [ ] **Step 3: Clear `confirm_off_delete` when `ExecuteDeviceOp` dispatches**

In `src/app.rs`, in the `Action::ExecuteDeviceOp { .. }` handler (if one exists — 
check where ExecuteDeviceOp is handled). If it's only handled in `main.rs`,
add to the `handle_action`:

```rust
Action::ExecuteDeviceOp { .. } => {
    // Clear both confirm modals when executing
    self.confirm_action = None;
    self.confirm_off_delete = None;
}
```

If there's already a handler, just add `self.confirm_off_delete = None;` to it.

- [ ] **Step 4: Add `OffAndDelete` arm in `main.rs`**

In `src/main.rs`, in the `spawn_blocking` block where `ExecuteDeviceOp` is handled,
add the new arm:

```rust
DeviceOpKind::OffAndDelete => {
    match backend.swap_off(&path) {
        Ok(()) => match std::fs::remove_file(&path) {
            Ok(()) => OpStatus::Done,
            Err(e) => {
                OpStatus::Error(format!("deactivated; delete failed: {e}"))
            }
        },
        Err(e) => OpStatus::Error(e.to_string()),
    }
}
```

- [ ] **Step 5: Add `render_off_delete_modal` in `ui/devices.rs`**

In `src/ui/devices.rs`, add after `render_modal`:

```rust
fn render_off_delete_modal(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(modal) = &state.confirm_off_delete else {
        return;
    };

    let modal_width = (area.width * 60 / 100).max(48);
    let modal_height = 9_u16;
    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_rect = Rect::new(modal_x, modal_y, modal_width, modal_height);

    let border_color = if modal.delete_file {
        Color::Red
    } else {
        Color::Yellow
    };

    let checkbox = if modal.delete_file { "[x]" } else { "[ ]" };
    let checkbox_style = if modal.delete_file {
        Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", modal.path.display()),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "  This will deactivate the swap area.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{checkbox} also delete file (cannot be undone)"),
                checkbox_style,
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            key_span("Space"),
            desc_span(" toggle    "),
            key_span("s"),
            desc_span(" confirm    "),
            key_span("Esc"),
            desc_span(" cancel"),
        ]),
    ];

    f.render_widget(Clear, modal_rect);
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(Span::styled(
                    " Deactivate Swap File ",
                    Style::default()
                        .fg(border_color)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        ),
        modal_rect,
    );
}
```

Call it from `render`:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area);
    render_header(f, layout[0]);
    render_table(f, layout[1], state);
    render_footer(f, layout[2], state);

    if state.confirm_action.is_some() {
        render_modal(f, area, state);
    }

    if state.confirm_off_delete.is_some() {
        render_off_delete_modal(f, area, state);
    }

    if state.create_swap_modal.is_some() {
        crate::ui::create_swap::render(f, area, state);
    }
}
```

- [ ] **Step 6: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 7: Commit**

```bash
git add src/input.rs src/ui/devices.rs src/main.rs src/app.rs
git commit -m "feat(phase6): route f-key for file devices, render off-delete modal, wire OffAndDelete"
```

---

## Task 7: Inactive swap discovery

**Files:**
- Modify: `src/platform/linux.rs` (extend `swap_devices`, add probe helpers)

- [ ] **Step 1: Write tests for `probe_swap_file`**

In `src/platform/linux.rs` test module:

```rust
#[test]
fn probe_swap_file_returns_none_for_nonexistent() {
    let result = probe_swap_file(Path::new("/tmp/nonexistent_swap_probe_test_xyz"));
    assert!(result.is_none());
}

#[test]
fn probe_swap_file_returns_none_for_non_swap() {
    // Create a temp file with no swap magic
    use std::io::Write;
    let dir = std::env::temp_dir();
    let path = dir.join("swaptop_test_non_swap");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(&[0u8; 4096]).unwrap();
    drop(f);

    let result = probe_swap_file(&path);
    assert!(result.is_none());

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn probe_swap_file_returns_device_for_swap_magic() {
    use std::io::Write;
    let dir = std::env::temp_dir();
    let path = dir.join("swaptop_test_swap_magic");
    let mut buf = vec![0u8; 4096];
    buf[4086..4096].copy_from_slice(b"SWAPSPACE2");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(&buf).unwrap();
    drop(f);

    let result = probe_swap_file(&path);
    assert!(result.is_some());
    let dev = result.unwrap();
    assert_eq!(dev.path, path);
    assert!(!dev.active);
    assert!(matches!(dev.kind, SwapKind::File));
    assert_eq!(dev.total, 4096);

    std::fs::remove_file(&path).unwrap();
}
```

- [ ] **Step 2: Run tests — expect FAIL**

```bash
rtk cargo test probe_swap_file
```

- [ ] **Step 3: Implement `probe_swap_file` and `is_block_device`**

In `src/platform/linux.rs`, add after the `parse_swap_line` function:

```rust
use crate::create_swap::detect_swap_magic;

const WELL_KNOWN_SWAP_PATHS: &[&str] = &["/swapfile", "/var/swapfile", "/swap", "/swap.img"];

/// Check if `path` is a regular file with swap magic header.
fn probe_swap_file(path: &Path) -> Option<SwapDevice> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let size = meta.len();
    if size < 4096 {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    detect_swap_magic(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::File,
        active: false,
    })
}

/// Check if `path` is a block device with swap magic header.
fn probe_swap_device(path: &Path) -> Option<SwapDevice> {
    if !is_block_device(path) {
        return None;
    }
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = [0u8; 4096];
    std::io::Read::read_exact(&mut f, &mut buf).ok()?;
    let size = std::fs::metadata(path).ok()?.len();
    detect_swap_magic(&buf, size)?;
    Some(SwapDevice {
        path: path.to_path_buf(),
        total: size,
        used: 0,
        priority: 0,
        kind: SwapKind::Partition,
        active: false,
    })
}

fn is_block_device(path: &Path) -> bool {
    use nix::sys::stat::SFlag;
    nix::sys::stat::stat(path)
        .map(|s| SFlag::from_bits_truncate(s.st_mode) & SFlag::S_IFMT == SFlag::S_IFBLK)
        .unwrap_or(false)
}
```

- [ ] **Step 4: Run probe tests**

```bash
rtk cargo test probe_swap_file
```

- [ ] **Step 5: Write test for `discover_inactive_swaps`**

```rust
#[test]
fn discover_inactive_skips_active_paths() {
    use std::collections::HashSet;

    let active: HashSet<PathBuf> = [PathBuf::from("/swapfile")].into_iter().collect();

    // Simulate: /swapfile is active, so even if it has magic it should be skipped
    let mut found = Vec::new();
    for candidate in WELL_KNOWN_SWAP_PATHS {
        let path = PathBuf::from(candidate);
        if active.contains(&path) {
            continue;
        }
        if let Some(dev) = probe_swap_file(&path) {
            found.push(dev);
        }
    }
    // /swapfile should NOT appear
    assert!(!found.iter().any(|d| d.path == PathBuf::from("/swapfile")));
}
```

- [ ] **Step 6: Extend `swap_devices()` in the `SwapBackend` impl**

Replace the current `swap_devices` method:

```rust
fn swap_devices(&mut self) -> Result<Vec<SwapDevice>> {
    let content = std::fs::read_to_string("/proc/swaps")?;
    let mut devices = parse_proc_swaps(&content);

    let active_paths: std::collections::HashSet<PathBuf> =
        devices.iter().map(|d| d.path.clone()).collect();

    // Probe well-known file paths
    for candidate in WELL_KNOWN_SWAP_PATHS {
        let path = PathBuf::from(candidate);
        if active_paths.contains(&path) {
            continue;
        }
        if let Some(dev) = probe_swap_file(&path) {
            devices.push(dev);
        }
    }

    // Probe block devices in /dev/
    if let Ok(entries) = std::fs::read_dir("/dev/") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if active_paths.contains(&path) {
                continue;
            }
            if let Some(dev) = probe_swap_device(&path) {
                devices.push(dev);
            }
        }
    }

    Ok(devices)
}
```

- [ ] **Step 7: Run full suite**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

- [ ] **Step 8: Commit**

```bash
git add src/platform/linux.rs
git commit -m "feat(phase6): discover inactive swap files at well-known paths and /dev/ block devices"
```

---

## Task 8: Final validation and docs

**Files:**
- Modify: `docs/devices.md` (document new keys: Space in off-delete modal, Tab in create-swap)

- [ ] **Step 1: Update docs**

In `docs/devices.md`, add rows documenting the new keys where applicable.

- [ ] **Step 2: Run full validation**

```bash
rtk cargo build && rtk cargo clippy -- -D warnings && rtk cargo test && cargo fmt --check
```

Expected: zero warnings, all tests pass, format clean.

- [ ] **Step 3: Commit**

```bash
git add docs/devices.md
git commit -m "docs(phase6): document autocomplete and delete-file keybindings"
```
