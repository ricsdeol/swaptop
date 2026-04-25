# Bridge Responsiveness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add UI responsiveness signals so users see immediate feedback during background operations (collect, device ops).

**Architecture:** Bridge emits `CollectStarted`/`CollectFinished` around collect calls. AppState tracks `collect_in_progress`, `last_collect_completed`, and `device_op_started`. UI derives indicators (spinner, stale warning, elapsed time) from state. No structural changes to bridge threading.

**Tech Stack:** Rust, Ratatui, crossterm, tokio, std::time::Instant

**Spec deviation:** Spec said "no CollectFinished". Plan adds it because `SetError` is shared by non-collect callers (keyboard handlers) — clearing `collect_in_progress` on any `SetError` would cause false clears. `CollectFinished` is explicitly tied to collect lifecycle.

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Modify | `src/actions.rs` | Add `CollectStarted`, `CollectFinished` variants |
| Modify | `src/app.rs` | New fields, reducer handlers, tests |
| Modify | `src/platform_bridge.rs` | Emit `CollectStarted`/`CollectFinished` around collect, ordering test |
| Modify | `src/ui/statusbar.rs` | Collect spinner, stale indicator, device op elapsed |
| Modify | `src/ui/devices.rs` | Elapsed time on running device op |

---

## Task 1: Add actions and AppState fields with reducer handlers

**Files:**
- Modify: `src/actions.rs:53-102` (Action enum)
- Modify: `src/app.rs:23-55` (AppState struct)
- Modify: `src/app.rs:57-84` (AppState::new)
- Modify: `src/app.rs:121-470` (handle_action)
- Modify: `src/app.rs:476-1378` (tests)

- [ ] **Step 1: Write failing tests for collect lifecycle**

Add to `src/app.rs` test module (after the `form_input_event_clears_completions` test at line 1377):

```rust
// ── Collect lifecycle (responsiveness) ───────────────────────────────

#[test]
fn collect_started_sets_in_progress() {
    let mut state = AppState::new(make_caps());
    assert!(!state.collect_in_progress);
    state.handle_action(Action::CollectStarted);
    assert!(state.collect_in_progress);
}

#[test]
fn collect_finished_clears_in_progress() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::CollectStarted);
    assert!(state.collect_in_progress);
    state.handle_action(Action::CollectFinished);
    assert!(!state.collect_in_progress);
}

#[test]
fn update_snapshot_updates_last_collect_completed() {
    let mut state = AppState::new(make_caps());
    let before = state.last_collect_completed;
    std::thread::sleep(std::time::Duration::from_millis(10));
    state.handle_action(Action::UpdateSnapshot(make_snapshot()));
    assert!(state.last_collect_completed > before);
}

#[test]
fn set_error_does_not_clear_collect_in_progress() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::CollectStarted);
    state.handle_action(Action::SetError("some error".to_string()));
    assert!(state.collect_in_progress, "SetError must not clear collect flag");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test collect_started_sets_in_progress -- --nocapture 2>&1 | tail -5`
Expected: FAIL — `collect_in_progress` field doesn't exist

- [ ] **Step 3: Add Action variants**

In `src/actions.rs`, add two variants to the `Action` enum after `SetError(String)`:

```rust
// Global
Quit,
NextTab,
PrevTab,
SelectTab(usize),
UpdateSnapshot(MemSnapshot),
SetError(String),
CollectStarted,
CollectFinished,
```

- [ ] **Step 4: Add AppState fields**

In `src/app.rs`, add three fields to `AppState` struct after `is_root: bool`:

```rust
pub is_root: bool,

pub collect_in_progress: bool,
pub last_collect_completed: Instant,
pub device_op_started: Option<Instant>,
```

- [ ] **Step 5: Initialize new fields in AppState::new()**

In `src/app.rs` `AppState::new()`, add after `is_root: nix::unistd::geteuid().is_root(),`:

```rust
is_root: nix::unistd::geteuid().is_root(),
collect_in_progress: false,
last_collect_completed: Instant::now(),
device_op_started: None,
```

- [ ] **Step 6: Add reducer handlers**

In `src/app.rs` `handle_action`, add two new arms after `Action::SetError`:

```rust
Action::SetError(msg) => {
    self.error_msg = Some((msg, Instant::now()));
}

Action::CollectStarted => {
    self.collect_in_progress = true;
}

Action::CollectFinished => {
    self.collect_in_progress = false;
}
```

- [ ] **Step 7: Update UpdateSnapshot handler to track last_collect_completed**

In `src/app.rs`, in the `Action::UpdateSnapshot` handler, add after `self.current = Some(snapshot);` (line 174):

```rust
self.current = Some(snapshot);
self.last_collect_completed = Instant::now();
```

- [ ] **Step 8: Update ExecuteDeviceOp handler to track started time**

In `src/app.rs`, in the `Action::ExecuteDeviceOp` handler, add after `self.confirm_off_delete = None;`:

