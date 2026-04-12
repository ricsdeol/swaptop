# Phase 2 — Processes Table Design

**Date:** 2026-04-11
**Branch:** feature/processes-screen
**Scope:** Phase 2 only — table, sorting, filtering. Phase 3 (detail screen, kill, history) is separate.

---

## Overview

Add the Processes tab to swaptop: a sortable, filterable table of running processes showing
PID, name, user, RSS, swap usage, and CPU%. Swap per process is read from `/proc/PID/smaps`
in parallel tokio tasks, only while the Processes tab is active.

---

## 1. Process Collection

### Flag: `Arc<AtomicBool> processes_active`

Created in `main.rs`, cloned into `Collector`. Updated after every tab-change action:

```
active_tab == Tab::Processes  →  processes_active.store(true)
any other tab                 →  processes_active.store(false)
```

### Collector tick when `processes_active = true`

1. `backend.process_list()` — new trait method — returns `Vec<ProcessRow>` via sysinfo
   (RSS, VMS, CPU%, name, user, pid; `swap = 0`)
2. For each process: `tokio::spawn` reads `/proc/{pid}/smaps`, sums `VmSwap:` fields
3. `futures::future::join_all` collects all results
4. Snapshot sent with real per-process swap values

### Collector tick when `processes_active = false`

`processes: vec![]` — no smaps I/O (same as today).

### Thread / kernel thread filtering

- sysinfo reads `/proc/{pid}` (thread group leaders only) — POSIX threads do not appear
- Kernel threads filtered by: `name.starts_with('[') && name.ends_with(']')`
  (e.g. `[kworker/0:0]`, `[migration/0]`, `[kswapd0]`)

### `SwapBackend` trait change

New method added to the trait:

```rust
fn process_list(&mut self) -> Result<Vec<ProcessRow>>;
```

- `LinuxBackend`: uses `sysinfo::System::processes()`, filters kernel threads, maps to `ProcessRow` with `swap = 0`
- `MacosBackend`, `WindowsBackend`, `BsdBackend`: return `Ok(vec![])` (stubs)

---

## 2. AppState

### New fields

```rust
pub processes:    Vec<ProcessRow>,  // full list, sorted; filter applied at render time
pub sort_col:     SortColumn,       // default: SortColumn::Swap
pub sort_dir:     SortDir,          // default: SortDir::Desc
pub selected_row: usize,            // index into the filtered list (computed at render)
pub filter_text:  String,           // empty = no filter
pub filter_mode:  bool,             // true = filter input is active
```

### New types (in `app.rs`)

```rust
pub enum SortColumn { Pid, Name, User, Rss, Swap, Cpu }
pub enum SortDir    { Asc, Desc }
```

### New Actions (in `actions.rs`)

```rust
Action::NavigateUp,
Action::NavigateDown,
Action::SortBy(SortColumn),
Action::EnterFilterMode,
Action::FilterChar(char),
Action::FilterBackspace,
Action::ExitFilterMode,
```

### Reducer logic

| Action | Effect |
|---|---|
| `SortBy(col)` | Same col → toggle `sort_dir`; different col → `sort_col = col, sort_dir = Desc` |
| `NavigateUp` | `selected_row = selected_row.saturating_sub(1)` |
| `NavigateDown` | `selected_row = min(selected_row + 1, filtered_len.saturating_sub(1))` |
| `UpdateSnapshot` | Store `processes`, re-apply sort, clamp `selected_row` against filtered len |
| `EnterFilterMode` | `filter_mode = true` |
| `FilterChar(c)` | `filter_text.push(c)`, reset `selected_row = 0` |
| `FilterBackspace` | `filter_text.pop()`, reset `selected_row = 0` |
| `ExitFilterMode` | `filter_mode = false` (filter text persists) |

Sort helper (pure, no I/O, called in reducer):
- Sorts `processes` in-place by `sort_col` / `sort_dir`
- Called inside `UpdateSnapshot` and `SortBy`

Filter (applied at render time and for navigation bounds):
- `process.name.to_lowercase().contains(&filter_text.to_lowercase())`
- Navigation actions compute `filtered_len` inline before clamping `selected_row`

