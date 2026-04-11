# UI Horizontal Margin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (1) Apply a 15-cell horizontal margin to the entire TUI so that the tabbar, content area, and statusbar are all inset 15 columns from each terminal edge. (2) Apply an additional 10-cell horizontal margin to the active tab content area only, so tab content is further indented relative to the chrome.

**Architecture:** Two `Rect::inner` calls in `render()` — one on `f.area()` (15-cell outer margin, affects all sections), and one on `layout[1]` (10-cell inner margin, affects only the active tab content). Each tab render function receives the already-shrunk rect so no changes are needed inside `overview.rs` or future tab modules.

**Tech Stack:** Ratatui 0.29 (`ratatui::layout::{Margin, Rect}`)

---

### Task 1: Add horizontal margin to the main render area

**Files:**
- Modify: `src/ui/mod.rs:14-24`

- [ ] **Step 1: Write the failing tests**

Add to the bottom of `src/ui/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use ratatui::layout::{Margin, Rect};

    #[test]
    fn outer_margin_shrinks_width_by_twice_margin() {
        let full = Rect::new(0, 0, 120, 40);
        let inner = full.inner(Margin { horizontal: 15, vertical: 0 });
        assert_eq!(inner.x,      15);
        assert_eq!(inner.width,  90);   // 120 - 15*2
        assert_eq!(inner.y,      0);
        assert_eq!(inner.height, 40);   // unchanged
    }

    #[test]
    fn outer_margin_clamps_on_narrow_terminal() {
        // If the terminal is narrower than 2*margin, inner width must not wrap
        let narrow = Rect::new(0, 0, 20, 40);
        let inner  = narrow.inner(Margin { horizontal: 15, vertical: 0 });
        // Ratatui saturates to 0 — must not panic
        assert_eq!(inner.width, 0);
    }

    #[test]
    fn content_margin_further_shrinks_content_area() {
        // Simulates: outer area after 15-cell margin, then 10-cell content margin
        let after_outer = Rect::new(15, 0, 90, 36); // as if outer margin was applied
        let content = after_outer.inner(Margin { horizontal: 10, vertical: 0 });
        assert_eq!(content.x,      25);  // 15 + 10
        assert_eq!(content.width,  70);  // 90 - 10*2
        assert_eq!(content.y,      0);
        assert_eq!(content.height, 36);  // unchanged
    }

    #[test]
    fn content_margin_clamps_on_narrow_content_area() {
        let narrow_content = Rect::new(15, 0, 10, 36);
        let content = narrow_content.inner(Margin { horizontal: 10, vertical: 0 });
        assert_eq!(content.width, 0);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (stdlib behaviour, no impl needed)**

```bash
cargo test ui::tests
```

Expected:
```
test ui::tests::outer_margin_shrinks_width_by_twice_margin ... ok
test ui::tests::outer_margin_clamps_on_narrow_terminal ... ok
test ui::tests::content_margin_further_shrinks_content_area ... ok
test ui::tests::content_margin_clamps_on_narrow_content_area ... ok
```

- [ ] **Step 3: Apply the margin in `render()`**

In `src/ui/mod.rs`, update the imports and the `render` function:

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};
```

Replace the `let area = f.area();` line in `render()`:

```rust
pub fn render(f: &mut Frame, state: &AppState) {
    // 15-cell outer margin: tabbar, content, and statusbar all inset from terminal edges
    let area = f.area().inner(Margin { horizontal: 15, vertical: 0 });

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_tabbar(f, layout[0], state);

    // 10-cell additional margin for the active tab content area
    let content_area = layout[1].inner(Margin { horizontal: 10, vertical: 0 });

    match state.active_tab {
        Tab::Overview => overview::render(f, content_area, state),
        _ => render_coming_soon(f, content_area),
    }

    statusbar::render(f, layout[2], state);
}
```

- [ ] **Step 4: Build and verify zero warnings**

```bash
cargo build
```

Expected: `Finished \`dev\` profile` with no warnings.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -- -D warnings
```

Expected: `Finished` with no warnings or errors.

- [ ] **Step 6: Run all tests**

```bash
cargo test
```

Expected: all tests pass (≥ 26 tests including the 4 new ones).

- [ ] **Step 7: Commit**

```bash
git add src/ui/mod.rs
git commit -m "feat(ui): 15-col outer margin on all sections, 10-col inner margin on active tab content"
```
