# Quality Improvement Pass — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish test coverage tooling, refactor main.rs into testable modules, clean up dead_code suppressions, and harden CI.

**Architecture:** Extract key-handling logic from main.rs into input.rs. Add MockBackend for collector tests. Add proptest for parser property tests. Slim tokio features and add coverage to CI.

**Tech Stack:** Rust 2024, cargo-llvm-cov, proptest, tokio (slimmed features), GitHub Actions

**Branch:** `chore/improve` (create from `main`)

---

## File Map

| Action | File | Responsibility |
|--------|------|----------------|
| Create | `src/input.rs` | Key event → Action resolution, extracted from main.rs |
| Modify | `src/main.rs` | Event loop only — delegates to input.rs |
| Modify | `src/collector.rs` | Add `#[cfg(test)]` MockBackend + collector tests |
| Modify | `src/platform/mod.rs` | Remove `#[allow(dead_code)]` from SwapBackend trait |
| Modify | `src/platform/types.rs` | Remove `#![allow(dead_code)]` file-level suppression |
| Modify | `src/platform/proc_reader.rs` | Remove all `#[allow(dead_code)]` suppressions |
| Modify | `src/platform/linux.rs` | Add clarifying comment on thread::sleep, add proptest |
| Modify | `Cargo.toml` | Add proptest dev-dep, slim tokio features |
| Modify | `.github/workflows/ci.yml` | Add fmt check + coverage step |

---

## Task 1: Create branch

**Files:**
- None (git only)

- [ ] **Step 1: Create and switch to the chore/improve branch**

```bash
git checkout -b chore/improve
```

- [ ] **Step 2: Verify branch**

Run: `git branch --show-current`
Expected: `chore/improve`

---

## Task 2: Add proptest dev-dependency and slim tokio features

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Update Cargo.toml**

Replace the tokio line and add dev-dependencies section:

```toml
[dependencies]
ratatui     = "0.30.0"
crossterm   = { version = "0.29.0", features = ["event-stream"] }
tokio       = { version = "1.51.1", features = ["macros", "rt-multi-thread", "time", "sync", "signal"] }
tokio-util  = "0.7.18"
futures     = "0.3.32"
sysinfo     = "0.38.4"
nix         = { version = "0.31.2", features = ["process", "signal", "mount", "user"] }
color-eyre  = "0.6.5"
human_bytes = "0.4.3"
glob        = "0.3.3"

[dev-dependencies]
proptest = "1"
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully with zero warnings.

- [ ] **Step 3: Run existing tests to confirm nothing broke**

Run: `cargo test`
Expected: All 85 tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: slim tokio features, add proptest dev-dep"
```

---

## Task 3: Add MockBackend and Collector tests

**Files:**
- Modify: `src/collector.rs`

- [ ] **Step 1: Write the failing tests**

Append the following `#[cfg(test)]` module to the bottom of `src/collector.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use crate::platform::{Capabilities, MemSnapshot, ProcessRow, SwapBackend, SwapDevice, SwapInfo};

    struct MockBackend {
        ram:       SwapInfo,
        swap:      SwapInfo,
        devices:   Vec<SwapDevice>,
        processes: Vec<ProcessRow>,
        fail:      bool,
    }

    impl MockBackend {
        fn healthy() -> Self {
            Self {
                ram:       SwapInfo::new(16_000_000, 8_000_000),
                swap:      SwapInfo::new(4_000_000, 1_000_000),
                devices:   vec![],
                processes: vec![ProcessRow {
                    pid: 1, name: "init".into(), user: "root".into(),
                    rss: 1024, vms: 2048, swap: 512, cpu_pct: 0.5,
                }],
                fail: false,
            }
        }

        fn failing() -> Self {
            Self { fail: true, ..Self::healthy() }
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
        fn swap_on(&self, _device: &Path) -> color_eyre::Result<()> { Ok(()) }
        fn swap_off(&self, _device: &Path) -> color_eyre::Result<()> { Ok(()) }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                can_swap_on: true, can_swap_off: true, has_per_process: true,
                has_device_list: true, can_create_swap: true, requires_root: false,
            }
        }
    }

    #[tokio::test]
    async fn collect_assembles_snapshot_from_backend() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert_eq!(snap.ram.total, 16_000_000);
        assert_eq!(snap.swap.total, 4_000_000);
        assert!(snap.devices.is_empty());
    }

    #[tokio::test]
    async fn collect_skips_processes_when_inactive() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert!(snap.processes.is_empty());
    }

    #[tokio::test]
    async fn collect_includes_processes_when_active() {
        let backend = MockBackend::healthy();
        let active = Arc::new(AtomicBool::new(true));
        let mut col = Collector::new(Box::new(backend), active);
        let snap = col.collect().await.unwrap();
        assert_eq!(snap.processes.len(), 1);
        assert_eq!(snap.processes[0].name, "init");
    }

    #[tokio::test]
    async fn collect_propagates_backend_error() {
        let backend = MockBackend::failing();
        let active = Arc::new(AtomicBool::new(false));
        let mut col = Collector::new(Box::new(backend), active);
        let result = col.collect().await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib collector`
