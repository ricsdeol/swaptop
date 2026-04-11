# Processes Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Phase 2 — a sortable, filterable process table in the Processes tab, with per-process swap usage collected in parallel via `/proc/PID/smaps`.

**Architecture:** `SwapBackend` gets a `process_list()` method (default `Ok(vec![])`). `Collector` gains an `Arc<AtomicBool>` flag; when true it collects processes + spawns parallel smaps tasks per tick. `AppState` gains sort/filter/navigation state. `ui/processes.rs` renders a `ratatui::Table` with `TableState`.

**Tech Stack:** Rust, Ratatui 0.30, sysinfo 0.38, tokio, futures, human_bytes

**Worktree:** `/home/ricsdeol/projects/swaptop/.worktrees/processes-screen`

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src/platform/mod.rs` | Modify | Add `process_list()` to `SwapBackend` with default `Ok(vec![])` |
| `src/platform/linux.rs` | Modify | Implement `process_list()` + add `Users` field + extract `is_kernel_thread()` |
| `src/actions.rs` | Modify | Add `SortColumn`, `SortDir` enums + 7 new `Action` variants |
| `src/app.rs` | Modify | Add 6 new fields + `sort_processes()` + `filtered_len()` + reducer cases |
| `src/collector.rs` | Modify | Add `processes_active: Arc<AtomicBool>` + parallel smaps collection |
| `src/main.rs` | Modify | Context-aware keyboard routing + `processes_active` wiring + `next_sort_column()` |
| `src/ui/mod.rs` | Modify | Route `Tab::Processes` to `processes::render` |
| `src/ui/processes.rs` | Create | Full Table render: header, rows, filter bar, empty state, platform banner |
| `src/ui/statusbar.rs` | Modify | Show processes-tab keybindings when `active_tab == Processes` |
| `docs/processes-screen.md` | Create | User documentation |

---

## Task 1: Extract kernel-thread filter + add `process_list()` to trait + LinuxBackend

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/linux.rs`

- [ ] **Step 1: Write the failing tests for `is_kernel_thread`**

Add at the bottom of `src/platform/linux.rs` inside the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn kernel_thread_filter_matches_bracketed_names() {
    assert!(is_kernel_thread("[kworker/0:0]"));
    assert!(is_kernel_thread("[migration/0]"));
    assert!(is_kernel_thread("[kswapd0]"));
}

#[test]
fn kernel_thread_filter_rejects_regular_processes() {
    assert!(!is_kernel_thread("firefox"));
    assert!(!is_kernel_thread("kswapd0"));      // no brackets
    assert!(!is_kernel_thread("[incomplete"));   // missing closing bracket
    assert!(!is_kernel_thread("trailing]"));     // missing opening bracket
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd /home/ricsdeol/projects/swaptop/.worktrees/processes-screen
cargo test platform::linux::tests::kernel_thread 2>&1 | tail -5
```

Expected: `error[E0425]: cannot find function 'is_kernel_thread'`

- [ ] **Step 3: Add `is_kernel_thread` and `Users` to `LinuxBackend`, implement `process_list()`**

Replace the top of `src/platform/linux.rs` (the struct and impl block) with:

```rust
use std::path::{Path, PathBuf};

use color_eyre::Result;
use sysinfo::{System, Users};

use super::{Capabilities, ProcessRow, SwapBackend, SwapDevice, SwapInfo, SwapKind};

pub struct LinuxBackend {
    sys:   System,
    users: Users,
}

impl LinuxBackend {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let users = Users::new_with_refreshed_list();
        Self { sys, users }
    }
}
```

Then add the `process_list` method inside `impl SwapBackend for LinuxBackend`, after `swap_devices`:

```rust
fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
    self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let rows = self
        .sys
        .processes()
        .values()
        .filter(|p| !is_kernel_thread(&p.name().to_string_lossy()))
        .map(|p| {
            let user = p
                .user_id()
                .and_then(|uid| self.users.get_user_by_id(uid))
                .map(|u| u.name().to_string())
                .unwrap_or_default();
            ProcessRow {
                pid:     p.pid().as_u32(),
                name:    p.name().to_string_lossy().into_owned(),
                user,
                rss:     p.memory(),
                vms:     p.virtual_memory(),
                swap:    0,
                cpu_pct: p.cpu_usage(),
            }
        })
        .collect();
    Ok(rows)
}
```

Add the free function above the `// ── Parsing ───` section:

