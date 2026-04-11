# Overview Gauge Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the phantom unlabeled bar from the Overview tab by shrinking each gauge to exactly 1 terminal row.

**Architecture:** Two constraint values in `src/ui/overview.rs` control gauge height. The outer layout allocates 4 lines for gauges; the inner layout splits those into two 2-line rows, one per gauge. Ratatui's `Gauge` widget renders the bar on one line and leaves the other line filled with `DarkGray`, which looks like a third bar. Reducing both to 1 line eliminates the artifact.

**Tech Stack:** Rust, Ratatui (layout constraints)

---

### Task 1: Fix gauge height constraints

**Files:**
- Modify: `src/ui/overview.rs:17` (outer layout — gauges slot)
- Modify: `src/ui/overview.rs:33` (inner layout — per-gauge rows)

- [ ] **Step 1: Apply the two constraint changes**

In `src/ui/overview.rs`, make these two edits:

**Edit 1 — outer layout (line 17), change `Length(4)` → `Length(2)`:**

```rust
// Before
Constraint::Length(4), // gauges
// After
Constraint::Length(2), // gauges
```

**Edit 2 — inner rows (line 33), change both `Length(2)` → `Length(1)`:**

```rust
// Before
.constraints([Constraint::Length(2), Constraint::Length(2)])
// After
.constraints([Constraint::Length(1), Constraint::Length(1)])
```

- [ ] **Step 2: Build and lint**

```bash
cargo clippy -- -D warnings
```

Expected: zero warnings, zero errors.

- [ ] **Step 3: Run existing tests**

```bash
cargo test
```

Expected: all tests pass (layout changes don't affect AppState logic tests).

- [ ] **Step 4: Commit**

```bash
git add src/ui/overview.rs
git commit -m "fix: reduce gauge height to 1 line, remove phantom unlabeled bar"
```