Sort cycle for `s` key: `Swap → Cpu → Rss → Pid → Name → Swap`

---

## 3. Keyboard Handling (`main.rs`)

Three mutually exclusive contexts evaluated in order:

```
1. filter_mode == true
   printable char  → FilterChar(c)
   Backspace       → FilterBackspace
   Esc / Enter     → ExitFilterMode

2. active_tab == Processes && !filter_mode
   j / ↓           → NavigateDown
   k / ↑           → NavigateUp
   s               → SortBy(next col: Swap→Cpu→Rss→Pid→Name→Swap)
   /               → EnterFilterMode
   Enter           → (no-op in Phase 2; Phase 3 will open detail)

3. global (any tab, outside filter_mode)
   q / Q / Ctrl-C  → Quit
   r               → Refresh
   Tab / BackTab   → NextTab / PrevTab
   1–4             → SelectTab(n)
```

After any tab-change action, `processes_active` is updated immediately.

---

## 4. UI — `ui/processes.rs`

### Layout

```
┌─────────────────────────────────────────────────────────────────┐
│ Filter: [firefox_____________]    (only visible in filter_mode) │
│ ─────────────────────────────────────────────────────────────── │
│  PID   Nome            Usuário    RSS       Swap ▾     CPU%     │  ← header
│ ─────────────────────────────────────────────────────────────── │
│ 12345  firefox         ricardo    512.3 MB  128.0 MB  12.5%     │  ← selected (highlight)
│  3821  code            ricardo    256.1 MB   64.0 MB   4.2%     │
│    42  kswapd0         root         0.0  B    0.0  B   0.1%     │
└─────────────────────────────────────────────────────────────────┘
```

### Widget: `ratatui::widgets::Table` + `TableState`

- `TableState` drives scroll and selection natively
- Header row: sorted column gets `▾` (Desc) or `▲` (Asc) suffix
- Selected row: `Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)`
- Column widths (approximate, using `Constraint::Min` / `Constraint::Length`):

| Column | Width | Align |
|---|---|---|
| PID | 6 | Right |
| Nome | 20 | Left |
| Usuário | 10 | Left |
| RSS | 10 | Right |
| Swap | 10 | Right |
| CPU% | 6 | Right |

### Value formatting

- Bytes (RSS, Swap): `human_bytes` crate → `"512.3 MB"`, `"0.0 B"`
- CPU%: `format!("{:.1}%", cpu_pct)`, right-aligned
- PID: `format!("{:>5}", pid)`

### Platform limitation (macOS)

When `capabilities.has_per_process == false`:
- Swap column cells render `"—"` in `Color::DarkGray`
- Banner above table: `"Swap usage per process is not available on this platform"` in yellow

### Empty state

When filtered list is empty: `Paragraph::new("No processes found")` centered in content area.

### Filter bar (when `filter_mode == true`)

Single-line `Paragraph` with border, above the table:
```
 Filter: firefox_
```
Cursor simulated with trailing `_`. Height: 3 rows (with border).

---

## 5. User Documentation

A separate file `docs/processes-screen.md` to be created alongside implementation,
covering: navigation, sorting, filtering, keybindings, and platform limitations.
All user-facing text (banners, empty states, labels, error messages) must be in English.

---

## Files Changed

| File | Change |
|---|---|
| `src/platform/mod.rs` | Add `process_list()` to `SwapBackend` trait |
| `src/platform/linux.rs` | Implement `process_list()` |
| `src/platform/macos.rs` | Stub `process_list()` → `Ok(vec![])` |
| `src/platform/windows.rs` | Stub `process_list()` → `Ok(vec![])` |
| `src/platform/bsd.rs` | Stub `process_list()` → `Ok(vec![])` |
| `src/actions.rs` | Add 7 new variants |
| `src/app.rs` | Add `SortColumn`, `SortDir`, 6 new fields, reducer cases |
| `src/collector.rs` | Add `processes_active` flag, parallel smaps collection |
| `src/main.rs` | Context-aware keyboard routing, update `processes_active` |
| `src/ui/mod.rs` | Route `Tab::Processes` to `processes::render` |
| `src/ui/processes.rs` | New file — full table render |
| `docs/processes-screen.md` | New file — user documentation |
