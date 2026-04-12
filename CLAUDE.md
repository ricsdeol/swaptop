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
rtk cargo build          # must compile clean (zero warnings)
rtk cargo clippy -- -D warnings  # must pass with no warnings
rtk cargo test           # all tests must pass
```

## Key constraints

- `collector.rs` must only use the `SwapBackend` trait — never import `linux.rs` etc. directly
- smaps parsing is expensive — always do it in a `tokio::spawn` task, never block render
- `swapon`/`swapoff` require root — check `nix::unistd::geteuid() == 0` before calling
- History is in-memory only (no persistence between sessions) — `VecDeque` capped at `max_history`
- Chart widget expects `Vec<(f64, f64)>` — convert `Instant` to seconds-since-start when feeding charts

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
```

### Test (90-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%)
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
```
<!-- /rtk-instructions -->
