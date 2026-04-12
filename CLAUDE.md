# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                   # build debug
cargo build --release         # build release
cargo run                     # run (requires Linux for full functionality)
cargo test                    # run all tests
cargo test <test_name>        # run a single test
cargo clippy -- -D warnings   # lint
```

## Project: swaptop

TUI swap/memory manager for Linux (primary target), inspired by `htop`/`btop`. Built with Ratatui + crossterm + tokio.

## Architecture

### Event loop (main.rs → events.rs)

Three concurrent tasks via `tokio::select!`:
- **tick** (1s): triggers Collector → collects `MemSnapshot` → sends via `mpsc` → AppState updates
- **frame** (30fps): triggers `terminal.draw()` with current `&AppState`
- **input**: crossterm `EventStream` → `Action` enum → AppState mutation

`AppState` is shared as `Arc<Mutex<AppState>>` between the collector task and render.

### Platform abstraction (src/platform/)

The collector (`collector.rs`) only touches `Box<dyn SwapBackend>` — never imports platform modules directly.

- `mod.rs` — `SwapBackend` trait (system_swap, system_ram, swap_devices, process_swap, swap_on, swap_off, capabilities)
- `types.rs` — all shared structs: `SwapInfo`, `SwapDevice`, `SwapKind`, `ProcessRow`, `Capabilities`, `MemSnapshot`
- `factory.rs` — `detect() -> Box<dyn SwapBackend>` using `#[cfg(target_os)]`
- `linux.rs` — **primary impl**: sysinfo for totals + `/proc/swaps` for devices + `/proc/PID/smaps` for per-process swap + `nix::mount::swapon/swapoff`
- `macos.rs`, `windows.rs`, `bsd.rs` — stubs/future; macOS reads via glob + sysinfo, no write ops

Linux data sources:
- `sysinfo::System` → RAM totals, process list, CPU%
- `/proc/swaps` → active swap devices
- `/proc/PID/smaps` (field `VmSwap:`) → per-process swap (parse in tokio task, not render)
- `nix::mount::swapon/swapoff` → control (requires root)

### State (app.rs)

`AppState` holds: active tab/phase, ring-buffer histories (`VecDeque<(Instant, u64)>`, max 3600 points), current `MemSnapshot`, process list + sort/filter state, device list, create-swap form, and `Capabilities`.

Mutations happen only via `Action` enum (defined in `actions.rs`) — AppState reducer is pure/no I/O.

### UI (src/ui/)

- `mod.rs` — top-level `render()`, assembles layout and delegates to tab modules
- `overview.rs` — Phase 1: RAM/Swap gauges + history charts
- `processes.rs` — Phase 2/3: sortable process table + process detail with kill
- `devices.rs` — Phase 4: swap device management (swapon/swapoff/reset)
- `create_swap.rs` — Phase 5: wizard (fallocate → chmod → mkswap → swapon)
- `statusbar.rs` — keybindings hint line + error banners

## Implementation phases

```
Phase 1 → event loop, collector, AppState, LinuxBackend basics, overview UI
Phase 2 → smaps parsing, processes table, sorting/filtering
Phase 3 → process_history, detail screen, kill action
Phase 4 → swapon/swapoff via nix, devices UI, root check
Phase 5 → create-swap wizard, tokio::process::Command, disk space validation
```

## After implementing all phases

Run these and fix any issues after implement all phases:

```bash
cargo build          # must compile clean (zero warnings)
cargo clippy -- -D warnings  # must pass with no warnings
cargo test           # all tests must pass
```

## Key constraints

- `collector.rs` must only use the `SwapBackend` trait — never import `linux.rs` etc. directly
- smaps parsing is expensive — always do it in a `tokio::spawn` task, never block render
- `swapon`/`swapoff` require root — check `nix::unistd::geteuid() == 0` before calling
- History is in-memory only (no persistence between sessions) — `VecDeque` capped at `max_history`
- Chart widget expects `Vec<(f64, f64)>` — convert `Instant` to seconds-since-start when feeding charts
