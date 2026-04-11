# UI Design System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract all UI spacing constants into `src/ui/design.rs` (single source of truth), then use Ratatui's native `Layout::spacing(N)` API to apply gaps — `design::OUTER_GAP` in `mod.rs` chrome layout and `design::INNER_GAP` in every tab view layout.

**Architecture:** `src/ui/design.rs` exports `OUTER_GAP: u16 = 2` and `INNER_GAP: u16 = OUTER_GAP / 2`. The `.spacing(N)` method on `Layout` inserts N blank rows between every segment automatically, eliminating interleaved `Constraint::Length` gap entries. Chrome layout (`mod.rs`) goes from 5 constraints back to 3 (`[tabbar, content, statusbar]`); overview layout (`overview.rs`) goes from 5 back to 3 (`[gauges, chart, device-summary]`). Every future tab module just calls `.spacing(design::INNER_GAP)` — one line, no manual gap slots.

**Tech Stack:** Ratatui 0.29 — `Layout::spacing()`, `ratatui::layout::{Constraint, Direction, Layout, Rect}`

> **Replaces:** manual `Constraint::Length(SECTION_VERTICAL_GAP)` gap rows added in the previous plan.
> **Supersedes:** `docs/superpowers/plans/2026-04-11-ui-inner-section-gap.md`

---

## Why `Layout::spacing()` instead of gap constraints

The current layout in `mod.rs` uses 5 constraints to express 3 sections + 2 gaps:
```rust
[Length(3), Length(OUTER_GAP), Min(0), Length(OUTER_GAP), Length(1)]
//  tabbar     gap             content   gap              statusbar
```
Ratatui's `.spacing(N)` expresses the same thing with 3 constraints:
```rust
Layout::vertical([Length(3), Min(0), Length(1)]).spacing(OUTER_GAP)
```
Benefits: simpler indices (`[0]`/`[1]`/`[2]` not `[0]`/`[2]`/`[4]`), fewer constraints to count, spacing intent is explicit, and every tab module inherits the pattern for free.

---

## File map

| File | Action | Responsibility |
|---|---|---|
| `src/ui/design.rs` | **Create** | Single source of truth: `OUTER_GAP`, `INNER_GAP` |
| `src/ui/mod.rs` | **Modify** | Use `design::OUTER_GAP` via `.spacing()`, simplify to 3-constraint layout |
| `src/ui/overview.rs` | **Modify** | Extract `build_overview_layout` with `.spacing(design::INNER_GAP)` + 7 tests |

---

### Task 1: Create `src/ui/design.rs`

**Files:**
- Create: `src/ui/design.rs`

- [ ] **Step 1: Write the file with tests**

```rust
/// Vertical gap between the main chrome sections (tab bar ↕ content ↕ status bar).
/// Must be ≥ 2 so that INNER_GAP = OUTER_GAP / 2 ≥ 1.
pub const OUTER_GAP: u16 = 2;

/// Vertical gap between sub-sections inside any tab view.
/// Always half of OUTER_GAP so inner spacing is subordinate to outer spacing.
pub const INNER_GAP: u16 = OUTER_GAP / 2;

#[cfg(test)]
mod tests {
    use super::{INNER_GAP, OUTER_GAP};

    #[test]
    fn inner_gap_is_half_of_outer_gap() {
        assert_eq!(INNER_GAP, OUTER_GAP / 2);
    }

    #[test]
    fn outer_gap_is_at_least_two_so_inner_gap_is_nonzero() {
        assert!(OUTER_GAP >= 2, "OUTER_GAP must be ≥ 2 so INNER_GAP ≥ 1");
        assert!(INNER_GAP >= 1, "INNER_GAP must be ≥ 1 for visible spacing");
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test ui::design::tests
```

Expected:
```
test ui::design::tests::inner_gap_is_half_of_outer_gap ... ok
test ui::design::tests::outer_gap_is_at_least_two_so_inner_gap_is_nonzero ... ok
```

- [ ] **Step 3: Commit**

```bash
git add src/ui/design.rs
git commit -m "feat(ui/design): add design system module with OUTER_GAP=2 and INNER_GAP=1"
```

