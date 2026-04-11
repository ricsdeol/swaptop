# UI Inner Section Gap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the sub-sections inside the active tab (gauges / history chart / device summary in `overview.rs`) use a vertical gap of `SECTION_VERTICAL_GAP / 2`, so inner spacing is always half the outer spacing between the main TUI sections.

**Architecture:** Bump `SECTION_VERTICAL_GAP` from `1` to `2` (so that integer division `/ 2` yields a meaningful `1`). Expose the constant to child modules via `pub(super)`. Extract a `build_overview_layout` helper in `overview.rs` (mirroring the existing `build_layout` in `mod.rs`), add 7 tests for it, then wire the 5-constraint layout into `render()`. Chunk indices shift: gauges → `[0]`, chart → `[2]`, device summary → `[4]`.

**Tech Stack:** Ratatui 0.29 (`ratatui::layout::{Constraint, Direction, Layout, Rect}`)

---

### Task 1: Bump `SECTION_VERTICAL_GAP` to 2 and fix the one hardcoded-height test

**Files:**
- Modify: `src/ui/mod.rs:4` (constant value)
- Modify: `src/ui/mod.rs:160-165` (minimal-terminal test)

> **Why only one test needs a code change:** all other tests reference `SECTION_VERTICAL_GAP` symbolically — they automatically reflect the new value. Only `layout_is_stable_on_minimal_terminal` uses a hardcoded terminal height `6` that was calculated for gap=1 (`3+1+0+1+1=6`). For gap=2 the minimum is `3+2+0+2+1=8`.

- [ ] **Step 1: Change the constant value**

In `src/ui/mod.rs` line 4, change:

```rust
const SECTION_VERTICAL_GAP: u16 = 1;
```

to:

```rust
pub(super) const SECTION_VERTICAL_GAP: u16 = 2;
```

`pub(super)` makes it visible to `overview.rs` (a child module of `ui`) so it can reference `super::SECTION_VERTICAL_GAP` without duplicating the value.

- [ ] **Step 2: Fix the hardcoded minimal-terminal test**

In `src/ui/mod.rs`, replace the `layout_is_stable_on_minimal_terminal` test:

```rust
#[test]
fn layout_is_stable_on_minimal_terminal() {
    // minimum: 3 (tabbar) + 2 (gap) + 0 (content) + 2 (gap) + 1 (statusbar) = 8
    let area = Rect::new(0, 0, 40, 8);
    let layout = build_layout(area);
    assert_eq!(layout[2].height, 0);
}
```

- [ ] **Step 3: Run all existing tests to verify they pass**

```bash
cargo test ui::tests
```

Expected:
```
test ui::tests::tabbar_starts_at_top_and_has_correct_height ... ok
test ui::tests::gap_after_tabbar_has_correct_position ... ok
test ui::tests::content_starts_after_tabbar_and_gap ... ok
test ui::tests::gap_before_statusbar_has_correct_position ... ok
test ui::tests::statusbar_is_last_row ... ok
test ui::tests::sections_span_full_width ... ok
test ui::tests::layout_is_stable_on_minimal_terminal ... ok
```

- [ ] **Step 4: Build and clippy**

```bash
cargo build && cargo clippy -- -D warnings
```

