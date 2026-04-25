# Process Detail Modal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a modal detail screen for processes, showing 15-minute RAM/swap history charts, extended metadata (threads, status, exe path), and process termination with confirmation.

**Architecture:** Per-process history is collected continuously into a `HashMap<u32, ProcessHistory>` inside `AppState`, populated on every `UpdateSnapshot`. The modal is a read-only overlay rendered from `ui/process_detail.rs`. Process killing (`SIGTERM`) is dispatched through `PlatformBridge` like existing device operations.

**Tech Stack:** Rust, Ratatui, crossterm, tokio, nix

---

## File Map

| File | Responsibility |
|------|--------------|
| `src/platform/types.rs` | `ProcessRow` struct — add `threads` and `status` |
| `src/platform/linux/proc_reader.rs` | Parse `/proc/PID/stat` for threads + state |
| `src/platform/mod.rs` | `PlatformProvider` trait — add `kill_process` |
| `src/platform/linux/mod.rs` | Linux `kill_process` impl using `nix::sys::signal::kill` |
| `src/platform/bsd.rs` | Stub `kill_process` |
| `src/platform/macos.rs` | Stub `kill_process` |
| `src/platform/windows.rs` | Stub `kill_process` |
| `src/platform_bridge.rs` | `PlatformCommand::KillProcess` + bridge thread handler |
| `src/actions.rs` | New `Action` variants for detail modal + kill flow |
| `src/app/mod.rs` | `AppState` fields + `handle_action` wiring + test helpers |
| `src/app/snapshot.rs` | `push_process_history` called per PID on every snapshot |
| `src/app/processes.rs` | Detail modal handlers (open, close, confirm, kill result) |
| `src/input.rs` | `ProcessDetailContext` + key resolution for modal |
| `src/main.rs` | Intercept `KillProcess` action, send to bridge |
| `src/ui/process_detail.rs` | **NEW** — modal render, charts, metadata, kill confirm |
| `src/ui/mod.rs` | Conditionally render modal overlay |

---

### Task 0: Create feature branch

- [ ] **Step 1: Create and switch to feature branch**

```bash
git checkout -b feat/process-detail-modal
```

- [ ] **Step 2: Push branch to remote (optional but recommended)**

```bash
git push -u origin feat/process-detail-modal
```

---

### Task 1: Extend `ProcessRow` with `threads` and `status`

**Files:**
- Modify: `src/platform/types.rs`
- Modify: `src/app/mod.rs` (test helper)
- Modify: `src/platform_bridge.rs` (test mock)
- Test: `cargo test`

- [ ] **Step 1: Add fields to `ProcessRow`**

In `src/platform/types.rs`, insert two new fields after `cpu_pct`:

```rust
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub exe_path: Option<String>,
    pub user: String,
    pub rss: u64,
    pub swap: u64,
    pub cpu_pct: f32,
    pub threads: u32,      // NEW
    pub status: char,      // NEW — 'R', 'S', 'D', etc.
}
```

- [ ] **Step 2: Update `make_process` test helper**

In `src/app/mod.rs`, inside `test_helpers::make_process`, add the two fields:

```rust
pub fn make_process(pid: u32, name: &str, swap: u64) -> ProcessRow {
    ProcessRow {
        pid,
        name: name.to_string(),
        exe_path: None,
        user: "user".to_string(),
        rss: 0,
        swap,
        cpu_pct: 0.0,
        threads: 1,      // NEW
        status: 'R',     // NEW
    }
}
```

- [ ] **Step 3: Update `MockBackend` test data**

In `src/platform_bridge.rs`, inside `MockBackend::healthy()`, update the `ProcessRow` literal:

```rust
processes: vec![ProcessRow {
    pid: 1,
    name: "init".into(),
    exe_path: None,
    user: "root".into(),
    rss: 1024,
    swap: 512,
    cpu_pct: 0.5,
    threads: 1,      // NEW
    status: 'S',     // NEW
}],
```

- [ ] **Step 4: Run tests to ensure compilation passes**

Run: `rtk cargo test --lib`
Expected: All existing tests compile and pass. No new functionality yet.

- [ ] **Step 5: Commit**

```bash
git add src/platform/types.rs src/app/mod.rs src/platform_bridge.rs
git commit -m "feat(platform): add threads and status fields to ProcessRow"
```

---

### Task 2: Parse threads and status from `/proc/PID/stat`

**Files:**
- Modify: `src/platform/linux/proc_reader.rs`
- Test: `cargo test proc_reader`

- [ ] **Step 1: Add `parse_stat_threads` function**

Insert after `parse_stat_cpu_ticks` in `src/platform/linux/proc_reader.rs`:

```rust
fn parse_stat_threads(content: &str) -> Option<(char, u64)> {
    let after_comm = content.rfind(')')? + 1;
    let fields: Vec<&str> = content[after_comm..].split_whitespace().collect();
    let state = fields.get(0)?.chars().next()?;
    let num_threads = fields.get(17)?.parse().ok()?;
    Some((state, num_threads))
}
```

- [ ] **Step 2: Call it in `ProcReader::collect()`**

In the `collect` method, after reading `stat_content`, extract threads and state alongside CPU ticks:

Replace:
```rust
let ticks = parse_stat_cpu_ticks(&stat_content).unwrap_or(0);
new_ticks.insert(pid, ticks);
```

With:
```rust
let (ticks, state, threads) = match parse_stat_cpu_ticks(&stat_content) {
    Some(t) => {
        let (st, th) = parse_stat_threads(&stat_content).unwrap_or(('?', 1));
        (t, st, th)
    }
    None => continue,
};
new_ticks.insert(pid, ticks);
```