```rust
pub(crate) fn is_kernel_thread(name: &str) -> bool {
    name.starts_with('[') && name.ends_with(']')
}
```

- [ ] **Step 4: Add `process_list()` default impl to the trait in `src/platform/mod.rs`**

```rust
fn process_list(&mut self) -> Result<Vec<ProcessRow>> {
    Ok(vec![])
}
```

Add it after `swap_devices` in the trait body. The default returns an empty vec, so `MacosBackend`, `WindowsBackend`, and `BsdBackend` compile without changes.

- [ ] **Step 5: Run tests**

```bash
cargo test platform::linux::tests::kernel_thread 2>&1 | tail -5
```

Expected: `test result: ok. 2 passed`

- [ ] **Step 6: Verify full build and existing tests still pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: `test result: ok. 38 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add src/platform/mod.rs src/platform/linux.rs
git commit -m "feat(platform): add process_list() to SwapBackend, implement in LinuxBackend"
```

---

## Task 2: Add `SortColumn`, `SortDir`, and new Action variants

**Files:**
- Modify: `src/actions.rs`

- [ ] **Step 1: Replace `src/actions.rs` entirely**

```rust
use crate::platform::MemSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Pid,
    Name,
    User,
    Rss,
    Swap,
    Cpu,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    Refresh,
    NextTab,
    PrevTab,
    SelectTab(usize),
    UpdateSnapshot(MemSnapshot),
    NavigateUp,
    NavigateDown,
    SortBy(SortColumn),
    EnterFilterMode,
    FilterChar(char),
    FilterBackspace,
    ExitFilterMode,
}
```

- [ ] **Step 2: Verify build compiles**

```bash
cargo build 2>&1 | grep -E "^error" | head -10
```

Expected: no output (zero errors)

- [ ] **Step 3: Commit**

```bash
git add src/actions.rs
git commit -m "feat(actions): add SortColumn, SortDir, navigation and filter actions"
```

---

## Task 3: AppState — new fields, reducer cases, and helpers

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `src/app.rs`:

```rust
use crate::actions::{SortColumn, SortDir};
use crate::platform::ProcessRow;

fn make_process(pid: u32, name: &str, swap: u64) -> ProcessRow {
    ProcessRow { pid, name: name.to_string(), user: "user".to_string(),
                 rss: 0, vms: 0, swap, cpu_pct: 0.0 }
}

// ── Default sort ──────────────────────────────────────────────────────────────

#[test]
fn sort_col_defaults_to_swap() {
    let state = AppState::new(make_caps());
    assert_eq!(state.sort_col, SortColumn::Swap);
}

#[test]
fn sort_dir_defaults_to_desc() {
    let state = AppState::new(make_caps());
    assert_eq!(state.sort_dir, SortDir::Desc);
}

// ── SortBy ────────────────────────────────────────────────────────────────────

#[test]
fn sort_by_same_column_toggles_direction() {
    let mut state = AppState::new(make_caps());
    // starts Swap/Desc
    state.handle_action(Action::SortBy(SortColumn::Swap));
    assert_eq!(state.sort_col, SortColumn::Swap);
    assert_eq!(state.sort_dir, SortDir::Asc);
}

#[test]
fn sort_by_different_column_resets_to_desc() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::SortBy(SortColumn::Cpu));
    assert_eq!(state.sort_col, SortColumn::Cpu);
    assert_eq!(state.sort_dir, SortDir::Desc);
}

// ── Navigation ────────────────────────────────────────────────────────────────

#[test]
fn navigate_down_increments_selected_row() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.handle_action(Action::NavigateDown);
    assert_eq!(state.selected_row, 1);
}

#[test]
fn navigate_down_clamps_at_last_row() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.handle_action(Action::NavigateDown);
    state.handle_action(Action::NavigateDown); // beyond end
    assert_eq!(state.selected_row, 1);
}

#[test]
fn navigate_up_decrements_selected_row() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.handle_action(Action::NavigateDown);
    state.handle_action(Action::NavigateUp);
    assert_eq!(state.selected_row, 0);
}

#[test]
fn navigate_up_clamps_at_zero() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::NavigateUp);
    assert_eq!(state.selected_row, 0);
}

// ── Filter mode ───────────────────────────────────────────────────────────────

#[test]
fn enter_filter_mode_sets_flag() {
    let mut state = AppState::new(make_caps());
    state.handle_action(Action::EnterFilterMode);
    assert!(state.filter_mode);
}

#[test]
fn filter_char_appends_and_resets_selection() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "a", 0), make_process(2, "b", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.selected_row = 1;
    state.handle_action(Action::FilterChar('f'));
    assert_eq!(state.filter_text, "f");
    assert_eq!(state.selected_row, 0);
}

#[test]
fn filter_backspace_removes_last_char() {
    let mut state = AppState::new(make_caps());
    state.filter_text = "fi".to_string();
    state.handle_action(Action::FilterBackspace);
    assert_eq!(state.filter_text, "f");
}

#[test]
fn exit_filter_mode_clears_flag_keeps_text() {
    let mut state = AppState::new(make_caps());
    state.filter_mode = true;
    state.filter_text = "fox".to_string();
    state.handle_action(Action::ExitFilterMode);
    assert!(!state.filter_mode);
    assert_eq!(state.filter_text, "fox");
}

// ── filtered_len ─────────────────────────────────────────────────────────────

#[test]
fn filtered_len_with_empty_filter_returns_all() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "firefox", 0), make_process(2, "bash", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    assert_eq!(state.filtered_len(), 2);
}

#[test]
fn filtered_len_with_filter_returns_matches() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![make_process(1, "firefox", 0), make_process(2, "bash", 0)];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.filter_text = "fire".to_string();
    assert_eq!(state.filtered_len(), 1);
}

// ── UpdateSnapshot sorts ──────────────────────────────────────────────────────

#[test]
fn update_snapshot_sorts_by_swap_desc_by_default() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![
        make_process(1, "a", 100),
        make_process(2, "b", 500),
        make_process(3, "c", 200),
    ];
    state.handle_action(Action::UpdateSnapshot(snap));
    assert_eq!(state.processes[0].swap, 500);
    assert_eq!(state.processes[1].swap, 200);
    assert_eq!(state.processes[2].swap, 100);
}

#[test]
fn update_snapshot_clamps_selected_row_when_list_shrinks() {
    let mut state = AppState::new(make_caps());
    let mut snap = make_snapshot();
    snap.processes = vec![
        make_process(1, "a", 0), make_process(2, "b", 0), make_process(3, "c", 0),
    ];
    state.handle_action(Action::UpdateSnapshot(snap));
    state.selected_row = 2;
    // Now snapshot with only 1 process
    let mut snap2 = make_snapshot();
    snap2.processes = vec![make_process(1, "a", 0)];
    state.handle_action(Action::UpdateSnapshot(snap2));
    assert_eq!(state.selected_row, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test app::tests::sort_col_defaults 2>&1 | tail -5
```

Expected: compilation error — new fields and methods don't exist yet.

- [ ] **Step 3: Update imports and add new fields to `AppState`**

In `src/app.rs`, update the use line at the top:

```rust
use crate::actions::{Action, SortColumn, SortDir};
use crate::platform::{Capabilities, MemSnapshot, ProcessRow, SwapDevice};
```

Add the new fields to `AppState`:

```rust
pub struct AppState {
    pub active_tab:   Tab,
    pub ram_history:  VecDeque<(Instant, u64)>,
    pub swap_history: VecDeque<(Instant, u64)>,
    pub max_history:  usize,
    pub current:      Option<MemSnapshot>,
    pub devices:      Vec<SwapDevice>,
    pub capabilities: Capabilities,
    pub error_msg:    Option<String>,
    pub start_time:   Instant,
    pub should_quit:  bool,
    // Phase 2
    pub processes:    Vec<ProcessRow>,
    pub sort_col:     SortColumn,
    pub sort_dir:     SortDir,
    pub selected_row: usize,
    pub filter_text:  String,
    pub filter_mode:  bool,
}
```

Update `AppState::new` to initialize the new fields:

```rust
pub fn new(capabilities: Capabilities) -> Self {
    Self {
        active_tab:   Tab::Overview,
        ram_history:  VecDeque::new(),
        swap_history: VecDeque::new(),
        max_history:  3600,
        current:      None,
        devices:      Vec::new(),
        capabilities,
        error_msg:    None,
        start_time:   Instant::now(),
        should_quit:  false,
        // Phase 2
        processes:    Vec::new(),
        sort_col:     SortColumn::Swap,
        sort_dir:     SortDir::Desc,
        selected_row: 0,
        filter_text:  String::new(),
        filter_mode:  false,
    }
}
```

- [ ] **Step 4: Add `sort_processes()` and `filtered_len()` helpers**

Add these methods to the `impl AppState` block, before `handle_action`:

```rust
pub fn filtered_len(&self) -> usize {
    if self.filter_text.is_empty() {
        self.processes.len()
    } else {
        let lower = self.filter_text.to_lowercase();
        self.processes
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .count()
    }
}

fn sort_processes(&mut self) {
    // Clone to avoid simultaneous borrow of self while mutating self.processes.
    let col = self.sort_col.clone();
    let dir = self.sort_dir.clone();
    self.processes.sort_by(|a, b| {
        let ord = match col {
            SortColumn::Pid  => a.pid.cmp(&b.pid),
            SortColumn::Name => a.name.cmp(&b.name),
            SortColumn::User => a.user.cmp(&b.user),
            SortColumn::Rss  => a.rss.cmp(&b.rss),
            SortColumn::Swap => a.swap.cmp(&b.swap),
            SortColumn::Cpu  => a
                .cpu_pct
                .partial_cmp(&b.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal),
        };
        if dir == SortDir::Desc {
            ord.reverse()
        } else {
            ord
        }
    });
}
```

- [ ] **Step 5: Add reducer cases to `handle_action`**

Inside the `match action { ... }` block, add after the `Action::Refresh` arm:

```rust
Action::NavigateUp => {
    self.selected_row = self.selected_row.saturating_sub(1);
}

Action::NavigateDown => {
    let len = self.filtered_len();
    if len > 0 {
        self.selected_row = (self.selected_row + 1).min(len - 1);
    }
}

Action::SortBy(col) => {
    if col == self.sort_col {
        self.sort_dir = if self.sort_dir == SortDir::Asc {
            SortDir::Desc
        } else {
            SortDir::Asc
        };
    } else {
        self.sort_col = col;
        self.sort_dir = SortDir::Desc;
    }
    self.sort_processes();
}

Action::EnterFilterMode => {
    self.filter_mode = true;
}

Action::FilterChar(c) => {
    self.filter_text.push(c);
    self.selected_row = 0;
}

Action::FilterBackspace => {
    self.filter_text.pop();
    self.selected_row = 0;
}

Action::ExitFilterMode => {
    self.filter_mode = false;
}
```

Also update the `Action::UpdateSnapshot` arm to store processes and sort them:

```rust
Action::UpdateSnapshot(snapshot) => {
    self.ram_history.push_back((snapshot.timestamp, snapshot.ram.used));
    self.swap_history.push_back((snapshot.timestamp, snapshot.swap.used));
    while self.ram_history.len() > self.max_history {
        self.ram_history.pop_front();
    }
    while self.swap_history.len() > self.max_history {
        self.swap_history.pop_front();
    }
    self.devices    = snapshot.devices.clone();
    self.processes  = snapshot.processes.clone();
    self.sort_processes();
    let len = self.filtered_len();
    if len > 0 {
        self.selected_row = self.selected_row.min(len - 1);
    } else {
        self.selected_row = 0;
    }
    self.current   = Some(snapshot);
    self.error_msg = None;
}
```

