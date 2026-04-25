# Bridge Responsiveness — Design Spec

**Date:** 2026-04-24
**Goal:** Add UI responsiveness signals to PlatformBridge so users see immediate feedback during background operations (collect, device ops, create swap).

**Approach:** Bridge emits lightweight start signals before blocking work. AppState tracks timestamps. UI derives indicators from state.

---

## New Action

```rust
Action::CollectStarted
```

Single new action. Bridge emits before `handle_collect`. No `CollectFinished` — `UpdateSnapshot` and `SetError` already cover both exit paths.

No `DeviceOpStarted` — reducer already sets `OpStatus::Running` via `ExecuteDeviceOp` immediately. Gap between reducer set and bridge start is microseconds (channel send).

---

## AppState Changes

### New fields

```rust
pub struct AppState {
    // ... existing fields ...
    pub collect_in_progress: bool,
    pub last_collect_completed: Instant,
}
```

- `collect_in_progress`: `true` between `CollectStarted` and `UpdateSnapshot`/`SetError`
- `last_collect_completed`: timestamp of last successful snapshot, used for stale detection

### DeviceOp — new field

```rust
pub struct DeviceOp {
    pub path: PathBuf,
    pub kind: DeviceOpKind,
    pub status: OpStatus,
    pub started_at: Instant,  // NEW
}
```

UI calculates elapsed: `Instant::now() - started_at` while `status == Running`.

### Reducer handlers

| Action | Reducer behavior |
|--------|-----------------|
| `CollectStarted` | `self.collect_in_progress = true` |
| `UpdateSnapshot` | `self.collect_in_progress = false; self.last_collect_completed = snap.timestamp` |
| `SetError` | `self.collect_in_progress = false` |
| `ExecuteDeviceOp` | Set `started_at: Instant::now()` on the DeviceOp |

### Initialization

```rust
impl AppState {
    pub fn new(capabilities: Capabilities) -> Self {
        Self {
            // ... existing ...
            collect_in_progress: false,
            last_collect_completed: Instant::now(),
        }
    }
}
```

---

## PlatformBridge Changes

One line added in the bridge thread loop:

```rust
PlatformCommand::Collect => {
    let _ = action_tx.send(Action::CollectStarted);  // NEW
    Self::handle_collect(&mut *backend, &action_tx, &processes_active);
}
```

No other bridge changes. DeviceOp and CreateSwap paths unchanged.

---

## UI Indicators

### Statusbar (`src/ui/statusbar.rs`)

- `collect_in_progress == true` → show `"⟳"` indicator
- `device_op.status == Running` → show `"swapon /path (3s)"` with elapsed from `started_at`
- Stale detection: `now - last_collect_completed > 3s` → show `"⚠ stale"` or change gauge colors. Threshold 3s = 3× tick interval, avoids false alarms on normal tick.

### Devices tab (`src/ui/devices.rs`)

- When `device_op.status == Running`, show elapsed time on the device row under operation.

### No changes needed

- `overview.rs` — stale indicator centralized in statusbar
- `processes.rs` — no responsiveness concern
- `create_swap.rs` — already has step-by-step progress UI

---

## Testing

### Reducer tests (`src/app.rs`)

- `collect_started_sets_flag` — `CollectStarted` → `collect_in_progress == true`
- `update_snapshot_clears_collect_in_progress` — `CollectStarted` then `UpdateSnapshot` → `collect_in_progress == false`
- `set_error_clears_collect_in_progress` — `CollectStarted` then `SetError` → `collect_in_progress == false`
- `execute_device_op_sets_started_at` — `ExecuteDeviceOp` → `device_op.started_at` is recent
- `last_collect_completed_updates_on_snapshot` — `UpdateSnapshot` → `last_collect_completed` advances

### Bridge tests (`src/platform_bridge.rs`)

- `collect_emits_started_before_snapshot` — send `Collect`, receive `CollectStarted` first, `UpdateSnapshot` second

### UI

No new unit tests — visual indicators verified manually in TUI.

---

## Out of scope

- Multi-lane bridge (separate threads for collect vs device ops) — future if needed
- `DeviceOpStarted` from bridge — reducer `Running` is sufficient
- `CollectFinished` explicit action — `UpdateSnapshot`/`SetError` cover both paths
- Queued vs Running distinction — gap is microseconds
- Persistence of metrics — all in-memory like rest of app

---

## File map

| Action | File | Change |
|--------|------|--------|
| Modify | `src/actions.rs` | Add `CollectStarted` variant |
| Modify | `src/app.rs` | New fields, reducer handlers, tests |
| Modify | `src/platform_bridge.rs` | Emit `CollectStarted` before collect |
| Modify | `src/ui/statusbar.rs` | Collect spinner, stale indicator, device op elapsed |
| Modify | `src/ui/devices.rs` | Elapsed time on running device op |