---

### Task 2: Migrate `mod.rs` to `design::OUTER_GAP` via `.spacing()`

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Register the design module and remove the old constant**

Replace lines 1–4 in `src/ui/mod.rs`:

```rust
mod design;
mod overview;
mod statusbar;
```

The local `const SECTION_VERTICAL_GAP` is deleted entirely — `design::OUTER_GAP` replaces it.

- [ ] **Step 2: Update `build_layout` — 3 constraints + `.spacing()`**

Replace the current `build_layout` function:

```rust
fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // [0] tab bar
            Constraint::Min(0),    // [1] content
            Constraint::Length(1), // [2] status bar
        ])
        .spacing(design::OUTER_GAP)
        .split(area)
}
```

Indices are now `[0]` tabbar, `[1]` content, `[2]` statusbar.

- [ ] **Step 3: Update `render()` to use new indices**

Replace the `render()` function:

```rust
pub fn render(f: &mut Frame, state: &AppState) {
    let layout = build_layout(f.area());

    render_tabbar(f, layout[0], state);

    match state.active_tab {
        Tab::Overview => overview::render(f, layout[1], state),
        _ => render_coming_soon(f, layout[1]),
    }

    statusbar::render(f, layout[2], state);
}
```

- [ ] **Step 4: Replace the test module**

Replace the entire `#[cfg(test)] mod tests { … }` block:

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use crate::ui::design::{INNER_GAP, OUTER_GAP};

    fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
        super::build_layout(area)
    }

    #[test]
    fn tabbar_starts_at_top_and_has_correct_height() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 3);
    }

    #[test]
    fn content_starts_after_tabbar_plus_outer_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[1].y, 3 + OUTER_GAP);
    }

    #[test]
    fn statusbar_is_last_row() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[2].y,      area.height - 1);
        assert_eq!(layout[2].height, 1);
    }

    #[test]
    fn sections_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        for rect in layout.iter() {
            assert_eq!(rect.x,     0);
            assert_eq!(rect.width, 120);
        }
    }

    #[test]
    fn layout_is_stable_on_minimal_terminal() {
        // minimum rows: 3 (tabbar) + OUTER_GAP(2) + 0 (content) + OUTER_GAP(2) + 1 (statusbar) = 8
        let area = Rect::new(0, 0, 40, 8);
        let layout = build_layout(area);
        assert_eq!(layout[1].height, 0);
    }

    #[test]
    fn inner_gap_accessible_and_half_of_outer() {
        assert_eq!(INNER_GAP * 2, OUTER_GAP);
    }

    // Verify spacing is applied consistently using a direct Layout call
    #[test]
    fn spacing_produces_same_positions_as_explicit_gaps() {
        let area = Rect::new(0, 0, 120, 40);
        // Using .spacing()
        let with_spacing = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .spacing(OUTER_GAP)
            .split(area);
        // Tabbar at 0, content at 3+OUTER_GAP, statusbar at last row
        assert_eq!(with_spacing[0].y, 0);
        assert_eq!(with_spacing[1].y, 3 + OUTER_GAP);
        assert_eq!(with_spacing[2].y, area.height - 1);
    }
}
```

Note: the old tests for explicit gap rows (`gap_after_tabbar_has_correct_position`, `gap_before_statusbar_has_correct_position`) are removed — `.spacing()` owns that behaviour now. Replaced by `spacing_produces_same_positions_as_explicit_gaps` which verifies the API contract directly.

- [ ] **Step 5: Build and run all tests**

```bash
cargo build && cargo clippy -- -D warnings && cargo test ui::
```

Expected: all `ui::` tests pass (7 `ui::tests` + 2 `ui::design::tests`), zero warnings.

- [ ] **Step 6: Commit**

```bash
git add src/ui/mod.rs
git commit -m "refactor(ui): replace gap constraints with Layout::spacing(design::OUTER_GAP)"
```

---

### Task 3: Apply `design::INNER_GAP` via `.spacing()` in `overview.rs`

**Files:**
- Modify: `src/ui/overview.rs`

- [ ] **Step 1: Write the failing tests**

Add this block at the bottom of `src/ui/overview.rs`:

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use crate::ui::design::{INNER_GAP, OUTER_GAP};
    use super::build_overview_layout;

    #[test]
    fn gauges_start_at_top() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 2);
    }

    #[test]
    fn chart_starts_after_gauges_plus_inner_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        assert_eq!(layout[1].y, 2 + INNER_GAP);
    }

    #[test]
    fn device_summary_occupies_last_two_rows() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        assert_eq!(layout[2].y,      area.height - 2);
        assert_eq!(layout[2].height, 2);
    }

    #[test]
    fn all_sections_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        for rect in layout.iter() {
            assert_eq!(rect.x,     0);
            assert_eq!(rect.width, 120);
        }
    }

    #[test]
    fn inner_gap_is_half_of_outer_gap() {
        assert_eq!(INNER_GAP * 2, OUTER_GAP);
    }

    #[test]
    fn layout_stable_on_minimal_terminal() {
        // minimum: 2 (gauges) + INNER_GAP(1) + 8 (chart Min) + INNER_GAP(1) + 2 (device) = 14
        let area = Rect::new(0, 0, 40, 14);
        let layout = build_overview_layout(area);
        assert_eq!(layout[1].height, 8); // chart gets exactly its Min
    }

    #[test]
    fn spacing_produces_same_result_as_direct_layout_call() {
        let area = Rect::new(0, 0, 120, 40);
        let direct = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Min(8),
                Constraint::Length(2),
            ])
            .spacing(INNER_GAP)
            .split(area);
        let via_helper = build_overview_layout(area);
        assert_eq!(direct[0], via_helper[0]);
        assert_eq!(direct[1], via_helper[1]);
        assert_eq!(direct[2], via_helper[2]);
    }
}
```

