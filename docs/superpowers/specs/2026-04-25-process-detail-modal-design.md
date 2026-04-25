# Design Spec: Process Detail Modal

**Date:** 2026-04-25
**Topic:** process-detail-modal
**Status:** Approved

---

## Summary

Add a modal detail screen triggered by pressing `Enter` on a process in the Processes tab. The modal displays historical RAM and swap usage charts (15 minutes / 900 data points), extended process metadata (executable path, user, threads, status), and supports killing the process (`k` with confirmation).

---

## Goals

1. Allow users to inspect a single process in depth without leaving swaptop.
2. Provide visual history charts for RAM and swap, reusing the existing Sparkline infrastructure from the Overview tab.
3. Support process termination (`SIGTERM`) with a confirmation step.
4. Maintain the existing architecture: reducer is pure, I/O goes through `PlatformBridge`, UI is read-only.

---

## Non-Goals

1. Persistent history across app restarts (history remains in-memory only).
2. Editable process properties (priority, cgroup, etc.).
3. Real-time process tree / child process inspection.
4. Multiple simultaneous detail modals (only one at a time).

---

## Architecture

### State Changes

#### New Types

```rust
// In platform/types.rs — extend ProcessRow
pub struct ProcessRow {
    pub pid: u32,
    pub name: String,
    pub exe_path: Option<String>,
    pub user: String,
    pub rss: u64,
    pub swap: u64,
    pub cpu_pct: f32,
    pub threads: u32,      // NEW
    pub status: char,      // NEW — 'R', 'S', 'D', etc. from /proc/PID/stat
}

// In app/mod.rs — new field in AppState
pub process_history: HashMap<u32, ProcessHistory>,
pub selected_process_detail: Option<u32>,          // PID being viewed, None = closed
pub process_detail_confirm_kill: bool,               // show kill confirmation line

pub struct ProcessHistory {
    pub rss_history: VecDeque<(Instant, u64)>,   // capped at 900
    pub swap_history: VecDeque<(Instant, u64)>,    // capped at 900
}
```

#### `AppState` Initialization

- `process_history` starts empty (`HashMap::new()`).
- `selected_process_detail` starts `None`.
- `process_detail_confirm_kill` starts `false`.

#### `ProcessHistory` Lifecycle

1. **Population:** Every `UpdateSnapshot`, `snapshot.rs` iterates `snapshot.processes` and calls `push_process_history(pid, rss, swap)` for each row.
2. **Capping:** Each deque is capped at 900 entries (15 minutes at 1 collect/second). Excess entries are popped from the front.
3. **Retention:** When a PID disappears from a snapshot, its `ProcessHistory` is **retained** in the `HashMap`. If the PID reappears later, the chart continues seamlessly.
4. **Memory estimate:** ~200 steady-state processes × 900 pts × 2 histories × 16 bytes ≈ 18 MB. With transient processes (e.g., 1000 unique PIDs over a session), overhead grows to ~28 MB. No explicit pruning is implemented for this first version; the HashMap grows unbounded but remains acceptable for a desktop TUI tool. A future enhancement could add LRU eviction or TTL pruning.
5. **Initialization:** `process_history` starts as `HashMap::new()` in `AppState::new`.

---

### Data Flow

```
User presses Enter on selected process (Processes tab)
  → input.rs: resolve_key → Action::OpenProcessDetail { pid }
  → app/processes.rs: handle_open_process_detail()
      → selected_process_detail = Some(pid)
      → process_detail_confirm_kill = false

Tick (1s) — already existing
  → bridge sends UpdateSnapshot
  → app/snapshot.rs: apply_snapshot()
      → push_history (global ram/swap)
      → push_process_history for each ProcessRow
      → on_processes_updated(), on_devices_updated()

Render (30fps)
  → ui/mod.rs: if selected_process_detail.is_some()
      → call ui/process_detail::render(f, area, state)
      → reads process_history, current snapshot, selected_process_detail

User presses k inside modal
  → input.rs: resolve_key → Action::ConfirmKillProcess { pid }
  → app/processes.rs: handle_confirm_kill_process()
      → process_detail_confirm_kill = true

User presses y on kill confirmation
  → input.rs: resolve_key → Action::KillProcess { pid }
  → main.rs: intercepts KillProcess, sends PlatformCommand::KillProcess { pid } to bridge
  → platform_bridge.rs: calls platform.kill_process(pid)
      → Linux: nix::sys::signal::kill(pid, SIGTERM)
  → bridge sends Action::KillProcessResult { pid, success } back via action_tx
  → reducer: on success → close modal (selected_process_detail = None), SetError("Sent SIGTERM to PID {pid}")
    on failure → SetError("Failed to kill PID {pid}: {msg}"), keep modal open

User presses Esc / q inside modal (no confirmation active)
  → input.rs: resolve_key → Action::CloseProcessDetail
  → app/processes.rs: handle_close_process_detail()
      → if process_detail_confirm_kill → process_detail_confirm_kill = false (keep modal open)
      → else → selected_process_detail = None

User presses n / Esc when kill confirmation is active
  → Same as above: reducer sees process_detail_confirm_kill == true, clears it, keeps modal open
```

