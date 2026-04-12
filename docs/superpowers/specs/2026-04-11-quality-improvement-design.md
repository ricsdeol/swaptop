# Quality Improvement Pass — Design Spec

**Branch:** `chore/improve`
**Date:** 2026-04-11
**Scope:** Architecture refactoring, test coverage tooling, code quality cleanup

---

## 1. Architecture Refactoring

### 1a. Extract `src/input.rs` from `main.rs`

Move the following from `main.rs` into a new `src/input.rs` module:

- `resolve_key_with_context()` (lines 164–229)
- `handle_devices_key()` (lines 231–282)
- `next_sort_column()` (lines 284–292)
- Existing tests for `next_sort_column` (lines 294–311)

After extraction, `main.rs` retains only:
- `main()` setup (backend, state, collector, terminal, shutdown)
- `run()` event loop (`tokio::select!`)
- Device op spawning logic (lines 128–144)

**Result:** `main.rs` drops from ~310 to ~160 lines. Input handling becomes independently testable.

### 1b. Remove `_filter_active` redundant parameter

In `resolve_key_with_context`, the 8th parameter `_filter_active` duplicates the 5th parameter `filter_mode` — both are read from `s.filter_mode` (lines 109 and 111 of current main.rs). Remove the parameter, the tuple field that produces it, and the `#[allow(clippy::too_many_arguments)]` suppression (function drops to 7 params).

### 1c. Tighten visibility

| Target | Current | Change |
|--------|---------|--------|
| `#[allow(dead_code)]` on `SwapBackend` trait | module-level | Remove — trait is used via dyn dispatch |
| `#[allow(dead_code)]` on `ProcReader` struct | struct-level | Remove — struct is used by `LinuxBackend` |
| `#[allow(dead_code)]` on `StatusInfo` | struct-level | Remove — used by `parse_status` |
| `#[allow(dead_code)]` on `parse_status`, `is_kernel_thread`, `parse_kb_value`, `parse_stat_cpu_ticks` | function-level | Remove — all called from `ProcReader::collect()` |
| `#[allow(dead_code)]` on `DeviceOp::kind` | field-level | Keep — genuinely stored for future use |
| `#![allow(dead_code)]` in `types.rs` | file-level | Remove — all types are used |

If removing an `#[allow(dead_code)]` causes a clippy warning, that means the code *is* dead and should be deleted rather than re-suppressed.

---

## 2. Test Coverage & Tooling

### 2a. Coverage tooling

- Add `cargo-llvm-cov` installation step to CI
- Add `cargo llvm-cov --fail-under-lines 70` to CI pipeline
- Add `cargo fmt --check` step to CI (currently missing)

CI workflow additions in `.github/workflows/ci.yml`:
```yaml
- uses: taiki-e/install-action@cargo-llvm-cov

- name: Check formatting
  run: cargo fmt --check

- name: Coverage
  run: cargo llvm-cov --fail-under-lines 70
```

### 2b. MockBackend for Collector tests

Create a `MockBackend` struct implementing `SwapBackend` inside `collector.rs` under `#[cfg(test)]`. No external crate needed — the trait has 6 required methods with simple return types.

Tests to write:
- `collect_assembles_snapshot_from_backend` — verify ram, swap, devices, timestamp fields
- `collect_skips_processes_when_inactive` — `processes_active = false` yields empty process list
- `collect_includes_processes_when_active` — `processes_active = true` yields backend's process list
- `collect_propagates_backend_error` — backend returns `Err`, collector surfaces it

### 2c. Input handler tests (in new `input.rs`)

After extraction, add tests for:
- `filter_mode_captures_printable_chars` — in filter mode, letter keys return `FilterChar`
- `filter_mode_esc_exits` — Esc returns `ExitFilterMode`
- `global_quit_keys_work_from_any_tab` — q, Q, Ctrl+C all return `Quit`
- `tab_keys_cycle_correctly` — Tab/BackTab/1-4 return expected actions
- `process_tab_keys_only_fire_on_process_tab` — j/k/s/slash only active on Processes tab
- `device_keys_require_root_check` — o/f/r without root returns `SetError`

### 2d. Proptest for parsers

Add to `Cargo.toml`:
```toml
[dev-dependencies]
proptest = "1"
```

Property-based tests:
- `parse_proc_swaps`: any 5-field whitespace-separated line with valid integers produces a `SwapDevice`; lines with <5 fields return `None`
- `parse_status`: content with `Name:\t<val>` and `Uid:\t<val>` always produces `Some(StatusInfo)` with matching name/uid
- `parse_stat_cpu_ticks`: result is always `utime + stime` regardless of comm field content (spaces, parens)

---

## 3. Code Quality & Async Hygiene

### 3a. `std::thread::sleep` in `swap_reset`

Keep the current `std::thread::sleep(100ms)` in `LinuxBackend::swap_reset`. It runs inside `spawn_blocking` and changing it to async would require making the `SwapBackend` trait async — out of scope.

Add a clarifying comment:
```rust
// NOTE: This runs inside spawn_blocking, so std::thread::sleep is
// appropriate here. The SwapBackend trait is synchronous by design.
std::thread::sleep(Duration::from_millis(100));
```

### 3b. Slim tokio features

Replace in `Cargo.toml`:
```toml
# Before
tokio = { version = "1.51.1", features = ["full"] }

# After
tokio = { version = "1.51.1", features = ["macros", "rt-multi-thread", "time", "sync", "signal"] }
```

Features justified:
- `macros` — `#[tokio::main]`, `tokio::select!`
- `rt-multi-thread` — multi-threaded runtime for `spawn_blocking`
- `time` — `interval()`, future `sleep()`
- `sync` — `mpsc::unbounded_channel`
- `signal` — `ctrl_c()`

`process` is omitted (Phase 5 not yet implemented; add when needed).

### 3c. `expect()` audit

All current `expect()` calls use the message `"state mutex poisoned"` — already consistent. No changes needed.

---

## Out of Scope

- Making `SwapBackend` trait async
- Adding `thiserror` custom error types (current `color_eyre` usage is appropriate for a binary)
- Integration tests requiring a real `/proc` filesystem
- Benchmarks with criterion (no performance-critical path identified yet)
- Phase 5 (create-swap wizard) implementation

---

## Success Criteria

1. `cargo build` — zero warnings
2. `cargo clippy -- -D warnings` — passes clean
3. `cargo test` — all tests pass (existing 85 + new tests)
4. `cargo fmt --check` — passes
5. `cargo llvm-cov --fail-under-lines 70` — passes
6. No `#[allow(dead_code)]` except on `DeviceOp::kind`
7. `main.rs` < 170 lines
8. CI workflow includes fmt, clippy, test, and coverage steps
