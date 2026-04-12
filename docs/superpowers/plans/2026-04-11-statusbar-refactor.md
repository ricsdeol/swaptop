# Statusbar Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move context-aware key hints (processes navigation, filter mode) from `statusbar.rs` into `processes.rs`, leaving the statusbar with only global commands.

**Architecture:** `processes::render` adopts the indexed-layout pattern (`layout[0]`/`[1]`/`[2]`) where slot `[2]` is always a 1-line footer rendered by `render_footer`. `build_layout(area, filter_mode)` returns `Rc<[Rect]>` with 3 slots: filter bar (0-height when inactive), table, footer. `render_footer` uses `key_span`/`desc_span` helpers and is context-aware. The global statusbar becomes a fixed, unconditional list.

**Tech Stack:** Rust, Ratatui (`Layout`, `Paragraph`, `Line`, `Span`, `Rc<[Rect]>`)

---

## File Map

| File | Change |
|------|--------|
| `src/ui/processes.rs` | `build_layout` → returns `Rc<[Rect]>` (3 slots); add `render_footer`, `key_span`, `desc_span`; update `render`; update tests |
| `src/ui/statusbar.rs` | Remove context logic + `Tab` import; add `("?", "help")` |

---

### Task 1: Rewrite `build_layout` in `processes.rs`

**Files:**
- Modify: `src/ui/processes.rs`

The function changes from returning `(Rect, Option<Rect>)` to `std::rc::Rc<[Rect]>` with 3 fixed-index slots:
- `layout[0]` — filter bar: `Length(3)` when active, `Length(0)` when inactive
- `layout[1]` — table (main content): `Min(0)`
- `layout[2]` — footer (hint bar): `Length(1)`, always present

- [ ] **Step 1: Replace `build_layout` in `src/ui/processes.rs`**

```rust
fn build_layout(area: Rect, filter_mode: bool) -> std::rc::Rc<[Rect]> {
    let filter_height = if filter_mode { 3 } else { 0 };
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(filter_height), // [0] filter bar
            Constraint::Min(0),                // [1] table
            Constraint::Length(1),             // [2] footer / hint bar
        ])
        .split(area)
}
```

Note: `build_layout` is no longer `pub(crate)` — it's private (remove the visibility modifier).

- [ ] **Step 2: Update `processes::render` to use indexed layout**

Replace the entire `render` function:

```rust
pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area, state.filter_mode);

    if state.filter_mode {
        render_filter_bar(f, layout[0], state);
    }

    render_table(f, layout[1], state);
    render_footer(f, layout[2], state);
}
```

- [ ] **Step 3: Update `build_layout` tests**

Replace the three existing `build_layout` tests in the `#[cfg(test)]` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn without_filter_mode_footer_is_1_line_and_filter_slot_is_zero() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area, false);
        assert_eq!(layout[0].height, 0);  // filter bar hidden
        assert_eq!(layout[1].height, 39); // table fills rest
        assert_eq!(layout[2].height, 1);  // footer
        assert_eq!(layout[2].y, 39);
    }

    #[test]
    fn with_filter_mode_filter_is_3_table_shrinks_footer_is_1() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area, true);
        assert_eq!(layout[0].height, 3);  // filter bar
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[1].height, 36); // table
        assert_eq!(layout[2].height, 1);  // footer
        assert_eq!(layout[2].y,      39);
    }

    #[test]
    fn all_slots_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area, true);
        for rect in layout.iter() {
            assert_eq!(rect.width, 120);
        }
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/ui/processes.rs
git commit -m "refactor(processes): build_layout returns Rc<[Rect]> with indexed footer slot"
```

---

### Task 2: Add `render_footer`, `key_span`, `desc_span` to `processes.rs`

**Files:**
- Modify: `src/ui/processes.rs`

- [ ] **Step 1: Add `Line` to imports at top of `src/ui/processes.rs`**

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
```

- [ ] **Step 2: Add helpers and `render_footer` after `render_filter_bar`**

```rust
fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hint_line = if state.filter_mode {
        Line::from(vec![
            key_span("Enter/Esc"), desc_span(" exit filter  "),
            key_span("Backspace"), desc_span(" delete char"),
        ])
    } else {
        Line::from(vec![
            key_span("j/k"), desc_span(" navigate  "),
            key_span("s"),   desc_span(" sort  "),
            key_span("/"),   desc_span(" filter"),
        ])
    };

    f.render_widget(Paragraph::new(hint_line), area);
}

fn key_span(key: &str) -> Span<'_> {
    Span::styled(
        format!(" {key} "),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn desc_span(desc: &str) -> Span<'_> {
    Span::styled(desc, Style::default().fg(Color::DarkGray))
}
```

- [ ] **Step 3: Build and test**

```bash
cargo build && cargo clippy -- -D warnings && cargo test
```

Expected: clean build, no warnings, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/ui/processes.rs
git commit -m "feat(processes): add render_footer with key_span/desc_span context-aware hints"
```

---

### Task 3: Revert `statusbar.rs` to global-only commands

**Files:**
- Modify: `src/ui/statusbar.rs`

- [ ] **Step 1: Replace `statusbar.rs` entirely**

```rust
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::AppState;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let keys: &[(&str, &str)] = &[
        ("q", "quit"),
        ("Tab", "next tab"),
        ("1-4", "switch tab"),
        ("r", "refresh"),
        ("?", "help"),
    ];

    let mut spans: Vec<Span> = keys
        .iter()
        .flat_map(|(key, desc)| {
            [
                Span::styled(
                    format!(" {key} "),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {desc}  "),
                    Style::default().fg(Color::DarkGray),
                ),
            ]
        })
        .collect();

    if let Some(err) = &state.error_msg {
        spans.push(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
```

Changes vs the current file:
- `use crate::app::AppState` — `Tab` removed (no longer needed)
- `let keys` — unconditional `&[...]`, no `if/else if` on `filter_mode` / `active_tab`
- Added `("?", "help")` entry

- [ ] **Step 2: Final verification**

```bash
cargo build && cargo clippy -- -D warnings && cargo test
```

Expected: clean build, no warnings, all tests pass.

- [ ] **Step 3: Commit**

```bash
git add src/ui/statusbar.rs
git commit -m "refactor(statusbar): global-only keybindings; context hints moved to processes panel"
```

---

## Verification

Manual smoke-test after all tasks complete:
1. `cargo run` → on any tab, statusbar shows `q  Tab  1-4  r  ?` — unchanged regardless of tab
2. Switch to Processes tab → bottom line of the content panel shows `j/k navigate  s sort  / filter`
3. Press `/` → bottom line changes to `Enter/Esc exit filter  Backspace delete char`
4. Press Esc → bottom line reverts to navigation hints