---

### Actions

```rust
// New variants in Action enum (actions.rs)
OpenProcessDetail { pid: u32 },
CloseProcessDetail,
ConfirmKillProcess { pid: u32 },
KillProcess { pid: u32 },          // INTERCEPTED by main.rs — sent to PlatformBridge, never reaches reducer
KillProcessResult { pid: u32, success: bool, msg: Option<String> }, // returned by bridge, handled by reducer
```

---

### Input Mapping (`input.rs`)

**New context field:**

```rust
pub struct KeyContext {
    // ... existing fields ...
    pub process_detail: Option<ProcessDetailContext>,
}

pub struct ProcessDetailContext {
    pub pid: u32,
    pub show_kill_confirm: bool,
}
```

**Resolution logic:**

- If `process_detail` is `Some(ctx)`:
  - `Esc` / `q`:
    - If `ctx.show_kill_confirm` → `CloseProcessDetail` (reducer clears confirmation flag, keeps modal open)
    - Else → `CloseProcessDetail` (reducer closes modal)
  - `k` (and `ctx.show_kill_confirm == false`) → `ConfirmKillProcess { pid: ctx.pid }`
  - `y` (and `ctx.show_kill_confirm`) → `KillProcess { pid: ctx.pid }`
  - `n` (and `ctx.show_kill_confirm`) → `CloseProcessDetail` (reducer clears confirmation flag, keeps modal open)
  - All other keys → `None`

- On `Tab::Processes` (and no detail open, and `filter_mode == false`):
  - `Enter` → `OpenProcessDetail { pid }` where `pid` = PID of the currently selected filtered process

---

### Platform Changes

#### `platform/mod.rs` — `PlatformProvider` trait

```rust
fn kill_process(&self, pid: u32) -> Result<(), String>;
```

#### `platform/linux/mod.rs` — `LinuxBackend`

```rust
fn kill_process(&self, pid: u32) -> Result<(), String> {
    use nix::sys::signal::{kill, Signal};
    kill(nix::unistd::Pid::from_raw(pid as i32), Signal::SIGTERM)
        .map_err(|e| format!("{e}"))
}
```

#### `platform/linux/proc_reader.rs`

Extend process parsing to read `num_threads` (field 20) and `state` (field 3, char) from `/proc/PID/stat`.

#### `platform_bridge.rs`

Add new `PlatformCommand` variant:

```rust
KillProcess { pid: u32 },
```

And handle it in the bridge thread:

```rust
PlatformCommand::KillProcess { pid } => {
    let result = backend.kill_process(pid);
    let action = match result {
        Ok(()) => Action::KillProcessResult { pid, success: true, msg: None },
        Err(msg) => Action::KillProcessResult { pid, success: false, msg: Some(msg) },
    };
    let _ = action_tx.send(action);
}
```

---

### UI Layout (`ui/process_detail.rs`)

**Modal overlay:** centered, 70% width, 70% height, minimum 24×12. Uses `ratatui::widgets::Clear` to blank the background.

**Content layout (top to bottom):**

```
┌─ firefox (PID 12345) ──────────────────────────────────┐  ← title bar with name + PID
│ Usuário: ricardo | Threads: 48 | Status: R (running)     │  ← metadata line 1
│ Exec: /usr/lib/firefox/firefox                            │  ← metadata line 2
├──────────────────┬────────────────────────────────────────┤  ← separator
│ RAM (histórico)  │ Swap (histórico)                      │  ← chart headers
│                  │                                       │
│  [sparkline]     │  [sparkline]                          │  ← charts (min height 5)
│                  │                                       │
├──────────────────┴────────────────────────────────────────┤
│ Atual: RSS 512.3 MB | Swap 128.0 MB                       │  ← current values
│ [k] matar  [Esc/q] voltar                                 │  ← footer hints
└───────────────────────────────────────────────────────────┘
```

**Kill confirmation state:**

```
│ Matar PID 12345? [y]es / [n]o                              │  ← replaces footer, red fg
```

**Chart rendering:**
- Reuse `ratatui::widgets::Sparkline` (same widget used in Overview).
- Convert `VecDeque<(Instant, u64)>` → `Vec<(f64, f64)>` using `seconds_since_start` (same pattern as Overview charts).
- Max value for sparkline = max in the history deque (or 1 if empty, to avoid division by zero).
- Color: RAM chart = Blue, Swap chart = Magenta (or reuse `design.rs` theme colors).

**Empty / short history:**
- If history has < 10 points: display text `"Collecting history... (N/900)"` instead of sparkline.

**Process ended while modal open:**
- If `current` snapshot does not contain the PID, show metadata as `(process ended)` and freeze charts at last known values.
- Allow closing modal with Esc/q.