Remove the `#[allow(dead_code)]` from the `capabilities` field since it's now used in Phase 2 UI.

- [ ] **Step 6: Run tests**

```bash
cargo test app::tests 2>&1 | tail -10
```

Expected: all new tests pass, existing tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src/app.rs
git commit -m "feat(app): add processes sort/filter/navigation state and reducer"
```

---

## Task 4: Collector — `processes_active` flag and parallel smaps

**Files:**
- Modify: `src/collector.rs`

- [ ] **Step 1: Replace `src/collector.rs`**

```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use color_eyre::Result;
use futures::future::join_all;

use crate::platform::{MemSnapshot, SwapBackend};

pub struct Collector {
    backend:          Box<dyn SwapBackend>,
    processes_active: Arc<AtomicBool>,
}

impl Collector {
    pub fn new(backend: Box<dyn SwapBackend>, processes_active: Arc<AtomicBool>) -> Self {
        Self { backend, processes_active }
    }

    pub async fn collect(&mut self) -> Result<MemSnapshot> {
        let ram     = self.backend.system_ram()?;
        let swap    = self.backend.system_swap()?;
        let devices = self.backend.swap_devices()?;

        let processes = if self.processes_active.load(Ordering::Relaxed) {
            let mut rows = self.backend.process_list()?;

            // Spawn one task per process to read /proc/{pid}/smaps in parallel.
            let handles: Vec<_> = rows
                .iter()
                .map(|p| {
                    let pid = p.pid;
                    tokio::spawn(async move { (pid, read_smaps_swap(pid)) })
                })
                .collect();

            let swap_map: HashMap<u32, u64> = join_all(handles)
                .await
                .into_iter()
                .filter_map(|r| r.ok())
                .collect();

            for row in &mut rows {
                if let Some(&bytes) = swap_map.get(&row.pid) {
                    row.swap = bytes;
                }
            }
            rows
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
    }
}

/// Read `/proc/{pid}/smaps` and sum all `VmSwap:` fields, returning bytes.
/// Returns 0 if the file is unreadable (process exited mid-collection).
fn read_smaps_swap(pid: u32) -> u64 {
    let content = std::fs::read_to_string(format!("/proc/{pid}/smaps"))
        .unwrap_or_default();
    content
        .lines()
        .filter_map(|l| l.strip_prefix("VmSwap:"))
        .filter_map(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        .sum::<u64>()
        * 1024
}
```

- [ ] **Step 2: Verify build**

```bash
cargo build 2>&1 | grep "^error" | head -10
```

Expected: error about `Collector::new` called with wrong number of arguments in `main.rs` — that's expected, we'll fix it in Task 5.

- [ ] **Step 3: Commit**

```bash
git add src/collector.rs
git commit -m "feat(collector): parallel smaps collection via processes_active flag"
```

---

## Task 5: main.rs — context-aware keyboard + `processes_active` wiring

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `src/main.rs`**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyCode, KeyModifiers};
use futures::{FutureExt, StreamExt};
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod platform;
mod tui;
mod ui;

use actions::{Action, SortColumn};
use app::{AppState, Tab};
use collector::Collector;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let backend          = platform::factory::detect();
    let caps             = backend.capabilities();
    let state            = Arc::new(Mutex::new(AppState::new(caps)));
    let processes_active = Arc::new(AtomicBool::new(false));
    let mut col          = Collector::new(backend, Arc::clone(&processes_active));

    // Initial collection before entering the TUI so the first frame is not blank.
    match col.collect().await {
        Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
        Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
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

    let result = run(&mut terminal, state, &mut col, processes_active, shutdown).await;
    tui::restore()?;
    result
}

async fn run(
    terminal:         &mut tui::Tui,
    state:            Arc<Mutex<AppState>>,
    col:              &mut Collector,
    processes_active: Arc<AtomicBool>,
    shutdown:         CancellationToken,
) -> Result<()> {
    let mut tick       = interval(Duration::from_secs(1));
    let mut frame_tick = interval(Duration::from_millis(33));
    let mut events     = EventStream::new();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            _ = tick.tick() => {
                match col.collect().await {
                    Ok(snap) => state.lock().expect("state mutex poisoned").handle_action(Action::UpdateSnapshot(snap)),
                    Err(e)   => state.lock().expect("state mutex poisoned").error_msg = Some(e.to_string()),
                }
            }

            _ = frame_tick.tick() => {
                let s = state.lock().expect("state mutex poisoned");
                terminal.draw(|f| ui::render(f, &s))?;
            }

            Some(Ok(event)) = events.next().fuse() => {
                if let CrosstermEvent::Key(key) = event {
                    let action = {
                        let s = state.lock().expect("state mutex poisoned");
                        resolve_key(&s, key)
                    };
                    if let Some(a) = action {
                        let mut s = state.lock().expect("state mutex poisoned");
                        s.handle_action(a);
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

fn resolve_key(state: &AppState, key: crossterm::event::KeyEvent) -> Option<Action> {
    // Priority 1: filter input captures almost all keys
    if state.filter_mode {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::ExitFilterMode),
            KeyCode::Backspace            => Some(Action::FilterBackspace),
            KeyCode::Char(c)              => Some(Action::FilterChar(c)),
            _                             => None,
        };
    }

    // Priority 2: processes-tab-specific keys
    if state.active_tab == Tab::Processes {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down  => return Some(Action::NavigateDown),
            KeyCode::Char('k') | KeyCode::Up    => return Some(Action::NavigateUp),
            KeyCode::Char('s')                  => {
                return Some(Action::SortBy(next_sort_column(&state.sort_col)));
            }
            KeyCode::Char('/')                  => return Some(Action::EnterFilterMode),
            _ => {}
        }
    }

    // Priority 3: global keys
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Action::Quit),
        KeyCode::Char('r')  => Some(Action::Refresh),
        KeyCode::Tab        => Some(Action::NextTab),
        KeyCode::BackTab    => Some(Action::PrevTab),
        KeyCode::Char('1')  => Some(Action::SelectTab(1)),
        KeyCode::Char('2')  => Some(Action::SelectTab(2)),
        KeyCode::Char('3')  => Some(Action::SelectTab(3)),
        KeyCode::Char('4')  => Some(Action::SelectTab(4)),
        _                   => None,
    }
}

fn next_sort_column(current: &SortColumn) -> SortColumn {
    match current {
        SortColumn::Swap => SortColumn::Cpu,
        SortColumn::Cpu  => SortColumn::Rss,
        SortColumn::Rss  => SortColumn::Pid,
        SortColumn::Pid  => SortColumn::Name,
        SortColumn::Name | SortColumn::User => SortColumn::Swap,
    }
}
```

- [ ] **Step 2: Verify build and tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: `test result: ok. N passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): context-aware keyboard routing and processes_active wiring"
```

---

## Task 6: Create `ui/processes.rs`

**Files:**
- Create: `src/ui/processes.rs`

- [ ] **Step 1: Write layout tests first**

Create `src/ui/processes.rs` with only the tests and the `build_layout` function:

```rust
use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(crate) fn build_layout(area: Rect, filter_mode: bool) -> (Rect, Option<Rect>) {
    if filter_mode {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (parts[1], Some(parts[0]))
    } else {
        (area, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_filter_mode_returns_full_area_and_no_filter_rect() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, false);
        assert_eq!(table_area, area);
        assert!(filter_area.is_none());
    }

    #[test]
    fn with_filter_mode_splits_top_3_rows_for_filter_bar() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        let filter = filter_area.unwrap();
        assert_eq!(filter.y,      0);
        assert_eq!(filter.height, 3);
        assert_eq!(table_area.y,  3);
        assert_eq!(table_area.height, 37);
    }

    #[test]
    fn filter_and_table_rects_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        assert_eq!(table_area.width, 120);
        assert_eq!(filter_area.unwrap().width, 120);
    }
}
```

- [ ] **Step 2: Run layout tests**

```bash
cargo test ui::processes::tests 2>&1 | tail -5
```

Expected: `test result: ok. 3 passed`

- [ ] **Step 3: Add full render implementation**

Replace the content of `src/ui/processes.rs` with:

```rust
use human_bytes::human_bytes;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::actions::{SortColumn, SortDir};
use crate::app::AppState;
use crate::platform::ProcessRow;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let (table_area, filter_area) = build_layout(area, state.filter_mode);

    if let Some(fa) = filter_area {
        render_filter_bar(f, fa, state);
    }

    let render_area = if !state.capabilities.has_per_process {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(table_area);
        render_platform_banner(f, parts[0]);
        parts[1]
    } else {
        table_area
    };

    render_table(f, render_area, state);
}

pub(crate) fn build_layout(area: Rect, filter_mode: bool) -> (Rect, Option<Rect>) {
    if filter_mode {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (parts[1], Some(parts[0]))
    } else {
        (area, None)
    }
}

fn render_filter_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let text = format!(" {}_", state.filter_text);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Filter ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

fn render_platform_banner(f: &mut Frame, area: Rect) {
    let p = Paragraph::new(Span::styled(
        "  Swap usage per process is not available on this platform",
        Style::default().fg(Color::Yellow),
    ));
    f.render_widget(p, area);
}

fn render_table(f: &mut Frame, area: Rect, state: &AppState) {
    let has_per_process = state.capabilities.has_per_process;

    // Build filtered list
    let lower = state.filter_text.to_lowercase();
    let visible: Vec<&ProcessRow> = if lower.is_empty() {
        state.processes.iter().collect()
    } else {
        state.processes
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .collect()
    };

    if visible.is_empty() {
        let p = Paragraph::new("  No processes found")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }

    let header = Row::new(vec![
        header_cell("PID",  &SortColumn::Pid,  state),
        header_cell("Name", &SortColumn::Name, state),
        header_cell("User", &SortColumn::User, state),
        header_cell("RSS",  &SortColumn::Rss,  state),
        header_cell("Swap", &SortColumn::Swap, state),
        header_cell("CPU%", &SortColumn::Cpu,  state),
    ])
    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = visible
        .iter()
        .map(|p| {
            let swap_cell = if has_per_process {
                Cell::from(format!("{:>10}", human_bytes(p.swap as f64)))
            } else {
                Cell::from(format!("{:>10}", "—"))
                    .style(Style::default().fg(Color::DarkGray))
            };
            Row::new(vec![
                Cell::from(format!("{:>6}", p.pid)),
                Cell::from(p.name.clone()),
                Cell::from(p.user.clone()),
                Cell::from(format!("{:>10}", human_bytes(p.rss as f64))),
                swap_cell,
                Cell::from(format!("{:>5.1}%", p.cpu_pct)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(7),
        Constraint::Min(20),
        Constraint::Length(12),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(7),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut table_state = TableState::default();
    table_state.select(Some(state.selected_row));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn header_cell<'a>(label: &'a str, col: &SortColumn, state: &AppState) -> Cell<'a> {
    let indicator = if col == &state.sort_col {
        if state.sort_dir == SortDir::Desc { " ▾" } else { " ▲" }
    } else {
        ""
    };
    Cell::from(format!("{label}{indicator}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn without_filter_mode_returns_full_area_and_no_filter_rect() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, false);
        assert_eq!(table_area, area);
        assert!(filter_area.is_none());
    }

    #[test]
    fn with_filter_mode_splits_top_3_rows_for_filter_bar() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        let filter = filter_area.unwrap();
        assert_eq!(filter.y,          0);
        assert_eq!(filter.height,     3);
        assert_eq!(table_area.y,      3);
        assert_eq!(table_area.height, 37);
    }

    #[test]
    fn filter_and_table_rects_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        assert_eq!(table_area.width,         120);
        assert_eq!(filter_area.unwrap().width, 120);
    }
}
```

- [ ] **Step 4: Run all tests**

```bash
cargo test 2>&1 | tail -10
```

Expected: all tests pass, zero failures.

- [ ] **Step 5: Commit**

```bash
git add src/ui/processes.rs
git commit -m "feat(ui): add processes table with sort indicator, filter bar, and empty state"
```

---

## Task 7: Wire routing, update statusbar, write user docs

**Files:**
- Modify: `src/ui/mod.rs`
- Modify: `src/ui/statusbar.rs`
- Create: `docs/processes-screen.md`

- [ ] **Step 1: Add `processes` module and route in `src/ui/mod.rs`**

Add `mod processes;` at the top with the other module declarations:

```rust
mod design;
mod overview;
mod processes;
mod statusbar;
```

Update the match in `render()`:

```rust
match state.active_tab {
    Tab::Overview   => overview::render(f, layout[1], state),
    Tab::Processes  => processes::render(f, layout[1], state),
    _               => render_coming_soon(f, layout[1]),
}
```

- [ ] **Step 2: Update `src/ui/statusbar.rs` to show context-aware hints**

Replace the import line `use crate::app::AppState;` with:

```rust
use crate::app::{AppState, Tab};
```

Then replace the `render` function body:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let keys: &[(&str, &str)] = if state.filter_mode {
        &[
            ("Enter/Esc", "exit filter"),
            ("Backspace", "delete char"),
        ]
    } else if state.active_tab == Tab::Processes {
        &[
            ("j/k", "navigate"),
            ("s", "sort"),
            ("/", "filter"),
            ("Tab", "next tab"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("q", "quit"),
            ("Tab", "next tab"),
            ("1-4", "switch tab"),
            ("r", "refresh"),
        ]
    };

    let mut spans: Vec<Span> = keys
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::styled(
                    format!(" {key} "),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {desc}  "),
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        })
        .collect();

    if let Some(err) = &state.error_msg {
        spans.push(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

- [ ] **Step 3: Create `docs/processes-screen.md`**

```markdown
# Processes Screen

The Processes screen (tab `2`) shows all running user-space processes with their
memory and CPU usage. Press `2` or `Tab` to reach it.

## Columns

| Column | Description |
|--------|-------------|
| PID    | Process ID |
| Name   | Executable name |
| User   | Owner of the process |
| RSS    | Resident memory in use (RAM) |
| Swap   | Swap space in use (Linux only) |
| CPU%   | Current CPU usage |

The active sort column is marked with `▾` (descending) or `▲` (ascending).

## Navigation

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `s` | Cycle sort column (Swap → CPU% → RSS → PID → Name → Swap) |
| `/` | Open filter input |
| `Enter` | Open process detail *(Phase 3)* |

## Filtering

Press `/` to open the filter bar. Type to narrow the list by process name.
Press `Enter` or `Esc` to close the filter bar — the filter text stays active
until you clear it with `Backspace`.

## Platform notes

**Linux:** All columns available. Swap per process is read from `/proc/PID/smaps`.

**macOS / other:** The Swap column shows `—`. Swap per process is not available
without private kernel APIs.
```

- [ ] **Step 4: Run full test suite and clippy**

```bash
cargo test 2>&1 | tail -5
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -10
```

Expected: all tests pass, zero clippy errors.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs src/ui/statusbar.rs docs/processes-screen.md
git commit -m "feat(ui): wire processes tab, context-aware statusbar, user docs"
```

---

## Task 8: Final verification

- [ ] **Step 1: Clean build**

```bash
cargo build 2>&1 | tail -3
```

Expected: `Finished \`dev\` profile ... in ...s` with zero warnings.

- [ ] **Step 2: Clippy clean**

```bash
cargo clippy -- -D warnings 2>&1 | tail -3
```

Expected: `Finished ... in ...s` with no `error` lines.

- [ ] **Step 3: All tests pass**

```bash
cargo test 2>&1 | tail -5
```

Expected: all tests pass, 0 failed.

- [ ] **Step 4: Final commit (if any files unstaged)**

```bash
git status
```

If clean, no action needed. All work should be in individual commits from prior tasks.
