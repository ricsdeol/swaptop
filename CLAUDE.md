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

### Event loop (`main.rs`)

Five arms via `tokio::select!`: **shutdown** (CancellationToken); **action_rx** (unbounded `mpsc` — results from background `spawn_blocking` tasks, e.g. DeviceOp, CreateSwap); **tick** (1s) → Collector → `MemSnapshot` → AppState; **frame_tick** (30fps) → `terminal.draw()`; **events** → key → `input::resolve_key` → `Action` → AppState mutation. `AppState` shared as `Arc<Mutex<AppState>>`.

### Platform abstraction (`src/platform/`)

`collector.rs` only touches `Box<dyn SwapBackend>` — never imports platform modules directly.

- `mod.rs` — `SwapBackend` trait + `Capabilities`
- `types.rs` — shared structs (`SwapInfo`, `SwapDevice`, `ProcessRow`, `MemSnapshot`, …) + `parse_swap_header`
- `factory.rs` — `detect() -> Box<dyn SwapBackend>` via `#[cfg(target_os)]`
- `linux/mod.rs` — primary impl: sysinfo + `/proc/swaps` + `/proc/PID/smaps` + `nix::mount`
- `linux/proc_reader.rs` — low-level `/proc` parsing helpers
- `linux/create_swap.rs` — background step runner: fallocate → chmod → mkswap → swapon
- `swap_discovery.rs` — glob-based swap file discovery (cross-platform)
- `macos.rs`, `windows.rs`, `bsd.rs` — stubs

### State (`app.rs`)

`AppState`: tab, ring-buffer histories (`VecDeque`, max 3600), `MemSnapshot`, process/device/form state, `Capabilities`. Mutations only via `Action` enum (defined in `actions.rs`) — reducer is pure/no I/O.

### Supporting modules

- `actions.rs` — `Action` enum (all state mutations) + `SortColumn`, `DeviceOp`, `OpStatus`
- `input.rs` — `resolve_key()`: maps crossterm key events + `KeyContext` → `Option<Action>`
- `tui.rs` — terminal init/restore helpers
- `create_swap.rs` (`src/`) — wizard state types (`CreateSwapModal`, `CreateSwapField`, `StepStatus`, etc.)

### UI (`src/ui/`)

- `overview.rs` — RAM/Swap gauges + history charts
- `processes.rs` — sortable process table + detail/kill
- `devices.rs` — swapon/swapoff/reset
- `create_swap.rs` — wizard form UI (modal, inputs, progress display)
- `statusbar.rs` — keybindings + error banners
- `design.rs` — shared colour/style constants

## After implementing all phases

Run these and fix any issues after implement all phases and tasks:

```bash
rtk cargo build          # must compile clean (zero warnings)
rtk cargo clippy -- -D warnings  # must pass with no warnings
rtk cargo fmt --check     # must be properly formatted
rtk cargo test           # all tests must pass
```

## Key constraints

- `collector.rs` must only use the `SwapBackend` trait — never import `linux.rs` etc. directly
- smaps parsing is expensive — always do it in a `tokio::spawn` task, never block render
- `swapon`/`swapoff` require root — check `nix::unistd::geteuid() == 0` before calling
- History is in-memory only (no persistence between sessions) — `VecDeque` capped at `max_history`
- Chart widget expects `Vec<(f64, f64)>` — convert `Instant` to seconds-since-start when feeding charts