- [ ] **Step 2: Run tests — expect compile error**

```bash
cargo test ui::overview::tests 2>&1 | head -20
```

Expected: compile error — `build_overview_layout` not found.

- [ ] **Step 3: Add import, extract helper, update `render()`**

Add the design import below the existing `use crate::app::AppState;` line:

```rust
use crate::app::AppState;
use crate::ui::design;
```

Replace the `pub fn render` function:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_overview_layout(area);
    render_gauges(f, layout[0], state);
    render_chart(f, layout[1], state);
    render_device_summary(f, layout[2], state);
}

fn build_overview_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // [0] gauges
            Constraint::Min(8),    // [1] history chart
            Constraint::Length(2), // [2] device summary
        ])
        .spacing(design::INNER_GAP)
        .split(area)
}
```

Indices are back to clean 0/1/2 — no gap slots to skip.

- [ ] **Step 4: Run overview tests — all must pass**

```bash
cargo test ui::overview::tests
```

Expected:
```
test ui::overview::tests::gauges_start_at_top ... ok
test ui::overview::tests::chart_starts_after_gauges_plus_inner_gap ... ok
test ui::overview::tests::device_summary_occupies_last_two_rows ... ok
test ui::overview::tests::all_sections_span_full_width ... ok
test ui::overview::tests::inner_gap_is_half_of_outer_gap ... ok
test ui::overview::tests::layout_stable_on_minimal_terminal ... ok
test ui::overview::tests::spacing_produces_same_result_as_direct_layout_call ... ok
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all tests pass (≥ 38 total: pre-existing + 2 design + 7 overview + 7 mod).

- [ ] **Step 6: Build and clippy**

```bash
cargo build && cargo clippy -- -D warnings
```

Expected: `Finished` with zero warnings.

- [ ] **Step 7: Commit**

```bash
git add src/ui/overview.rs
git commit -m "feat(ui/overview): extract build_overview_layout with Layout::spacing(design::INNER_GAP)"
```

---

## Pattern for future tab modules

Every new tab module (processes, devices, create_swap) follows this template:

```rust
use crate::ui::design;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area);
    // render sub-sections using layout[0], layout[1], …
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([/* tab-specific constraints */])
        .spacing(design::INNER_GAP)  // ← only line that ties into the design system
        .split(area)
}
```