Expected: 4 new tests pass. The MockBackend correctly implements the trait.

- [ ] **Step 3: Commit**

```bash
git add src/collector.rs
git commit -m "test: add MockBackend and Collector unit tests"
```

---

## Task 4: Extract `src/input.rs` from `main.rs`

**Files:**
- Create: `src/input.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/input.rs` with extracted functions and imports**

Create `src/input.rs` with the following content:

```rust
use std::sync::{Arc, Mutex};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::actions::{Action, DeviceOpKind, SortColumn};
use crate::app::{AppState, Tab};

pub fn resolve_key(
    key:            KeyEvent,
    active_tab:     &Tab,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    filter_mode:    bool,
    sort_col:       &SortColumn,
    state:          &Arc<Mutex<AppState>>,
) -> Option<Action> {
    // Priority 1: filter input captures almost all keys
    if filter_mode {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => Some(Action::ExitFilterMode),
            KeyCode::Backspace            => Some(Action::FilterBackspace),
            KeyCode::Char(c)              => Some(Action::FilterChar(c)),
            _                             => None,
        };
    }

    // Global keys (always active)
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Some(Action::Quit),
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return Some(Action::Quit),
        KeyCode::Tab     => return Some(Action::NextTab),
        KeyCode::BackTab => return Some(Action::PrevTab),
        KeyCode::Char('1') => return Some(Action::SelectTab(1)),
        KeyCode::Char('2') => return Some(Action::SelectTab(2)),
        KeyCode::Char('3') => return Some(Action::SelectTab(3)),
        KeyCode::Char('4') => return Some(Action::SelectTab(4)),
        _ => {}
    }

    // Tab-specific keys
    match active_tab {
        Tab::Processes => {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down  => return Some(Action::NavigateDown),
                KeyCode::Char('k') | KeyCode::Up    => return Some(Action::NavigateUp),
                KeyCode::Char('s')                  => {
                    return Some(Action::SortBy(next_sort_column(sort_col)));
                }
                KeyCode::Char('/')                  => return Some(Action::EnterFilterMode),
                KeyCode::Char('r')                  => return Some(Action::Refresh),
                _ => {}
            }
        }
        Tab::Devices => {
            return handle_devices_key(
                key.code,
                confirm_action,
                selected_dev,
                has_devices,
                state,
            );
        }
        _ => {
            if let KeyCode::Char('r') = key.code {
                return Some(Action::Refresh);
            }
        }
    }

    None
}

fn handle_devices_key(
    code:           KeyCode,
    confirm_action: Option<&DeviceOpKind>,
    selected_dev:   usize,
    has_devices:    bool,
    state:          &Arc<Mutex<AppState>>,
) -> Option<Action> {
    if let Some(kind) = confirm_action {
        // Modal is open — only 's'/Enter and Esc are active
        return match code {
            KeyCode::Char('s') | KeyCode::Enter => {
                let path = state
                    .lock()
                    .expect("state mutex poisoned")
                    .devices
                    .get(selected_dev)?
                    .path
                    .clone();
                Some(Action::ExecuteDeviceOp { path, kind: kind.clone() })
            }
            KeyCode::Esc => Some(Action::CancelConfirm),
            _ => None,
        };
    }

    match code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::DeviceDown),
        KeyCode::Char('k') | KeyCode::Up   => Some(Action::DeviceUp),
        KeyCode::Char('r') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Reset))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('o') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::On))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        KeyCode::Char('f') if has_devices  => {
            if nix::unistd::geteuid().is_root() {
                Some(Action::RequestConfirm(DeviceOpKind::Off))
            } else {
                Some(Action::SetError("Requires root — run: sudo swaptop".to_string()))
            }
        }
        _ => None,
    }
}

pub fn next_sort_column(current: &SortColumn) -> SortColumn {
    match current {
        SortColumn::Swap => SortColumn::Cpu,
        SortColumn::Cpu  => SortColumn::Rss,
        SortColumn::Rss  => SortColumn::Pid,
        SortColumn::Pid  => SortColumn::Name,
        SortColumn::Name | SortColumn::User => SortColumn::Swap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_column_cycles_through_all_columns() {
        assert_eq!(next_sort_column(&SortColumn::Swap), SortColumn::Cpu);
        assert_eq!(next_sort_column(&SortColumn::Cpu),  SortColumn::Rss);
        assert_eq!(next_sort_column(&SortColumn::Rss),  SortColumn::Pid);
        assert_eq!(next_sort_column(&SortColumn::Pid),  SortColumn::Name);
        assert_eq!(next_sort_column(&SortColumn::Name), SortColumn::Swap);
    }

    #[test]
    fn user_column_falls_back_to_swap() {
        assert_eq!(next_sort_column(&SortColumn::User), SortColumn::Swap);
    }
}
```