```rust
Action::ExecuteDeviceOp { path, kind } => {
    self.confirm_action = None;
    self.confirm_off_delete = None;
    self.device_op_started = Some(Instant::now());
    self.device_op = Some(DeviceOp {
        path,
        kind,
        status: OpStatus::Running,
    });
}
```

- [ ] **Step 9: Write failing test for device_op_started**

Add to `src/app.rs` test module:

```rust
#[test]
fn execute_device_op_sets_started_timestamp() {
    let mut state = AppState::new(make_caps());
    assert!(state.device_op_started.is_none());
    state.handle_action(Action::ExecuteDeviceOp {
        path: "/dev/sda2".into(),
        kind: DeviceOpKind::Off,
    });
    assert!(state.device_op_started.is_some());
    assert!(state.device_op_started.unwrap().elapsed().as_secs() < 1);
}

#[test]
fn device_op_update_preserves_started_timestamp() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::ExecuteDeviceOp {
        path: "/dev/sda2".into(),
        kind: DeviceOpKind::Off,
    });
    let started = state.device_op_started.unwrap();
    state.handle_action(Action::DeviceOpUpdate(DeviceOp {
        path: "/dev/sda2".into(),
        kind: DeviceOpKind::Off,
        status: OpStatus::Done,
    }));
    assert_eq!(state.device_op_started.unwrap(), started);
}
```

- [ ] **Step 10: Run all new tests**

Run: `cargo test collect_started -- --nocapture`
Run: `cargo test collect_finished -- --nocapture`
Run: `cargo test last_collect_completed -- --nocapture`
Run: `cargo test set_error_does_not_clear -- --nocapture`
Run: `cargo test execute_device_op_sets_started -- --nocapture`
Run: `cargo test device_op_update_preserves -- --nocapture`
Expected: All PASS

- [ ] **Step 11: Run full test suite**

Run: `cargo test`
Expected: All 188+ tests pass

- [ ] **Step 12: Commit**

```bash
git add src/actions.rs src/app.rs
git commit -m "feat: add collect lifecycle and device op timing to reducer

CollectStarted/CollectFinished track collect-in-progress state.
last_collect_completed enables stale detection in UI.
device_op_started tracks elapsed time for running device operations."
```

---

## Task 2: Bridge emits CollectStarted/CollectFinished

**Files:**
- Modify: `src/platform_bridge.rs:38-42` (Collect handler)
- Modify: `src/platform_bridge.rs:135-359` (tests)

- [ ] **Step 1: Write failing ordering test**

Add to `src/platform_bridge.rs` test module (after `create_swap_does_not_block_collect` test):

```rust
#[test]
fn collect_emits_started_and_finished_around_snapshot() {
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

    let first = recv_action(&mut action_rx);
    assert!(
        matches!(first, Action::CollectStarted),
        "expected CollectStarted, got {first:?}"
    );
    let second = recv_action(&mut action_rx);
    assert!(
        matches!(second, Action::UpdateSnapshot(_)),
        "expected UpdateSnapshot, got {second:?}"
    );
    let third = recv_action(&mut action_rx);
    assert!(
        matches!(third, Action::CollectFinished),
        "expected CollectFinished, got {third:?}"
    );
}

#[test]
fn collect_error_still_emits_finished() {
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

    let first = recv_action(&mut action_rx);
    assert!(
        matches!(first, Action::CollectStarted),
        "expected CollectStarted, got {first:?}"
    );
    let second = recv_action(&mut action_rx);
    assert!(
        matches!(second, Action::SetError(_)),
        "expected SetError, got {second:?}"
    );
    let third = recv_action(&mut action_rx);
    assert!(
        matches!(third, Action::CollectFinished),
        "expected CollectFinished, got {third:?}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test collect_emits_started_and_finished -- --nocapture 2>&1 | tail -10`
Expected: FAIL — first action is `UpdateSnapshot`, not `CollectStarted`

- [ ] **Step 3: Add emit lines to bridge Collect handler**

In `src/platform_bridge.rs`, replace the `PlatformCommand::Collect` arm (lines 40-42):

```rust
PlatformCommand::Collect => {
    let _ = action_tx.send(Action::CollectStarted);
    Self::handle_collect(
        &mut *backend,
        &action_tx,
        &processes_active,
    );
    let _ = action_tx.send(Action::CollectFinished);
}
```

- [ ] **Step 4: Run ordering tests**

Run: `cargo test collect_emits_started_and_finished -- --nocapture`
Run: `cargo test collect_error_still_emits_finished -- --nocapture`
Expected: Both PASS

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src/platform_bridge.rs
git commit -m "feat: bridge emits CollectStarted/CollectFinished around collect

Enables UI to show collect-in-progress spinner and detect stale data.
CollectFinished always emits, even on collect error."
```

---

## Task 3: Statusbar responsiveness indicators

**Files:**
- Modify: `src/ui/statusbar.rs:1-43`
- Modify: `src/actions.rs:28` (remove `#[allow(dead_code)]` from DeviceOp.kind)

