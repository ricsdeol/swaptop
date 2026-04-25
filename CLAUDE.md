# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
rtk cargo build                   # build debug
rtk cargo build --release         # build release
rtk cargo run                     # run (requires Linux for full functionality)
rtk cargo test                    # run all tests
rtk cargo test <test_name>        # run a single test
rtk cargo clippy -- -D warnings   # lint
```

## Project: swaptop

TUI swap/memory manager for Linux (primary target), inspired by `htop`/`btop`. Built with Ratatui + crossterm + tokio.

## Architecture

### Data flow

```
User keypress
  → input::resolve_key(KeyEvent, &KeyContext) → Option<Action>
  → main.rs dispatches Action to AppState::handle_action() [reducer, pure, no I/O]
  → for I/O actions (DeviceOp, CreateSwap): main.rs sends PlatformCommand to bridge
  → PlatformBridge (dedicated std::thread) calls PlatformProvider methods
  → bridge sends result Actions back via tokio::sync::mpsc → reducer updates state
  → ui/ reads &AppState on each frame (30fps) — render only, no mutations
```

### Layer rules

| Layer | May call | Must NOT call |
|-------|----------|---------------|
| `ui/` | read `&AppState` | mutate state, call platform, send commands |
| `input.rs` | return `Option<Action>` | lock mutex, do I/O, call platform |
| `app.rs` (reducer) | mutate `AppState` | do I/O, call platform, send commands |
| `main.rs` | lock state, send `PlatformCommand`, dispatch `Action` | call `PlatformProvider` directly |
| `platform_bridge.rs` | call `PlatformProvider`, send `Action` | read `AppState`, touch UI |
| `platform/` | system calls, `/proc`, `nix` | know about `Action`, `AppState`, UI |

### Modules

- **`main.rs`** — `tokio::select!` with 5 arms: shutdown, action_rx, tick (1s → `PlatformCommand::Collect`), frame_tick (30fps → draw), events (key → resolve → action). `AppState` as `Arc<Mutex<AppState>>`.
- **`platform_bridge.rs`** — owns `Box<dyn PlatformProvider>` on a `std::thread`. Receives `PlatformCommand` (Collect, DeviceOp, CreateSwap, Shutdown) via `std::sync::mpsc`. Sends `Action`s back via `tokio::sync::mpsc`. Emits `CollectStarted`/`CollectFinished` around collect.
- **`platform/mod.rs`** — `PlatformProvider` trait. Methods: `system_ram`, `system_swap`, `swap_devices`, `process_list`, `swap_on`, `swap_off`, `swap_reset`, `create_swap_file`. Returns data types only — no `Action`, no `AppState`.
- **`platform/types.rs`** — `SwapInfo`, `SwapDevice`, `ProcessRow`, `MemSnapshot`, `StepStatus`, `CreateSwapProgress`, `parse_swap_header`
- **`platform/factory.rs`** — `detect() -> Box<dyn PlatformProvider>` via `#[cfg(target_os)]`
- **`platform/linux/`** — `LinuxBackend` impl, `proc_reader.rs`, `create_swap.rs` (step runner)
- **`app.rs`** — `AppState` struct + `handle_action()` reducer. Pure — no I/O. All mutations via `Action` enum.
- **`actions.rs`** — `Action` enum (all state mutations) + `DeviceOpKind`, `OpStatus`, `SortColumn`
- **`input.rs`** — `resolve_key(KeyEvent, &KeyContext) -> Option<Action>`. Pure function, zero mutex locks.
- **`create_swap.rs`** — wizard state types: `CreateSwapModal`, `CreateSwapField`, `SizeUnit`, `CreateSwapStep`. UI state only — no I/O.
- **`ui/`** — `overview.rs`, `processes.rs`, `devices.rs`, `create_swap.rs`, `statusbar.rs`, `design.rs`. Read-only access to `&AppState`.

## After implementing all phases

Run these and fix any issues after implement all phases and tasks:

```bash
rtk cargo build          # must compile clean (zero warnings)
rtk cargo clippy -- -D warnings  # must pass with no warnings
rtk cargo fmt --check     # must be properly formatted
rtk cargo test           # all tests must pass
```

## Key constraints

- `platform_bridge.rs` owns the `PlatformProvider` — `main.rs` never imports platform backends directly
- smaps parsing is expensive — runs on the bridge thread, never blocks render
- `swapon`/`swapoff` require root — check `nix::unistd::geteuid() == 0` before calling
- History is in-memory only (no persistence between sessions) — `VecDeque` capped at `max_history`
- Chart widget expects `Vec<(f64, f64)>` — convert `Instant` to seconds-since-start when feeding charts
