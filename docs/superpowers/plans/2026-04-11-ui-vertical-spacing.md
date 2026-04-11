# UI Vertical Section Spacing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the existing horizontal margins (15-col outer, 10-col inner) with vertical gaps between TUI sections, so the tabbar, content area, and statusbar use the full terminal width with 1-row breathing room between each section.

**Architecture:** Remove both `Rect::inner` horizontal-margin calls from `render()` and replace the 3-constraint vertical layout with a 5-constraint layout that interleaves `Constraint::Length(SECTION_VERTICAL_GAP)` gap rows between sections. Indices shift: tabbar → `layout[0]`, content → `layout[2]`, statusbar → `layout[4]`. Replace horizontal-margin constants with a single `SECTION_VERTICAL_GAP` constant. Remove the now-unused `Margin` import.

**Tech Stack:** Ratatui 0.29 (`ratatui::layout::{Constraint, Direction, Layout, Rect}`)

---

### Task 1: Replace horizontal-margin constants/imports with vertical-gap constant and update tests

**Files:**
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write the failing tests**

Replace the entire `#[cfg(test)] mod tests { … }` block at the bottom of `src/ui/mod.rs` with:

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use super::SECTION_VERTICAL_GAP;

    fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),                    // [0] tab bar
                Constraint::Length(SECTION_VERTICAL_GAP), // [1] gap
                Constraint::Min(0),                       // [2] content
                Constraint::Length(SECTION_VERTICAL_GAP), // [3] gap
                Constraint::Length(1),                    // [4] status bar
            ])
            .split(area)
    }

    #[test]
    fn tabbar_starts_at_top_and_has_correct_height() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 3);
    }

    #[test]
    fn gap_after_tabbar_has_correct_position() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[1].y,      3);
        assert_eq!(layout[1].height, SECTION_VERTICAL_GAP);
    }

    #[test]
    fn content_starts_after_tabbar_and_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[2].y, 3 + SECTION_VERTICAL_GAP);
    }

    #[test]
    fn gap_before_statusbar_has_correct_position() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        // content ends at: 40 - 1 (statusbar) - SECTION_VERTICAL_GAP (lower gap) = 38
        let expected_gap_y = area.height - 1 - SECTION_VERTICAL_GAP;
        assert_eq!(layout[3].y,      expected_gap_y);
        assert_eq!(layout[3].height, SECTION_VERTICAL_GAP);
    }

    #[test]
    fn statusbar_is_last_row() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[4].y,      area.height - 1);
        assert_eq!(layout[4].height, 1);
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
        // 5 rows minimum: 3 (tabbar) + 1 (gap) + 0 (content) + 1 (gap) + 1 (statusbar)
        let area = Rect::new(0, 0, 40, 6);
        let layout = build_layout(area);
        // Must not panic; content height should be 0 or 1 (saturated)
        assert!(layout[2].height <= 1);
    }
}
```

- [ ] **Step 2: Run tests — expect failure (constants and Margin not changed yet)**

```bash
cargo test ui::tests 2>&1 | head -30
```

Expected: compile error mentioning `SECTION_VERTICAL_GAP` not found / `OUTER_HORIZONTAL_MARGIN` unused.

- [ ] **Step 3: Update constants and imports in `src/ui/mod.rs`**

Replace the top of the file (lines 1–13) with:

```rust
mod overview;
mod statusbar;

const SECTION_VERTICAL_GAP: u16 = 1;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::app::{AppState, Tab};
```

Key changes:
- `OUTER_HORIZONTAL_MARGIN` and `CONTENT_HORIZONTAL_MARGIN` removed
- `Margin` removed from the `layout::` import (no longer used)
- `SECTION_VERTICAL_GAP: u16 = 1` added

- [ ] **Step 4: Run tests — expect pass**

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

---

### Task 2: Update `render()` to use full-width layout with vertical gaps

**Files:**
- Modify: `src/ui/mod.rs:17-41`

- [ ] **Step 1: Replace the `render()` function body**

Replace the current `render()` function (lines 17–41) with:

```rust
pub fn render(f: &mut Frame, state: &AppState) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),                    // [0] tab bar
            Constraint::Length(SECTION_VERTICAL_GAP), // [1] gap
            Constraint::Min(0),                       // [2] content
            Constraint::Length(SECTION_VERTICAL_GAP), // [3] gap
            Constraint::Length(1),                    // [4] status bar
        ])
        .split(area);

    render_tabbar(f, layout[0], state);

    match state.active_tab {
        Tab::Overview => overview::render(f, layout[2], state),
        _ => render_coming_soon(f, layout[2]),
    }

    statusbar::render(f, layout[4], state);
}
```

Key changes vs. old code:
- `f.area().inner(Margin { horizontal: OUTER_HORIZONTAL_MARGIN, … })` → `f.area()` (full width)
- 3-constraint layout → 5-constraint layout with gap rows at indices `[1]` and `[3]`
- `layout[1]` (old content) → `layout[2]`; `layout[2]` (old statusbar) → `layout[4]`
- `let content_area = layout[1].inner(Margin { horizontal: CONTENT_HORIZONTAL_MARGIN, … })` removed — tab modules receive `layout[2]` directly

- [ ] **Step 2: Build — must compile with zero warnings**

```bash
cargo build
```

Expected: `Finished \`dev\` profile` with no warnings.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: clean pass, no warnings.

- [ ] **Step 4: Run all tests**

```bash
cargo test
```

Expected: all tests pass (≥ 7 in `ui::tests`, plus all pre-existing tests).

- [ ] **Step 5: Commit**

```bash
git add src/ui/mod.rs
git commit -m "refactor(ui): replace horizontal margins with 1-row vertical gaps between sections"
```