**Small terminal handling:**
- If modal area < minimum height (12 rows), render a compact version: title + metadata + current values + footer (no charts).
- If < minimum width (24 cols), modal clamps to terminal size.

---

### Error Handling

| Scenario | Behavior |
|----------|----------|
| Process dies while modal open | Modal stays open, charts freeze, metadata shows "(process ended)". Close with Esc. |
| PID reappears after leaving | History continues seamlessly (not reset). |
| User is not root when killing | `KillProcessResult` returns failure → reducer calls `SetError("Permission denied — run with sudo")`. Modal stays open. |
| `kill()` syscall fails (EPERM, ESRCH) | Same as above — error message propagated from `nix` errno. |
| History not yet full | Sparkline renders whatever data exists. Footer shows collection progress. |
| Terminal too small | Modal renders compact version (metadata + text only, no charts). |

---

### Testing Strategy

#### Unit Tests — `app/processes.rs`

- `open_process_detail_sets_pid_and_clears_confirm_kill`
- `close_process_detail_clears_pid_and_confirm_kill`
- `confirm_kill_process_sets_flag`
- `kill_process_result_success_closes_modal`
- `kill_process_result_failure_keeps_modal_and_sets_error`

#### Unit Tests — `app/snapshot.rs`

- `snapshot_appends_process_history_for_each_row`
- `process_history_capped_at_900_entries`
- `process_history_retained_when_pid_leaves_snapshot`
- `process_history_resumed_when_pid_returns`

#### Unit Tests — `input.rs`

- `enter_opens_detail_on_processes_tab`
- `enter_does_nothing_on_overview_tab`
- `esc_closes_detail_when_no_confirm`
- `esc_cancels_confirm_when_active`
- `k_triggers_confirm_kill`
- `y_confirms_kill_when_confirming`
- `n_cancels_kill_confirm`
- `q_closes_detail`
- `other_keys_ignored_in_detail_mode`

#### Unit Tests — `ui/process_detail.rs`

- `layout_splits_into_title_meta_charts_current_footer`
- `compact_mode_when_height_below_minimum`
- `kill_confirm_replaces_footer`

#### Integration-style (no I/O needed)

- `process_row_parses_threads_and_status` (in `platform/linux/proc_reader.rs` tests, if testable; otherwise manual verification)

---

## Files to Modify / Create

| File | Change |
|------|--------|
| `src/platform/types.rs` | Add `threads: u32`, `status: char` to `ProcessRow` |
| `src/platform/linux/proc_reader.rs` | Parse threads + status from `/proc/PID/stat` |
| `src/platform/mod.rs` | Add `kill_process` to `PlatformProvider` trait |
| `src/platform/linux/mod.rs` | Implement `kill_process` using `nix::sys::signal::kill` |
| `src/platform/bsd.rs` | Stub `kill_process` (return `Err("Not supported")`) |
| `src/platform/macos.rs` | Stub `kill_process` |
| `src/platform/windows.rs` | Stub `kill_process` |
| `src/platform_bridge.rs` | Add `KillProcess` to `PlatformCommand`, handle in thread |
| `src/actions.rs` | Add `OpenProcessDetail`, `CloseProcessDetail`, `ConfirmKillProcess`, `KillProcess`, `KillProcessResult` |
| `src/app/mod.rs` | Add `process_history`, `selected_process_detail`, `process_detail_confirm_kill` to `AppState`; update `make_process` test helper for new `ProcessRow` fields |
| `src/app/snapshot.rs` | Add `push_process_history` loop in `apply_snapshot` |
| `src/app/processes.rs` | Add `handle_open_process_detail`, `handle_close_process_detail`, `handle_confirm_kill`, `handle_kill_result` |
| `src/input.rs` | Add `ProcessDetailContext` to `KeyContext`, resolve detail-mode keys |
| `src/main.rs` | Intercept `KillProcess` action, send `PlatformCommand::KillProcess` to bridge |
| `src/ui/mod.rs` | Conditionally render `process_detail` overlay before tab content |
| `src/ui/process_detail.rs` | **NEW** — render modal, charts, metadata, kill confirmation |

---

## Open Questions (none)

All decisions resolved during brainstorming:
- History length: 900 points (15 minutes) — approved.
- History collection: continuous from app start — approved (approach A).
- Kill signal: `SIGTERM` (gentle termination), not `SIGKILL`.
- History retention: retained when PID leaves, resumed when PID returns.

---

## Design Principles Followed

1. **Pure reducer:** `app/` only mutates state. No I/O.
2. **Bridge owns platform:** `main.rs` never calls platform directly.
3. **UI is read-only:** `ui/process_detail.rs` reads `&AppState`, never mutates.
4. **Minimal intrusions:** Reuses existing `Sparkline` widget, `VecDeque` history pattern, and `PlatformCommand`/`Action` message passing.
5. **Testability:** Every state transition and input mapping has a unit test target.
