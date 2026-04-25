# Spec 1: Pure Input + PlatformBridge

**Date:** 2026-04-24
**Scope:** P1 (impure resolve_key) + P2 (LinuxBackend leak in main.rs) + P3 (collector blocks tokio)
**Approach:** Bottom-up — purify input first, then extract PlatformBridge

---

## Problems Addressed

### P1: `Arc<Mutex<AppState>>` leaks into `input.rs`

`resolve_key` should be a pure function: `(KeyEvent, KeyContext) → Option<Action>`. Currently it takes `&Arc<Mutex<AppState>>` in `KeyContext` and performs multiple locks internally (6+ lock sites across `resolve_key`, `handle_devices_key`, `handle_create_swap_key`, `validate_and_submit`). Two call sites mutate state directly inside the resolver, breaking the Action/Reducer pattern:

- `input.rs:272-276` — clears completions by mutating `AppState` directly
- `input.rs:416-420` — sets `validation_error` by mutating `AppState` directly

### P2: `main.rs` imports `LinuxBackend` directly

`main.rs:27-28` imports `platform::linux::LinuxBackend` and `platform::linux::create_swap::run_create_swap_steps`. Device ops inside `spawn_blocking` instantiate `LinuxBackend::new()` directly, bypassing the factory and platform abstraction. This code won't compile on non-Linux targets.

### P3: `Collector::collect` blocks the tokio executor

`Collector::collect` is marked `async` but performs zero async work. All backend calls are synchronous, including `process_list()` which iterates every PID in `/proc`. Called from `tick.tick()` in `tokio::select!`, this blocks rendering and input handling.

---

## Phase 1: Pure `resolve_key`

### New `KeyContext` with sub-structs

Remove `Arc<Mutex<AppState>>` from `KeyContext`. Replace with value-only fields extracted via a single lock in `main.rs` before calling `resolve_key`.

```rust
pub struct KeyContext {
    pub active_tab: Tab,
    pub filter_mode: bool,
    pub sort_col: SortColumn,
    pub is_root: bool,
    pub device: DeviceContext,
    pub create_swap: Option<CreateSwapContext>,
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

pub struct ConfirmOffDeleteContext {
    pub path: PathBuf,
    pub delete_file: bool,
    pub active: bool,
}

pub struct CreateSwapContext {
    pub mode: CreateSwapModeKind,
    pub focused_field: Option<CreateSwapField>,
    pub path_value: String,
    pub size_value: String,
    pub priority_value: String,
    pub size_unit: SizeUnit,
    pub completions_showing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CreateSwapModeKind {
    Form,
    Progress,
    ConfirmActivateOnly,
}
```

### `is_root` at startup

Compute `nix::unistd::geteuid().is_root()` once at startup. Store as `pub is_root: bool` in `AppState`. Pass into `KeyContext` from there.

### Extraction point in `main.rs`

In the `events` arm of `tokio::select!`, replace the current multi-field extraction with a single lock that builds the full `KeyContext`:

```rust
let ctx = {
    let s = state.lock().expect("state mutex poisoned");
    KeyContext::from_state(&s)
};
let action = input::resolve_key(key, &ctx);
```

Add a `KeyContext::from_state(state: &AppState) -> KeyContext` constructor that extracts all needed values.

### New actions

None. The `SetValidationError` action is not needed — validation moves entirely into the reducer (see below).

### Removed: direct mutation in `validate_and_submit`

`validate_and_submit` is deleted entirely. Input resolver dispatches `CreateSwapSubmit` unconditionally on Enter. The reducer validates fields inline (it already has access to `CreateSwapModal` inputs) and either sets `validation_error` + stays in Form mode, or transitions to Progress mode.

### Removed: direct mutation for completion clearing

Currently `input.rs:272-276` clears completions by locking and mutating state. After refactor, the reducer auto-clears completions when processing any form input action (`CreateSwapInputEvent`, `CreateSwapFocusField`, `CreateSwapToggleUnit`, `CreateSwapToggleActivate`). Input resolver just returns the action; reducer handles the side-effect.

### Validation moves to reducer

`CreateSwapSubmit` action is always dispatched on Enter. `handle_action` for `CreateSwapSubmit` validates fields by reading `path_input.value()`, `size_input.value()`, `priority_input.value()` from the modal. On validation failure, sets `validation_error` and stays in Form mode. On success, transitions to Progress mode.

The `validate_and_submit` function in `input.rs` is deleted.

### `compute_path_completions` stays in `input.rs`