Then add `threads` and `status` to the `ProcessRow` construction at the end of the loop:

Replace:
```rust
rows.push(ProcessRow {
    pid,
    name: info.name,
    exe_path,
    user,
    rss: info.rss,
    swap: info.swap,
    cpu_pct,
});
```

With:
```rust
rows.push(ProcessRow {
    pid,
    name: info.name,
    exe_path,
    user,
    rss: info.rss,
    swap: info.swap,
    cpu_pct,
    threads: threads as u32,
    status: state,
});
```

- [ ] **Step 3: Add unit test for `parse_stat_threads`**

In `src/platform/linux/proc_reader.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn parse_stat_threads_extracts_state_and_num_threads() {
    let content = "1234 (firefox) S 1000 1234 1234 0 -1 4194304 \
                   1000 0 100 0 54321 12345 0 0 20 0 4 0 1000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0";
    let (state, threads) = parse_stat_threads(content).unwrap();
    assert_eq!(state, 'S');
    assert_eq!(threads, 4);
}

#[test]
fn parse_stat_threads_returns_none_for_garbage() {
    assert!(parse_stat_threads("not a stat line").is_none());
}
```

- [ ] **Step 4: Run proc_reader tests**

Run: `rtk cargo test proc_reader`
Expected: All tests pass, including the two new ones.

- [ ] **Step 5: Commit**

```bash
git add src/platform/linux/proc_reader.rs
git commit -m "feat(linux): parse threads and status from /proc/PID/stat"
```

---

### Task 3: Add `kill_process` to `PlatformProvider` trait and all backends

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/linux/mod.rs`
- Modify: `src/platform/bsd.rs`
- Modify: `src/platform/macos.rs`
- Modify: `src/platform/windows.rs`
- Modify: `src/platform_bridge.rs` (MockBackend)
- Test: `cargo test`

- [ ] **Step 1: Add trait method**

In `src/platform/mod.rs`, inside `PlatformProvider` trait, add:

```rust
fn kill_process(&self, pid: u32) -> color_eyre::Result<()>;
```

- [ ] **Step 2: Implement for Linux**

In `src/platform/linux/mod.rs`, inside `impl PlatformProvider for LinuxBackend`, add:

```rust
fn kill_process(&self, pid: u32) -> color_eyre::Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
        .map_err(|e| color_eyre::eyre::eyre!("kill failed: {e}"))
}
```

- [ ] **Step 3: Stub for BSD**

In `src/platform/bsd.rs`, inside `impl PlatformProvider for BsdBackend`, add:

```rust
fn kill_process(&self, _pid: u32) -> color_eyre::Result<()> {
    Err(color_eyre::eyre::eyre!("kill_process not supported on this platform"))
}
```

- [ ] **Step 4: Stub for macOS**

In `src/platform/macos.rs`, inside `impl PlatformProvider for MacosBackend`, add the same stub as BSD (adjust error message to "macOS").

- [ ] **Step 5: Stub for Windows**

In `src/platform/windows.rs`, inside `impl PlatformProvider for WindowsBackend`, add the same stub (adjust error message to "Windows").

- [ ] **Step 6: Add to MockBackend**

In `src/platform_bridge.rs` tests, add to `impl PlatformProvider for MockBackend`:

```rust
fn kill_process(&self, _pid: u32) -> color_eyre::Result<()> {
    Ok(())
}
```

- [ ] **Step 7: Compile check**

Run: `rtk cargo build`
Expected: Compiles cleanly (zero warnings).

- [ ] **Step 8: Commit**

```bash
git add src/platform/mod.rs src/platform/linux/mod.rs src/platform/bsd.rs src/platform/macos.rs src/platform/windows.rs src/platform_bridge.rs
git commit -m "feat(platform): add kill_process to PlatformProvider trait"
```

---

### Task 4: Wire `KillProcess` through `PlatformBridge`

**Files:**
- Modify: `src/platform_bridge.rs`
- Modify: `src/actions.rs`
- Test: `cargo test platform_bridge`

- [ ] **Step 1: Add `KillProcess` to `PlatformCommand`**

In `src/platform_bridge.rs`, add to the `PlatformCommand` enum:

```rust
KillProcess { pid: u32 },
```

- [ ] **Step 2: Handle it in the bridge thread**

In `PlatformBridge::spawn_with_backend`, inside the `while let Ok(cmd) = cmd_rx.recv()` loop, add a new match arm:

```rust
PlatformCommand::KillProcess { pid } => {
    let result = backend.kill_process(pid);
    let action = match result {
        Ok(()) => Action::KillProcessResult { pid, success: true, msg: None },
        Err(e) => Action::KillProcessResult { pid, success: false, msg: Some(e.to_string()) },
    };
    let _ = action_tx.send(action);
}
```

- [ ] **Step 3: Add `KillProcessResult` to `Action` enum**

In `src/actions.rs`, add to the `Action` enum:

```rust
KillProcessResult { pid: u32, success: bool, msg: Option<String> },
```

- [ ] **Step 4: Add bridge test**

In `src/platform_bridge.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn kill_process_sends_result() {
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
    let processes_active = Arc::new(AtomicBool::new(false));
    let bridge = PlatformBridge::spawn_with_backend(
        Box::new(MockBackend::healthy()),
        action_tx,
        processes_active,
    );
    bridge.send(PlatformCommand::KillProcess { pid: 1234 });
    bridge.send(PlatformCommand::Shutdown);
    drop(bridge);

    let action = recv_action(&mut action_rx);
    if let Action::KillProcessResult { pid, success, msg } = action {
        assert_eq!(pid, 1234);
        assert!(success);
        assert!(msg.is_none());
    } else {
        panic!("expected KillProcessResult, got {action:?}");
    }
}
```

- [ ] **Step 5: Run bridge tests**

Run: `rtk cargo test platform_bridge`
Expected: All tests pass, including the new `kill_process_sends_result`.

- [ ] **Step 6: Commit**

```bash
git add src/platform_bridge.rs src/actions.rs
git commit -m "feat(bridge): wire KillProcess through PlatformBridge"
```

---

### Task 5: Add remaining `Action` variants for detail modal

**Files:**
- Modify: `src/actions.rs`
- Test: `cargo test actions`

- [ ] **Step 1: Add variants**

In `src/actions.rs`, add inside the `Action` enum (before `KillProcessResult`):

```rust
OpenProcessDetail { pid: u32 },
CloseProcessDetail,
ConfirmKillProcess { pid: u32 },
KillProcess { pid: u32 },          // intercepted by main.rs, never reaches reducer
```

And `KillProcessResult` from Task 4 should already be there. If not, add it now.

- [ ] **Step 2: Add tests**

In `src/actions.rs` `#[cfg(test)]` module, add:

```rust
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
```

- [ ] **Step 3: Run tests**

Run: `rtk cargo test actions`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/actions.rs
git commit -m "feat(actions): add detail modal and kill action variants"
```

---

### Task 6: Add `ProcessHistory` fields to `AppState`

**Files:**
- Modify: `src/app/mod.rs`
- Test: `cargo test app`

- [ ] **Step 1: Add `ProcessHistory` struct**

In `src/app/mod.rs`, before `AppState`, add:

```rust
#[derive(Debug, Clone)]
pub struct ProcessHistory {
    pub rss_history: VecDeque<(Instant, u64)>,
    pub swap_history: VecDeque<(Instant, u64)>,
}
```

- [ ] **Step 2: Add fields to `AppState`**

Inside `AppState`, add after `device_op_started`:

```rust
pub process_history: HashMap<u32, ProcessHistory>,
pub selected_process_detail: Option<u32>,
pub process_detail_confirm_kill: bool,
```

- [ ] **Step 3: Initialize in `AppState::new`**

Inside `AppState::new`, add after `device_op_started: None,`:

```rust
process_history: HashMap::new(),
selected_process_detail: None,
process_detail_confirm_kill: false,
```

- [ ] **Step 4: Import `HashMap`**

At the top of `src/app/mod.rs`, `HashMap` is already imported via `std::collections::VecDeque` — no, wait. `HashMap` is from `std::collections::HashMap`. Check if it's already imported. Looking at the file, it has `use std::collections::VecDeque;`. Add `HashMap`:

```rust
use std::collections::{HashMap, VecDeque};
```

- [ ] **Step 5: Compile check**

Run: `rtk cargo test --lib`
Expected: Compiles. No new tests yet for these fields.

- [ ] **Step 6: Commit**

```bash
git add src/app/mod.rs
git commit -m "feat(app): add ProcessHistory and detail modal state to AppState"
```

---

### Task 7: Populate per-process history on every snapshot (TDD)

**Files:**
- Modify: `src/app/snapshot.rs`
- Test: `cargo test snapshot`

- [ ] **Step 1: Write failing tests**

In `src/app/snapshot.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn snapshot_appends_process_history_for_each_row() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 100), make_process(2, "b", 200)];
    state.handle_action(Action::UpdateSnapshot(snap));
    assert!(state.process_history.contains_key(&1));
    assert!(state.process_history.contains_key(&2));
    assert_eq!(state.process_history[&1].rss_history.len(), 1);
    assert_eq!(state.process_history[&1].swap_history.len(), 1);
}

#[test]
fn process_history_capped_at_900_entries() {
    let mut state = AppState::new(make_caps());
    for _ in 0..1000 {
        let mut snap = make_snapshot();
        snap.processes = vec![make_process(1, "a", 100)];
        state.handle_action(Action::UpdateSnapshot(snap));
    }
    assert_eq!(state.process_history[&1].rss_history.len(), 900);
    assert_eq!(state.process_history[&1].swap_history.len(), 900);
}

#[test]
fn process_history_retained_when_pid_leaves_snapshot() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 100)];
    state.handle_action(Action::UpdateSnapshot(snap));

    let snap2 = make_snapshot(); // processes = vec![]
    state.handle_action(Action::UpdateSnapshot(snap2));

    assert!(state.process_history.contains_key(&1));
    assert_eq!(state.process_history[&1].rss_history.len(), 1);
}

#[test]
fn process_history_resumed_when_pid_returns() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 100)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.handle_action(Action::UpdateSnapshot(make_snapshot())); // empty
    state.handle_action(Action::UpdateSnapshot(snap)); // returns

    assert_eq!(state.process_history[&1].rss_history.len(), 2);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `rtk cargo test process_history`
Expected: FAIL — `push_process_history` method does not exist.

- [ ] **Step 3: Implement `push_process_history`**

In `src/app/snapshot.rs`, inside `impl AppState`, add a private method:

```rust
fn push_process_history(&mut self, snapshot: &MemSnapshot) {
    for proc in &snapshot.processes {
        let entry = self.process_history.entry(proc.pid).or_insert_with(|| ProcessHistory {
            rss_history: VecDeque::new(),
            swap_history: VecDeque::new(),
        });
        entry.rss_history.push_back((snapshot.timestamp, proc.rss));
        entry.swap_history.push_back((snapshot.timestamp, proc.swap));
        while entry.rss_history.len() > 900 {
            entry.rss_history.pop_front();
        }
        while entry.swap_history.len() > 900 {
            entry.swap_history.pop_front();
        }
    }
}
```

Then call it in `apply_snapshot`, after `self.push_history(&snapshot);`:

```rust
self.push_process_history(&snapshot);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `rtk cargo test process_history`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/snapshot.rs
git commit -m "feat(app): collect per-process RAM/swap history on every snapshot"
```

---

### Task 8: Implement process detail modal handlers (TDD)

**Files:**
- Modify: `src/app/processes.rs`
- Test: `cargo test processes`

- [ ] **Step 1: Write failing tests**

In `src/app/processes.rs` `#[cfg(test)]` module, add:

```rust
#[test]
fn open_process_detail_sets_pid_and_clears_confirm_kill() {
    let mut state = AppState::new(make_caps());
    state.process_detail_confirm_kill = true;
    state.handle_action(Action::OpenProcessDetail { pid: 42 });
    assert_eq!(state.selected_process_detail, Some(42));
    assert!(!state.process_detail_confirm_kill);
}

#[test]
fn close_process_detail_clears_pid_and_confirm_kill() {
    let mut state = AppState::new(make_caps());
    state.selected_process_detail = Some(42);
    state.process_detail_confirm_kill = true;
    state.handle_action(Action::CloseProcessDetail);
    assert!(state.selected_process_detail.is_none());
    assert!(!state.process_detail_confirm_kill);
}

#[test]
fn close_process_detail_when_not_confirming_clears_only_pid() {
    let mut state = AppState::new(make_caps());
    state.selected_process_detail = Some(42);
    state.process_detail_confirm_kill = false;
    state.handle_action(Action::CloseProcessDetail);
    assert!(state.selected_process_detail.is_none());
    assert!(!state.process_detail_confirm_kill);
}

#[test]
fn confirm_kill_process_sets_flag() {
    let mut state = AppState::new(make_caps());
    state.selected_process_detail = Some(42);
    state.handle_action(Action::ConfirmKillProcess { pid: 42 });
    assert!(state.process_detail_confirm_kill);
}

#[test]
fn kill_process_result_success_closes_modal() {
    let mut state = AppState::new(make_caps());
    state.selected_process_detail = Some(42);
    state.process_detail_confirm_kill = true;
    state.handle_action(Action::KillProcessResult { pid: 42, success: true, msg: None });
    assert!(state.selected_process_detail.is_none());
    assert!(!state.process_detail_confirm_kill);
    assert!(state.error_msg.is_some());
}

#[test]
fn kill_process_result_failure_keeps_modal_and_sets_error() {
    let mut state = AppState::new(make_caps());
    state.selected_process_detail = Some(42);
    state.handle_action(Action::KillProcessResult {
        pid: 42,
        success: false,
        msg: Some("Permission denied".into()),
    });
    assert_eq!(state.selected_process_detail, Some(42));
    assert!(state.error_msg.is_some());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `rtk cargo test detail`
Expected: FAIL — handler methods do not exist.

- [ ] **Step 3: Implement handlers**

In `src/app/processes.rs`, inside `impl AppState`, add:

```rust
pub(crate) fn handle_open_process_detail(&mut self, pid: u32) {
    self.selected_process_detail = Some(pid);
    self.process_detail_confirm_kill = false;
}

pub(crate) fn handle_close_process_detail(&mut self) {
    if self.process_detail_confirm_kill {
        self.process_detail_confirm_kill = false;
    } else {
        self.selected_process_detail = None;
    }
}

pub(crate) fn handle_confirm_kill_process(&mut self, _pid: u32) {
    self.process_detail_confirm_kill = true;
}

pub(crate) fn handle_kill_process_result(&mut self, success: bool, msg: Option<String>) {
    if success {
        self.selected_process_detail = None;
        self.process_detail_confirm_kill = false;
        self.error_msg = Some((format!("Sent SIGTERM to process"), Instant::now()));
    } else {
        let text = msg.unwrap_or_else(|| "Failed to kill process".into());
        self.error_msg = Some((text, Instant::now()));
    }
}
```

- [ ] **Step 4: Wire into `handle_action` in `app/mod.rs`**

In `src/app/mod.rs`, inside `handle_action`, add match arms in the same order as the enum:

```rust
Action::OpenProcessDetail { pid } => self.handle_open_process_detail(pid),
Action::CloseProcessDetail => self.handle_close_process_detail(),
Action::ConfirmKillProcess { pid } => self.handle_confirm_kill_process(pid),
Action::KillProcessResult { success, msg, .. } => self.handle_kill_process_result(*success, msg.clone()),
```

Note: `KillProcess { pid }` is intercepted by `main.rs` and does NOT get a match arm in the reducer.

- [ ] **Step 5: Run tests to verify they pass**

Run: `rtk cargo test detail`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/app/processes.rs src/app/mod.rs
git commit -m "feat(app): add process detail modal state handlers"
```

---

### Task 9: Update input resolver for detail modal (TDD)

**Files:**
- Modify: `src/input.rs`
- Test: `cargo test input`

- [ ] **Step 1: Add `ProcessDetailContext` and update `KeyContext`**

In `src/input.rs`, add before `KeyContext`:

```rust
pub struct ProcessDetailContext {
    pub pid: u32,
    pub show_kill_confirm: bool,
}
```

And add to `KeyContext`:

```rust
pub process_detail: Option<ProcessDetailContext>,
```

- [ ] **Step 2: Populate in `KeyContext::from_state`**

At the end of `KeyContext::from_state`, before constructing `Self`, add:

```rust
let process_detail = s.selected_process_detail.map(|pid| ProcessDetailContext {
    pid,
    show_kill_confirm: s.process_detail_confirm_kill,
});
```

And include `process_detail` in the `Self { ... }` return:

```rust
Self {
    active_tab: s.active_tab.clone(),
    filter_mode: s.filter_mode,
    sort_col: s.sort_col,
    is_root: s.is_root,
    device,
    create_swap,
    process_detail,  // NEW
}
```

- [ ] **Step 3: Write failing tests**

In `src/input.rs` `#[cfg(test)]` module, add:

```rust
fn make_detail_ctx(pid: u32, confirm: bool) -> KeyContext {
    KeyContext {
        active_tab: Tab::Processes,
        filter_mode: false,
        sort_col: SortColumn::Swap,
        is_root: false,
        device: default_device(),
        create_swap: None,
        process_detail: Some(ProcessDetailContext { pid, show_kill_confirm: confirm }),
    }
}

#[test]
fn enter_opens_detail_on_processes_tab() {
    let ctx = KeyContext {
        active_tab: Tab::Processes,
        filter_mode: false,
        sort_col: SortColumn::Swap,
        is_root: false,
        device: default_device(),
        create_swap: None,
        process_detail: None,
    };
    // Need a selected process to get a PID — but input.rs doesn't know processes.
    // Instead, main.rs will look up the selected process's PID before dispatch.
    // For input.rs, we just resolve Enter as OpenProcessDetail with pid 0 as placeholder,
    // and main.rs fills in the real PID from selected_row.
    let action = resolve_key(key(KeyCode::Enter), &ctx);
    assert!(matches!(action, Some(Action::OpenProcessDetail { .. })));
}

#[test]
fn esc_closes_detail_when_no_confirm() {
    let ctx = make_detail_ctx(42, false);
    let action = resolve_key(key(KeyCode::Esc), &ctx);
    assert!(matches!(action, Some(Action::CloseProcessDetail)));
}

#[test]
fn esc_cancels_confirm_when_active() {
    let ctx = make_detail_ctx(42, true);
    let action = resolve_key(key(KeyCode::Esc), &ctx);
    assert!(matches!(action, Some(Action::CloseProcessDetail)));
}

#[test]
fn q_closes_detail() {
    let ctx = make_detail_ctx(42, false);
    let action = resolve_key(key(KeyCode::Char('q')), &ctx);
    assert!(matches!(action, Some(Action::CloseProcessDetail)));
}

#[test]
fn k_triggers_confirm_kill() {
    let ctx = make_detail_ctx(42, false);
    let action = resolve_key(key(KeyCode::Char('k')), &ctx);
    assert!(matches!(action, Some(Action::ConfirmKillProcess { pid: 42 })));
}

#[test]
fn y_confirms_kill_when_confirming() {
    let ctx = make_detail_ctx(42, true);
    let action = resolve_key(key(KeyCode::Char('y')), &ctx);
    assert!(matches!(action, Some(Action::KillProcess { pid: 42 })));
}

#[test]
fn n_cancels_kill_confirm() {
    let ctx = make_detail_ctx(42, true);
    let action = resolve_key(key(KeyCode::Char('n')), &ctx);
    assert!(matches!(action, Some(Action::CloseProcessDetail)));
}

#[test]
fn other_keys_ignored_in_detail_mode() {
    let ctx = make_detail_ctx(42, false);
    let action = resolve_key(key(KeyCode::Char('x')), &ctx);
    assert!(action.is_none());
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `rtk cargo test input`
Expected: FAIL — `ProcessDetailContext` not known, resolution logic missing.

- [ ] **Step 5: Implement resolution logic**

In `src/input.rs`, in `resolve_key`, after the `create_swap` block and before the global keys (`q`, `Ctrl+c`, `Tab`, etc.), add:

```rust
if let Some(ref detail) = ctx.process_detail {
    return match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('n')
            if detail.show_kill_confirm =>
        {
            Some(Action::CloseProcessDetail)
        }
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseProcessDetail),
        KeyCode::Char('k') if !detail.show_kill_confirm => {
            Some(Action::ConfirmKillProcess { pid: detail.pid })
        }
        KeyCode::Char('y') if detail.show_kill_confirm => {
            Some(Action::KillProcess { pid: detail.pid })
        }
        _ => None,
    };
}
```

Then, inside the `Tab::Processes` match arm, add `Enter` handling:

Replace:
```rust
Tab::Processes => match key.code {
    KeyCode::Char('j') | KeyCode::Down => return Some(Action::NavigateDown),
    KeyCode::Char('k') | KeyCode::Up => return Some(Action::NavigateUp),
    KeyCode::Char('s') => {
        return Some(Action::SortBy(next_sort_column(&ctx.sort_col)));
    }
    KeyCode::Char('/') => return Some(Action::EnterFilterMode),
    _ => {}
},
```

With:
```rust
Tab::Processes => match key.code {
    KeyCode::Char('j') | KeyCode::Down => return Some(Action::NavigateDown),
    KeyCode::Char('k') | KeyCode::Up => return Some(Action::NavigateUp),
    KeyCode::Char('s') => {
        return Some(Action::SortBy(next_sort_column(&ctx.sort_col)));
    }
    KeyCode::Char('/') => return Some(Action::EnterFilterMode),
    KeyCode::Enter => return Some(Action::OpenProcessDetail { pid: 0 }), // placeholder, filled by main.rs
    _ => {}
},
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `rtk cargo test input`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/input.rs
git commit -m "feat(input): resolve keys for process detail modal"
```

---

### Task 10: Update `main.rs` to intercept `KillProcess` and resolve `OpenProcessDetail` PID

**Files:**
- Modify: `src/main.rs`
- Test: `cargo build`

- [ ] **Step 1: Extract `KillProcess` and `OpenProcessDetail` before dispatch**

In `src/main.rs`, inside the `events.next().fuse()` arm, after `let action = input::resolve_key(key, &ctx);`, add extraction logic similar to existing `device_op_cmd` and `submit_activate_only`:

```rust
let kill_cmd = if let Some(Action::KillProcess { ref pid }) = action {
    Some(*pid)
} else {
    None
};