- [ ] **Step 1: Remove dead_code allow from DeviceOp.kind**

In `src/actions.rs`, remove the attribute and comment from the `kind` field:

```rust
#[derive(Debug, Clone)]
pub struct DeviceOp {
    pub path: PathBuf,
    pub kind: DeviceOpKind,
    pub status: OpStatus,
}
```

- [ ] **Step 2: Add imports to statusbar.rs**

In `src/ui/statusbar.rs`, replace the imports:

```rust
use std::time::Duration;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::actions::OpStatus;
use crate::app::AppState;
```

- [ ] **Step 3: Add collect spinner and stale indicator**

In `src/ui/statusbar.rs`, in the `render` function, add after building the `spans` vec from keys (after `.collect();` on line 33) and before the error_msg check:

```rust
    .collect();

    if state.collect_in_progress {
        spans.push(Span::styled(" ⟳ ", Style::default().fg(Color::Yellow)));
    } else if state.last_collect_completed.elapsed() >= Duration::from_secs(3) {
        spans.push(Span::styled(
            " ⚠ stale ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(op) = &state.device_op {
        if op.status == OpStatus::Running {
            if let Some(started) = state.device_op_started {
                let elapsed = started.elapsed().as_secs();
                let op_label = match &op.kind {
                    crate::actions::DeviceOpKind::On => "swapon",
                    crate::actions::DeviceOpKind::Off => "swapoff",
                    crate::actions::DeviceOpKind::OffAndDelete => "swapoff+rm",
                    crate::actions::DeviceOpKind::DeleteOnly => "rm",
                    crate::actions::DeviceOpKind::Reset => "reset",
                };
                spans.push(Span::styled(
                    format!(" {op_label} ({elapsed}s) "),
                    Style::default().fg(Color::Yellow),
                ));
            }
        }
    }

    if let Some((err, _)) = &state.error_msg {
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check`
Expected: Compiles clean

- [ ] **Step 5: Run clippy**

Run: `cargo clippy -- -D warnings`
Expected: Clean

- [ ] **Step 6: Commit**

```bash
git add src/ui/statusbar.rs src/actions.rs
git commit -m "feat: statusbar shows collect spinner, stale warning, device op elapsed

Collect in progress: yellow ⟳ indicator.
Data stale (>3s): red ⚠ stale warning.
Device op running: operation name + elapsed seconds.
Remove dead_code allow on DeviceOp.kind — now used by statusbar."
```

---

## Task 4: Devices tab elapsed time

**Files:**
- Modify: `src/ui/devices.rs:139-152` (status_cell function)

- [ ] **Step 1: Update status_cell to show elapsed time**

In `src/ui/devices.rs`, replace the `status_cell` function (lines 139-152):

```rust
fn status_cell<'a>(dev: &crate::platform::SwapDevice, state: &AppState) -> Cell<'a> {
    if let Some(op) = state.device_op.as_ref().filter(|op| op.path == dev.path) {
        return match &op.status {
            OpStatus::Running => {
                let elapsed = state
                    .device_op_started
                    .map(|s| s.elapsed().as_secs())
                    .unwrap_or(0);
                Cell::from(format!("⏳ {elapsed}s")).style(Style::default().fg(Color::Yellow))
            }
            OpStatus::Done => Cell::from("✓ OK").style(Style::default().fg(Color::Green)),
            OpStatus::Error(_) => Cell::from("✗ ERROR").style(Style::default().fg(Color::Red)),
        };
    }
    if dev.active {
        Cell::from("ACTIVE").style(Style::default().fg(Color::Green))
    } else {
        Cell::from("INACTIVE").style(Style::default().fg(Color::DarkGray))
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles clean

- [ ] **Step 3: Commit**

```bash
git add src/ui/devices.rs
git commit -m "feat: device status cell shows elapsed time during operations"
```

---

## Task 5: Final verification

- [ ] **Step 1: Build clean**

Run: `cargo build`
Expected: Zero warnings

- [ ] **Step 2: Clippy**

Run: `cargo clippy -- -D warnings`
Expected: Clean

- [ ] **Step 3: Format**

Run: `cargo fmt --check`
Expected: Formatted

- [ ] **Step 4: Full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 5: Manual verification checklist**

Verify by reading code:
- `CollectStarted` emitted before `handle_collect` in bridge
- `CollectFinished` emitted after `handle_collect` in bridge (both success and error)
- `collect_in_progress` only cleared by `CollectFinished` (not `SetError`)
- `device_op_started` set in `ExecuteDeviceOp`, preserved across `DeviceOpUpdate`
- Statusbar shows ⟳ when collecting, ⚠ stale when data old, op elapsed when running
- Devices tab shows elapsed seconds in status cell
- `#[allow(dead_code)]` removed from `DeviceOp.kind`