Expected: `Finished` with zero warnings.

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs
git commit -m "refactor(ui): bump SECTION_VERTICAL_GAP to 2, expose pub(super) for child modules"
```

---

### Task 2: Extract `build_overview_layout`, add tests, wire into `render()`

**Files:**
- Modify: `src/ui/overview.rs`

The inner gap for overview sub-sections is `SECTION_VERTICAL_GAP / 2` = `2 / 2` = `1`. The layout becomes 5 constraints: `[gauges, gap, chart, gap, device-summary]`.

- [ ] **Step 1: Write the failing tests**

Add this block at the bottom of `src/ui/overview.rs` (after all existing functions):

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use super::build_overview_layout;
    use crate::ui::SECTION_VERTICAL_GAP;

    #[test]
    fn gauges_start_at_top() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 2);
    }

    #[test]
    fn gap_after_gauges_is_half_outer_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        let inner_gap = SECTION_VERTICAL_GAP / 2;
        assert_eq!(layout[1].y,      2);
        assert_eq!(layout[1].height, inner_gap);
    }

    #[test]
    fn chart_starts_after_gauges_and_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        let inner_gap = SECTION_VERTICAL_GAP / 2;
        assert_eq!(layout[2].y, 2 + inner_gap);
    }

    #[test]
    fn gap_before_device_summary_is_half_outer_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        let inner_gap = SECTION_VERTICAL_GAP / 2;
        // content ends at: 40 - 2 (device-summary) - inner_gap (lower gap)
        let expected_y = area.height - 2 - inner_gap;
        assert_eq!(layout[3].y,      expected_y);
        assert_eq!(layout[3].height, inner_gap);
    }

    #[test]
    fn device_summary_is_last_two_rows() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_overview_layout(area);
        assert_eq!(layout[4].y,      area.height - 2);
        assert_eq!(layout[4].height, 2);
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
    fn layout_stable_on_minimal_terminal() {
        // minimum: 2 (gauges) + 1 (gap) + 8 (chart Min) + 1 (gap) + 2 (device) = 14
        // use a terminal taller than minimum to let Min(8) resolve naturally
        let area = Rect::new(0, 0, 40, 20);
        let layout = build_overview_layout(area);
        assert!(layout[2].height >= 8);
    }
}
```

- [ ] **Step 2: Run tests — expect compile error**

```bash
cargo test ui::overview::tests 2>&1 | head -20
```

Expected: compile error — `build_overview_layout` not found.

- [ ] **Step 3: Add `build_overview_layout` helper and update `render()`**

Replace the entire `pub fn render` function and add the helper. In `src/ui/overview.rs`:

First, add the import of `SECTION_VERTICAL_GAP` at the top, alongside the existing `use crate::app::AppState;` line:

```rust
use crate::app::AppState;
use crate::ui::SECTION_VERTICAL_GAP;
```

Then replace:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // gauges
            Constraint::Min(8),    // history chart
            Constraint::Length(2), // device summary
        ])
        .split(area);

    render_gauges(f, chunks[0], state);
    render_chart(f, chunks[1], state);
    render_device_summary(f, chunks[2], state);
}
```

With:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_overview_layout(area);
    render_gauges(f, layout[0], state);
    render_chart(f, layout[2], state);
    render_device_summary(f, layout[4], state);
}

fn build_overview_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    let inner_gap = SECTION_VERTICAL_GAP / 2;
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),           // [0] gauges
            Constraint::Length(inner_gap),   // [1] gap
            Constraint::Min(8),              // [2] history chart
            Constraint::Length(inner_gap),   // [3] gap
            Constraint::Length(2),           // [4] device summary
        ])
        .split(area)
}
```

- [ ] **Step 4: Run overview tests — must all pass**

```bash
cargo test ui::overview::tests
```

Expected:
```
test ui::overview::tests::gauges_start_at_top ... ok
test ui::overview::tests::gap_after_gauges_is_half_outer_gap ... ok
test ui::overview::tests::chart_starts_after_gauges_and_gap ... ok
test ui::overview::tests::gap_before_device_summary_is_half_outer_gap ... ok
test ui::overview::tests::device_summary_is_last_two_rows ... ok
test ui::overview::tests::all_sections_span_full_width ... ok
test ui::overview::tests::layout_stable_on_minimal_terminal ... ok
```

- [ ] **Step 5: Run all tests**

```bash
cargo test
```

Expected: all tests pass (≥ 36 total: 29 existing + 7 new).

- [ ] **Step 6: Build and clippy**

```bash
cargo build && cargo clippy -- -D warnings
```

Expected: `Finished` with zero warnings.

- [ ] **Step 7: Commit**

```bash
git add src/ui/overview.rs
git commit -m "feat(ui): add inner section gaps in overview (half of SECTION_VERTICAL_GAP)"
```