let open_detail = if let Some(Action::OpenProcessDetail { .. }) = action {
    // Look up the selected process PID from current state
    let s = state.lock().expect("state mutex poisoned");
    let lower = s.filter_text.to_lowercase();
    let visible: Vec<&ProcessRow> = if lower.is_empty() {
        s.processes.iter().collect()
    } else {
        s.processes.iter().filter(|p| {
            p.name.to_lowercase().contains(&lower)
                || p.exe_path.as_ref().is_some_and(|e| e.to_lowercase().contains(&lower))
        }).collect()
    };
    let clamped = s.selected_row.min(visible.len().saturating_sub(1));
    visible.get(clamped).map(|p| p.pid)
} else {
    None
};
```

Then, modify the action dispatch block:

Before `if let Some(a) = action`, add:

```rust
// Send kill command to bridge before consuming the action
if let Some(pid) = kill_cmd {
    bridge.send(PlatformCommand::KillProcess { pid });
}
```

And before dispatching the action to the reducer, replace `OpenProcessDetail { pid: 0 }` with the real PID:

Inside `if let Some(a) = action {`, at the very top, add:

```rust
let a = if let Some(pid) = open_detail {
    Action::OpenProcessDetail { pid }
} else {
    a
};
```

Note: The `open_detail` lookup happens while the state mutex is already held inside the `s` scope. We need to restructure slightly to avoid double-locking. A cleaner approach:

Extract the PID lookup BEFORE creating the action, while state is locked for `KeyContext::from_state`. Actually, `KeyContext::from_state` already holds the lock. We can extend it:

After `let ctx = { let s = state.lock()...; input::KeyContext::from_state(&s) };`, add:

```rust
let selected_process_pid = {
    let s = state.lock().expect("state mutex poisoned");
    let lower = s.filter_text.to_lowercase();
    let visible: Vec<&ProcessRow> = if lower.is_empty() {
        s.processes.iter().collect()
    } else {
        s.processes.iter().filter(|p| {
            p.name.to_lowercase().contains(&lower)
                || p.exe_path.as_ref().is_some_and(|e| e.to_lowercase().contains(&lower))
        }).collect()
    };
    let clamped = s.selected_row.min(visible.len().saturating_sub(1));
    visible.get(clamped).map(|p| p.pid)
};
```

Then, after `let action = input::resolve_key(key, &ctx);`, map the action:

```rust
let action = match action {
    Some(Action::OpenProcessDetail { .. }) => {
        selected_process_pid.map(|pid| Action::OpenProcessDetail { pid })
    }
    other => other,
};
```

And extract `kill_cmd` after this mapping.

- [ ] **Step 2: Send kill command to bridge**

Add before the reducer dispatch:

```rust
if let Some(Action::KillProcess { pid }) = action {
    bridge.send(PlatformCommand::KillProcess { pid });
}
```

But we need the action to NOT reach the reducer. So we should remove it before dispatch:

```rust
let kill_cmd = if let Some(Action::KillProcess { pid }) = action {
    Some(pid)
} else {
    None
};
let action = action.filter(|a| !matches!(a, Action::KillProcess { .. }));
```

Wait, `Option::filter` is awkward. Simpler:

```rust
let action = match action {
    Some(Action::KillProcess { pid }) => {
        bridge.send(PlatformCommand::KillProcess { pid });
        None
    }
    other => other,
};
```

This extracts `KillProcess`, sends to bridge, and replaces the action with `None` so the reducer never sees it.

- [ ] **Step 3: Compile check**

Run: `rtk cargo build`
Expected: Compiles cleanly.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): intercept KillProcess and resolve OpenProcessDetail PID"
```

---

### Task 11: Create `ui/process_detail.rs` module

**Files:**
- Create: `src/ui/process_detail.rs`
- Test: `cargo test ui::process_detail`

- [ ] **Step 1: Create the file with render function**

Create `src/ui/process_detail.rs`:

```rust
use human_bytes::human_bytes;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    style::Stylize,
    symbols,
    text::{Line, Span, Text},
    widgets::{Axis, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph},
};

use crate::app::AppState;

pub fn render(f: &mut Frame, state: &AppState) {
    let area = centered_rect(70, 70, f.area());
    f.render_widget(Clear, area); // clear background

    let layout = build_layout(area);
    let pid = state.selected_process_detail.unwrap_or(0);

    // Title
    let title = if let Some(proc) = find_process(state, pid) {
        format!(" {} (PID {}) ", proc.name, pid)
    } else {
        format!(" PID {} ", pid)
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let inner_layout = build_layout(inner);

    // Metadata
    render_metadata(f, inner_layout[0], state, pid);

    // Charts
    if inner_layout[1].height >= 5 {
        render_charts(f, inner_layout[1], state, pid);
    }

    // Current values + footer
    render_footer(f, inner_layout[2], state, pid);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // metadata
            Constraint::Min(5),      // charts
            Constraint::Length(2),   // current values + footer
        ])
        .split(area)
}

fn render_metadata(f: &mut Frame, area: Rect, state: &AppState, pid: u32) {
    let proc = find_process(state, pid);
    let ended = proc.is_none();
    let proc = proc.or_else(|| state.process_history.get(&pid).map(|_| None).unwrap_or(None));
    // Actually we need the last known info if process ended
    let (name, user, threads, status, exe_path) = if let Some(p) = find_process(state, pid) {
        (p.name.clone(), p.user.clone(), p.threads, p.status, p.exe_path.clone())
    } else {
        ("(process ended)".into(), "?".into(), 0, '?', None)
    };

    let status_desc = match status {
        'R' => "running",
        'S' => "sleeping",
        'D' => "disk sleep",
        'T' => "stopped",
        'Z' => "zombie",
        _ => "unknown",
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(format!("User: {user} "), Style::default().fg(Color::White)),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("Threads: {threads} "), Style::default().fg(Color::White)),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("Status: {status} ({status_desc})"), Style::default().fg(Color::White)),
        ]),
        if let Some(ref exe) = exe_path {
            Line::from(vec![
                Span::styled("Exec: ", Style::default().fg(Color::DarkGray)),
                Span::styled(exe.clone(), Style::default().fg(Color::White)),
            ])
        } else {
            Line::from("")
        },
    ];

    let p = Paragraph::new(Text::from(lines));
    f.render_widget(p, area);
}

fn render_charts(f: &mut Frame, area: Rect, state: &AppState, pid: u32) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let start = state.start_time;
    let hist = state.process_history.get(&pid);

    // Convert history to chart data points (x = seconds since start, y = bytes)
    let (ram_data, swap_data, ram_max, swap_max) = if let Some(h) = hist {
        let ram: Vec<(f64, f64)> = h.rss_history.iter()
            .map(|(t, v)| (t.duration_since(start).as_secs_f64(), *v as f64))
            .collect();
        let swap: Vec<(f64, f64)> = h.swap_history.iter()
            .map(|(t, v)| (t.duration_since(start).as_secs_f64(), *v as f64))
            .collect();
        let ram_max = ram.iter().map(|(_, y)| *y).fold(1.0, f64::max);
        let swap_max = swap.iter().map(|(_, y)| *y).fold(1.0, f64::max);
        (ram, swap, ram_max, swap_max)
    } else {
        (vec![], vec![], 1.0, 1.0)
    };

    let now_secs = start.elapsed().as_secs_f64();
    let window = 900.0_f64; // 15 minutes in seconds
    let x_max = now_secs.max(window);
    let x_min = (x_max - window).max(0.0);

    // RAM Chart
    let ram_datasets = vec![
        Dataset::default()
            .name("RAM")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&ram_data),
    ];
    let ram_chart = Chart::new(ram_datasets)
        .block(Block::bordered().title(" RAM History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec!["-15m".into(), "now".into()]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, ram_max])
                .labels(vec![
                    human_bytes(0.0).into(),
                    human_bytes(ram_max).into(),
                ]),
        );
    f.render_widget(ram_chart, parts[0]);

    // Swap Chart
    let swap_datasets = vec![
        Dataset::default()
            .name("Swap")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&swap_data),
    ];
    let swap_chart = Chart::new(swap_datasets)
        .block(Block::bordered().title(" Swap History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec!["-15m".into(), "now".into()]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, swap_max])
                .labels(vec![
                    human_bytes(0.0).into(),
                    human_bytes(swap_max).into(),
                ]),
        );
    f.render_widget(swap_chart, parts[1]);

    // Short history message overlay (rendered on top of left chart if data is sparse)
    if ram_data.len() < 10 {
        let msg = Paragraph::new(format!("Collecting history... ({}/900)", ram_data.len()))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(msg, parts[0]);
    }
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState, pid: u32) {
    let proc = find_process(state, pid);
    let (rss, swap) = if let Some(p) = proc {
        (p.rss, p.swap)
    } else {
        (0, 0)
    };

    let lines = if state.process_detail_confirm_kill {
        vec![Line::from(vec![
            Span::styled(format!(" Kill PID {pid}? "), Style::default().fg(Color::Red)),
            Span::styled("[y]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("es / ", Style::default().fg(Color::DarkGray)),
            Span::styled("[n]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("o / ", Style::default().fg(Color::DarkGray)),
            Span::styled("[Esc]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
        ])]
    } else {
        vec![
            Line::from(vec![
                Span::styled(" Current: ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("RSS {} ", human_bytes(rss as f64)), Style::default().fg(Color::White)),
                Span::styled("| ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("Swap {}", human_bytes(swap as f64)), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled(" [k]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" kill  ", Style::default().fg(Color::DarkGray)),
                Span::styled("[Esc/q]", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" back", Style::default().fg(Color::DarkGray)),
            ]),
        ]
    };

    let p = Paragraph::new(Text::from(lines));
    f.render_widget(p, area);
}

fn find_process<'a>(state: &'a AppState, pid: u32) -> Option<&'a crate::platform::ProcessRow> {
    state.processes.iter().find(|p| p.pid == pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn centered_rect_produces_reasonable_dimensions() {
        let area = Rect::new(0, 0, 100, 40);
        let popup = centered_rect(70, 70, area);
        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.width <= area.width);
        assert!(popup.height <= area.height);
    }

    #[test]
    fn build_layout_splits_into_three_sections() {
        let area = Rect::new(0, 0, 80, 24);
        let layout = build_layout(area);
        assert_eq!(layout.len(), 3);
    }
}
```

- [ ] **Step 2: Add module declaration**

In `src/ui/mod.rs`, add to the `pub(crate) mod` list:

```rust
pub(crate) mod process_detail;
```

- [ ] **Step 3: Wire overlay in `ui/mod.rs` `render` function**

In `src/ui/mod.rs`, inside `pub fn render`, after the `statusbar::render` call, add:

```rust
if state.selected_process_detail.is_some() {
    process_detail::render(f, state);
}
```

But we need to render the modal OVER the tab content. The current layout is:
```rust
render_tabbar(f, layout[0], state);
match state.active_tab { ... }
statusbar::render(f, layout[2], state);
```

So add the overlay AFTER `statusbar::render` (or before, either works since modal uses `Clear`):

```rust
if state.selected_process_detail.is_some() {
    process_detail::render(f, state);
}
```

- [ ] **Step 4: Compile check**

Run: `rtk cargo build`
Expected: Compiles cleanly. May have warnings about unused imports — fix them.

- [ ] **Step 5: Run UI tests**

Run: `rtk cargo test ui::process_detail`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ui/process_detail.rs src/ui/mod.rs
git commit -m "feat(ui): add process detail modal with charts and metadata"
```

---

### Task 12: Final verification

- [ ] **Step 1: Full test suite**

Run: `rtk cargo test`
Expected: All tests pass.

- [ ] **Step 2: Lint with clippy**

Run: `rtk cargo clippy -- -D warnings`
Expected: Zero warnings. Fix any that appear.

- [ ] **Step 3: Format check**

Run: `rtk cargo fmt --check`
Expected: No formatting issues. Run `rtk cargo fmt` if needed.

- [ ] **Step 4: Build release**

Run: `rtk cargo build --release`
Expected: Compiles cleanly.

- [ ] **Step 5: Final commit**

```bash
git add .
git commit -m "feat: process detail modal with history charts and kill action"
```

---

### Task 13: Open draft pull request

- [ ] **Step 1: Push branch to remote**

```bash
git push origin feat/process-detail-modal
```

- [ ] **Step 2: Run full verification suite**

```bash
rtk cargo test
rtk cargo clippy -- -D warnings
rtk cargo fmt --check
rtk cargo build --release
```

Expected: All pass cleanly. If any fail, fix and re-commit before opening PR.

- [ ] **Step 3: Open draft pull request**

```bash
gh pr create --draft --title "feat: process detail modal" --body "$(cat <<'EOF'
## Summary

Add a modal detail screen for processes, showing 15-minute RAM/swap history charts, extended metadata (threads, status, exe path), and process termination with confirmation.

## Changes

- Extend `ProcessRow` with `threads` and `status` fields
- Parse threads + status from `/proc/PID/stat`
- Add `kill_process` to `PlatformProvider` trait (SIGTERM via nix)
- Collect per-process history (900 points = 15 minutes) continuously
- Add process detail modal with Chart widgets (line charts with axes)
- Support process kill with `k` → confirmation → `y`/`n`

## Architecture

- Pure reducer: all state mutations in `app/` modules
- Bridge owns platform: `KillProcess` dispatched through `PlatformBridge`
- UI is read-only: `process_detail.rs` renders from `&AppState`
EOF
)"
```

---

## Plan Self-Review

### 1. Spec Coverage

| Spec Requirement | Plan Task |
|---|---|
| 15-minute history (900 pts) per process | Task 7: `push_process_history` caps at 900 |
| Continuous collection from app start | Task 7: called in `apply_snapshot` unconditionally |
| Extended metadata (threads, status, exe, user) | Task 2: proc_reader parsing; Task 11: render_metadata |
| `SIGTERM` kill with confirmation | Task 4 (bridge), Task 8 (confirm handlers), Task 9 (y/n keys), Task 11 (footer) |
| Modal overlay with charts | Task 11: `process_detail.rs` with Chart widget (line charts with axes) |
| `Esc`/`q` to close, `k` to initiate kill | Task 9: input resolver |
| Process ended while modal open | Task 11: `find_process` returns None → shows "(process ended)" |
| Small terminal handling | Task 11: charts only render if height >= 5 |
| Architecture: pure reducer, bridge owns platform, UI read-only | All tasks follow this — no I/O in app/, no platform in main.rs |

### 2. Placeholder Scan

No TBD, TODO, or vague steps found. Every step contains actual code or exact commands.

### 3. Type Consistency

- `ProcessRow` fields `threads: u32`, `status: char` — consistent across types.rs, proc_reader.rs, app/mod.rs test helper, platform_bridge.rs mock.
- `ProcessHistory` struct uses `VecDeque<(Instant, u64)>` — matches existing `ram_history` pattern.
- `Action` variants: `OpenProcessDetail { pid: u32 }`, `KillProcess { pid: u32 }`, `KillProcessResult { pid: u32, success: bool, msg: Option<String> }` — consistent types.
- `PlatformCommand::KillProcess { pid: u32 }` — consistent with Action.

No type mismatches or renamed methods across tasks.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-04-25-process-detail-modal.md`.**

**Two execution options:**

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
