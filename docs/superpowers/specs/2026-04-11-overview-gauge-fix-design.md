# Overview Gauge Fix — Design

**Date:** 2026-04-11  
**Scope:** `src/ui/overview.rs` — gauge section only

## Problem

Each `Gauge` widget is given a 2-line tall area (`Constraint::Length(2)`). Ratatui renders the actual gauge bar on one line; the other line is filled with the `DarkGray` background color. This creates a visually identical "bar" with no label, making the overview appear to have 3 bars (unlabeled + RAM + Swap) instead of 2.

## Fix

Two constraint changes in `src/ui/overview.rs`:

1. **`render_gauges` inner rows** (`overview.rs:33`):  
   `[Constraint::Length(2), Constraint::Length(2)]` → `[Constraint::Length(1), Constraint::Length(1)]`

2. **`render` outer gauges slot** (`overview.rs:17`):  
   `Constraint::Length(4)` → `Constraint::Length(2)`

## Out of scope

- Memory History chart — unchanged
- Device summary line — unchanged
- Status bar — unchanged
- All other tabs — unchanged