- [ ] **Step 2: Update `main.rs` — add `mod input;`, replace function calls**

Replace the entire `src/main.rs` with:

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream};
use futures::{FutureExt, StreamExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;

mod actions;
mod app;
mod collector;
mod input;
mod platform;
mod tui;
mod ui;

use actions::{Action, DeviceOp, DeviceOpKind, OpStatus};
use app::{AppState, Tab};
use collector::Collector;
use platform::linux::LinuxBackend;

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

    // Channel for background tasks (spawn_blocking) to send actions back.
    let (action_tx, mut action_rx) = mpsc::unbounded_channel::<Action>();

    loop {
        if state.lock().expect("state mutex poisoned").should_quit {
            break;
        }

        tokio::select! {
            _ = shutdown.cancelled() => break,

            // Background task result (e.g. DeviceOpUpdate from swapon/swapoff)
            Some(action) = action_rx.recv() => {
                state.lock().expect("state mutex poisoned").handle_action(action);
            }

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
                    // Read tab-relevant state before dropping the lock
                    let (active_tab, confirm_action, selected_dev, has_devices,
                         filter_mode, sort_col) = {
                        let s = state.lock().expect("state mutex poisoned");
                        (
                            s.active_tab.clone(),
                            s.confirm_action.clone(),
                            s.selected_dev,
                            !s.devices.is_empty(),
                            s.filter_mode,
                            s.sort_col,
                        )
                    };

                    let action = input::resolve_key(
                        key,
                        &active_tab,
                        confirm_action.as_ref(),
                        selected_dev,
                        has_devices,
                        filter_mode,
                        &sort_col,
                        &state,
                    );

                    // Spawn background task before dispatching ExecuteDeviceOp to AppState
                    if let Some(Action::ExecuteDeviceOp { ref path, ref kind }) = action {
                        let tx   = action_tx.clone();
                        let path = path.clone();
                        let kind = kind.clone();
                        tokio::task::spawn_blocking(move || {
                            let backend = LinuxBackend::new();
                            let result = match kind {
                                DeviceOpKind::On    => backend.swap_on(&path),
                                DeviceOpKind::Off   => backend.swap_off(&path),
                                DeviceOpKind::Reset => backend.swap_reset(&path),
                            };
                            let status = match result {
                                Ok(_)  => OpStatus::Done,
                                Err(e) => OpStatus::Error(e.to_string()),
                            };
                            let _ = tx.send(Action::DeviceOpUpdate(DeviceOp { path, kind, status }));
                        });
                    }

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
```

- [ ] **Step 3: Verify it compiles and all tests pass**

Run: `cargo build && cargo test`
Expected: Compiles cleanly. All 85 tests pass (2 sort tests now run from input.rs instead of main.rs).

- [ ] **Step 4: Commit**

```bash
git add src/input.rs src/main.rs
git commit -m "refactor: extract input handling from main.rs into input.rs"
```

---

## Task 5: Add input handler tests

**Files:**
- Modify: `src/input.rs`

- [ ] **Step 1: Add tests to the existing `#[cfg(test)]` module in `src/input.rs`**

Append inside the `mod tests` block (after the existing two tests).

**Important:** `Action` does not derive `PartialEq` (it contains `MemSnapshot` which holds `Instant`), so all assertions on `Action` must use `matches!` instead of `assert_eq!`.

```rust
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::sync::{Arc, Mutex};
    use crate::app::{AppState, Tab};
    use crate::actions::SortColumn;
    use crate::platform::Capabilities;

    fn make_caps() -> Capabilities {
        Capabilities {
            can_swap_on: true, can_swap_off: true, has_per_process: true,
            has_device_list: true, can_create_swap: true, requires_root: true,
        }
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::new(make_caps())))
    }

    // ── Filter mode ──────────────────────────────────────────────────────

    #[test]
    fn filter_mode_captures_printable_chars() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Char('a')), &Tab::Processes, None, 0, false, true,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::FilterChar('a'))));
    }

    #[test]
    fn filter_mode_esc_exits() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Esc), &Tab::Processes, None, 0, false, true,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_enter_exits() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Enter), &Tab::Processes, None, 0, false, true,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::ExitFilterMode)));
    }

    #[test]
    fn filter_mode_backspace_deletes() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Backspace), &Tab::Processes, None, 0, false, true,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::FilterBackspace)));
    }

    // ── Global keys ──────────────────────────────────────────────────────

    #[test]
    fn global_quit_keys_work_from_any_tab() {
        let state = make_state();
        for tab in [Tab::Overview, Tab::Processes, Tab::Devices, Tab::CreateSwap] {
            let q = resolve_key(
                key(KeyCode::Char('q')), &tab, None, 0, false, false,
                &SortColumn::Swap, &state,
            );
            assert!(matches!(q, Some(Action::Quit)), "q should quit from {tab:?}");

            let ctrl_c = resolve_key(
                ctrl('c'), &tab, None, 0, false, false,
                &SortColumn::Swap, &state,
            );
            assert!(matches!(ctrl_c, Some(Action::Quit)), "Ctrl+C should quit from {tab:?}");
        }
    }

    #[test]
    fn tab_keys_cycle_correctly() {
        let state = make_state();
        let fwd = resolve_key(
            key(KeyCode::Tab), &Tab::Overview, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(fwd, Some(Action::NextTab)));

        let back = resolve_key(
            key(KeyCode::BackTab), &Tab::Overview, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(back, Some(Action::PrevTab)));
    }

    #[test]
    fn number_keys_select_tabs() {
        let state = make_state();
        for n in [1_usize, 2, 3, 4] {
            let c = char::from_digit(n as u32, 10).unwrap();
            let action = resolve_key(
                key(KeyCode::Char(c)), &Tab::Overview, None, 0, false, false,
                &SortColumn::Swap, &state,
            );
            assert!(matches!(action, Some(Action::SelectTab(v)) if v == n));
        }
    }

    // ── Tab-specific keys ────────────────────────────────────────────────

    #[test]
    fn process_tab_keys_only_fire_on_process_tab() {
        let state = make_state();
        // 'j' on Processes tab → NavigateDown
        let on_proc = resolve_key(
            key(KeyCode::Char('j')), &Tab::Processes, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(on_proc, Some(Action::NavigateDown)));

        // 'j' on Overview tab → None (not a global key)
        let on_overview = resolve_key(
            key(KeyCode::Char('j')), &Tab::Overview, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(on_overview.is_none());
    }

    #[test]
    fn slash_enters_filter_mode_on_processes() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Char('/')), &Tab::Processes, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::EnterFilterMode)));
    }

    #[test]
    fn refresh_key_works_on_overview_tab() {
        let state = make_state();
        let action = resolve_key(
            key(KeyCode::Char('r')), &Tab::Overview, None, 0, false, false,
            &SortColumn::Swap, &state,
        );
        assert!(matches!(action, Some(Action::Refresh)));
    }
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cargo test --lib input`
Expected: All 13 tests pass (2 existing + 11 new).

- [ ] **Step 3: Commit**

```bash
git add src/input.rs
git commit -m "test: add input handler unit tests"
```

---

## Task 6: Remove `#[allow(dead_code)]` suppressions

**Files:**
- Modify: `src/platform/mod.rs`
- Modify: `src/platform/types.rs`
- Modify: `src/platform/proc_reader.rs`

- [ ] **Step 1: Remove `#[allow(dead_code)]` from SwapBackend trait in `src/platform/mod.rs`**

In `src/platform/mod.rs`, remove the `#[allow(dead_code)]` above the trait definition. Change:

```rust
#[allow(dead_code)]
pub trait SwapBackend: Send + Sync {
```

to:

```rust
pub trait SwapBackend: Send + Sync {
```

- [ ] **Step 2: Remove `#![allow(dead_code)]` from `src/platform/types.rs`**

Remove the first line `#![allow(dead_code)]` from `src/platform/types.rs`.

- [ ] **Step 3: Remove all `#[allow(dead_code)]` from `src/platform/proc_reader.rs`**

Remove the following 5 `#[allow(dead_code)]` attributes:
- Line 8: `#[allow(dead_code)]` above `struct StatusInfo`
- Line 17: `#[allow(dead_code)]` above `pub struct ProcReader`
- Line 25: `#[allow(dead_code)]` above `impl ProcReader`
- Line 134: `#[allow(dead_code)]` above `fn is_kernel_thread`
- Line 139: `#[allow(dead_code)]` above `fn parse_status`
- Line 170: `#[allow(dead_code)]` above `fn parse_kb_value`
- Line 179: `#[allow(dead_code)]` above `fn parse_stat_cpu_ticks`

- [ ] **Step 4: Verify it compiles with no warnings**

Run: `cargo clippy -- -D warnings`
Expected: Passes clean. If any item is genuinely dead, clippy will flag it — delete the dead code rather than re-adding the suppression.

- [ ] **Step 5: Run tests**

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/platform/mod.rs src/platform/types.rs src/platform/proc_reader.rs
git commit -m "chore: remove unnecessary #[allow(dead_code)] suppressions"
```

---

## Task 7: Add clarifying comment on `std::thread::sleep` in `swap_reset`

**Files:**
- Modify: `src/platform/linux.rs`

- [ ] **Step 1: Add comment above the sleep call**

In `src/platform/linux.rs`, in the `swap_reset` method (line 74), change:

```rust
    fn swap_reset(&self, device: &Path) -> Result<()> {
        self.swap_off(device)?;
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.swap_on(device)
    }
```

to:

```rust
    fn swap_reset(&self, device: &Path) -> Result<()> {
        self.swap_off(device)?;
        // NOTE: This runs inside spawn_blocking, so std::thread::sleep is
        // appropriate here. The SwapBackend trait is synchronous by design.
        std::thread::sleep(std::time::Duration::from_millis(100));
        self.swap_on(device)
    }
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles clean.

- [ ] **Step 3: Commit**

```bash
git add src/platform/linux.rs
git commit -m "docs: clarify why std::thread::sleep is used in swap_reset"
```

---

## Task 8: Add proptest for parsers

**Files:**
- Modify: `src/platform/linux.rs`
- Modify: `src/platform/proc_reader.rs`

- [ ] **Step 1: Add proptest to `src/platform/linux.rs`**

Append inside the existing `mod tests` block in `src/platform/linux.rs` (before the final `}`):

```rust
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        fn valid_swap_line() -> impl Strategy<Value = String> {
            (
                "/[a-z]{3,10}",         // path
                prop::sample::select(vec!["partition", "file"]),  // type
                1_u64..10_000_000,      // total_kb
                0_u64..10_000_000,      // used_kb
                -32768_i16..32767,      // priority
            ).prop_map(|(path, kind, total, used, prio)| {
                let used = used.min(total);
                format!("{path}\t\t{kind}\t{total}\t{used}\t{prio}")
            })
        }

        proptest! {
            #[test]
            fn valid_line_always_produces_device(line in valid_swap_line()) {
                let content = format!("{HEADER}{line}\n");
                let devices = parse_proc_swaps(&content);
                prop_assert_eq!(devices.len(), 1);
            }

            #[test]
            fn malformed_lines_never_panic(line in ".*") {
                let content = format!("{HEADER}{line}\n");
                let _ = parse_proc_swaps(&content); // must not panic
            }

            #[test]
            fn parsed_bytes_are_kb_times_1024(
                total_kb in 1_u64..10_000_000,
                used_kb in 0_u64..10_000_000,
            ) {
                let used_kb = used_kb.min(total_kb);
                let line = format!("/dev/sda2\t\tpartition\t{total_kb}\t{used_kb}\t-1\n");
                let content = format!("{HEADER}{line}");
                let devices = parse_proc_swaps(&content);
                prop_assert_eq!(devices.len(), 1);
                prop_assert_eq!(devices[0].total, total_kb * 1024);
                prop_assert_eq!(devices[0].used, used_kb * 1024);
            }
        }
    }
```

- [ ] **Step 2: Add proptest to `src/platform/proc_reader.rs`**

Append inside the existing `mod tests` block in `src/platform/proc_reader.rs` (before the final `}`):

```rust
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn parse_status_extracts_name_and_uid(
                name in "[a-zA-Z_][a-zA-Z0-9_]{0,15}",
                uid in 0_u32..65534,
            ) {
                let content = format!(
                    "Name:\t{name}\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\nVmRSS:\t 0 kB\nVmSize:\t 0 kB\nVmSwap:\t 0 kB\n"
                );
                let info = parse_status(&content);
                prop_assert!(info.is_some(), "parse_status returned None for valid input");
                let info = info.unwrap();
                prop_assert_eq!(&info.name, &name);
                prop_assert_eq!(info.uid, uid);
            }

            #[test]
            fn parse_status_never_panics(content in ".*") {
                let _ = parse_status(&content); // must not panic
            }

            #[test]
            fn parse_stat_sums_utime_and_stime(
                utime in 0_u64..1_000_000,
                stime in 0_u64..1_000_000,
            ) {
                let content = format!(
                    "1234 (test) S 1000 1234 1234 0 -1 4194304 \
                     1000 0 100 0 {utime} {stime} 0 0 20 0 4 0 1000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                );
                let result = parse_stat_cpu_ticks(&content);
                prop_assert_eq!(result, Some(utime + stime));
            }

            #[test]
            fn parse_stat_handles_arbitrary_comm(
                comm in "[a-zA-Z0-9 ()]{1,30}",
                utime in 0_u64..1_000_000,
                stime in 0_u64..1_000_000,
            ) {
                let content = format!(
                    "1234 ({comm}) S 1000 1234 1234 0 -1 4194304 \
                     1000 0 100 0 {utime} {stime} 0 0 20 0 4 0 1000 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"
                );
                let result = parse_stat_cpu_ticks(&content);
                // With nested parens the rfind(')') strategy should still work
                // as long as the closing paren of comm is the last ')' before fields.
                // Some generated strings may break this — that's a real edge case to discover.
                if let Some(ticks) = result {
                    prop_assert!(ticks >= utime.min(stime));
                }
            }
        }
    }
```

- [ ] **Step 3: Run all tests including proptests**

Run: `cargo test`
Expected: All tests pass (existing + proptests). Proptest runs 256 cases per test by default.

- [ ] **Step 4: Commit**

```bash
git add src/platform/linux.rs src/platform/proc_reader.rs
git commit -m "test: add proptest property-based tests for parsers"
```

---

## Task 9: Update CI workflow

**Files:**
- Modify: `.github/workflows/ci.yml`

- [ ] **Step 1: Update CI workflow**

Replace `.github/workflows/ci.yml` with:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: "1.94.1"
          components: rustfmt, clippy, llvm-tools-preview

      - name: Cache cargo registry and build artifacts
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-

      - name: Check formatting
        run: cargo fmt --check

      - name: Build
        run: cargo build

      - name: Clippy (deny warnings)
        run: cargo clippy -- -D warnings

      - name: Tests
        run: cargo test

      - uses: taiki-e/install-action@cargo-llvm-cov

      - name: Coverage
        run: cargo llvm-cov --fail-under-lines 70
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt check and coverage with cargo-llvm-cov"
```

---

## Task 10: Final verification

**Files:**
- None (verification only)

- [ ] **Step 1: Full build**

Run: `cargo build`
Expected: Zero warnings.

- [ ] **Step 2: Clippy**

Run: `cargo clippy -- -D warnings`
Expected: Passes clean.

- [ ] **Step 3: Formatting**

Run: `cargo fmt --check`
Expected: No formatting issues (run `cargo fmt` if needed).

- [ ] **Step 4: All tests**

Run: `cargo test`
Expected: All tests pass (85 original + ~4 collector + ~11 input + ~7 proptest = ~107 tests).

- [ ] **Step 5: Verify main.rs is under 170 lines**

Run: `wc -l src/main.rs`
Expected: < 170 lines.

- [ ] **Step 6: Verify no stale dead_code allows**

Run: `grep -rn 'allow(dead_code)' src/`
Expected: Only `src/actions.rs:24` (`DeviceOp::kind` field).

- [ ] **Step 7: Check coverage locally (if cargo-llvm-cov installed)**

Run: `cargo llvm-cov`
Expected: Coverage report showing line coverage. Target: 70%+.