`compute_path_completions` does sync I/O (filesystem read_dir). It remains in `input.rs` for now — moving it off the tokio thread is deferred to P8. After the pure-input refactor, `resolve_key` still calls it directly (it doesn't access AppState, only the filesystem). The function is pure in terms of state — its only impurity is I/O, which is a separate concern.

### Test impact

All `input.rs` tests use the `rk()` helper. Update `rk()` to construct value-based `KeyContext` instead of `Arc<Mutex<AppState>>`. Tests become simpler — no mutex, no state setup, just struct literals. Clean break, no backward compat adapters.

New tests for validation logic move to `app.rs` (reducer tests).

---

## Phase 2: PlatformBridge

### Architecture

```
main.rs ──PlatformCommand──► PlatformBridge (dedicated std::thread, owns backend)
         ◄──────Action──────  (tokio::sync::mpsc, same channel as current action_rx)
```

### Command enum

```rust
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
```

### PlatformBridge struct

```rust
pub struct PlatformBridge {
    cmd_tx: std::sync::mpsc::Sender<PlatformCommand>,
}

impl PlatformBridge {
    pub fn spawn(
        action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
        processes_active: Arc<AtomicBool>,
    ) -> Self {
        let (cmd_tx, cmd_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut backend = platform::factory::detect();
            // Event loop: recv PlatformCommand, execute on backend, send Action back
            loop {
                match cmd_rx.recv() {
                    Ok(PlatformCommand::Collect) => { /* collect and send UpdateSnapshot */ }
                    Ok(PlatformCommand::DeviceOp { path, kind }) => { /* execute and send DeviceOpUpdate */ }
                    Ok(PlatformCommand::CreateSwap { .. }) => { /* run steps, send StepUpdate actions */ }
                    Ok(PlatformCommand::Shutdown) => break,
                    Err(_) => break, // channel closed
                }
            }
        });
        Self { cmd_tx }
    }

    pub fn send(&self, cmd: PlatformCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}
```

### Dedicated thread loop details

- Owns `Box<dyn SwapBackend>` exclusively — no sharing, no Arc
- `Collect`: calls `system_ram`, `system_swap`, `swap_devices`, conditionally `process_list` (checks `processes_active` AtomicBool). Assembles `MemSnapshot`, sends `Action::UpdateSnapshot`.
- `DeviceOp`: calls `swap_on`/`swap_off`/`swap_reset` + file delete logic (currently in `main.rs:140-164`). Sends `Action::DeviceOpUpdate`.
- `CreateSwap`: runs `run_create_swap_steps` (moved from `platform::linux::create_swap` to platform-agnostic location or called via backend). Sends `Action::CreateSwapStepUpdate` per step.
- `Shutdown`: breaks loop, thread exits cleanly.

### Channel choice

- **Commands (main → bridge):** `std::sync::mpsc` — bridge thread is a std::thread, not a tokio task. `std::sync::mpsc::Receiver::recv()` blocks the thread naturally between commands.
- **Actions (bridge → main):** `tokio::sync::mpsc::UnboundedSender` — main loop already uses `action_rx` in `tokio::select!`. Reuse same channel.

### Changes to `main.rs`

- Remove `use platform::linux::LinuxBackend` and `use platform::linux::create_swap::run_create_swap_steps`
- Remove `Collector` import and instantiation
- Add `mod platform_bridge` and create bridge at startup: `let bridge = PlatformBridge::spawn(action_tx.clone(), processes_active.clone())`
- Do initial collection via bridge: `bridge.send(PlatformCommand::Collect)` — but initial snapshot needs to be synchronous (first frame must not be blank). Two options: (a) bridge returns initial snapshot via oneshot channel, or (b) call `factory::detect()` + collect directly in main before spawning bridge. Option (b) is simpler — keep the existing initial collection, then hand backend ownership to bridge.
- Tick arm: `bridge.send(PlatformCommand::Collect)` — non-blocking, result arrives via `action_rx`
- Device op arm: `bridge.send(PlatformCommand::DeviceOp { .. })` — replaces `spawn_blocking`
- Create swap arm: `bridge.send(PlatformCommand::CreateSwap { .. })` — replaces `spawn_blocking`
- Shutdown: `bridge.send(PlatformCommand::Shutdown)` before breaking

### Initial collection strategy

Keep initial collection in `main.rs` before spawning PlatformBridge:

```rust
let mut backend = platform::factory::detect();
let caps = backend.capabilities();
// ... initial collect ...
let bridge = PlatformBridge::spawn_with_backend(backend, action_tx, processes_active);
```

`spawn_with_backend` takes ownership of the already-created backend instead of calling `factory::detect()` internally. Avoids double-initialization.

### What gets deleted

- `src/collector.rs` — fully absorbed into PlatformBridge
- `LinuxBackend::new()` call in `main.rs:139` — gone
- Both `spawn_blocking` blocks in `main.rs` — replaced by `bridge.send()`
- `use platform::linux::*` imports in `main.rs` — gone

### File location

`src/platform_bridge.rs` — sits between `main.rs` and `platform/`.

### P3 resolution

All backend I/O runs on the dedicated thread. Tokio executor is never blocked. The fake `async fn collect` is eliminated. `tick.tick()` arm sends a non-blocking `PlatformCommand::Collect` and results arrive asynchronously via `action_rx`.

### Testing

- Mock backend (same as current `collector.rs` tests) injected into PlatformBridge
- Verify correct `Action`s sent back for each `PlatformCommand`
- Test error paths: backend returns `Err`, verify `Action::SetError` sent
- Test shutdown: verify thread exits cleanly

---

## Ordering and Shippability

Each phase is independently shippable:

1. **Phase 1 (pure input):** Land, verify all tests pass. Zero runtime behavior change. Pure refactor.
2. **Phase 2 (PlatformBridge):** Land, verify collection + device ops + create swap still work. Runtime behavior changes (async collection), but user-visible behavior identical.

---

## Out of Scope (Deferred to Spec 2)

- P4: AppState god object decomposition
- P5: Action enum grouping into sub-enums
- P6: Unnecessary cloning in UpdateSnapshot
- P7: Linux module cfg gate
- P8: compute_path_completions sync I/O
- P9: Background task cancellation
- P10: Mutex poisoning handling
