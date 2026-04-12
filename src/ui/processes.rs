use human_bytes::human_bytes;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::actions::{SortColumn, SortDir};
use crate::app::AppState;
use crate::platform::ProcessRow;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let (table_area, filter_area) = build_layout(area, state.filter_mode);

    if let Some(fa) = filter_area {
        render_filter_bar(f, fa, state);
    }

    let render_area = if !state.capabilities.has_per_process {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(table_area);
        render_platform_banner(f, parts[0]);
        parts[1]
    } else {
        table_area
    };

    render_table(f, render_area, state);
}

pub(crate) fn build_layout(area: Rect, filter_mode: bool) -> (Rect, Option<Rect>) {
    if filter_mode {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        (parts[1], Some(parts[0]))
    } else {
        (area, None)
    }
}

fn render_filter_bar(f: &mut Frame, area: Rect, state: &AppState) {
    let text = format!(" {}_", state.filter_text);
    let p = Paragraph::new(text)
        .block(
            Block::default()
                .title(" Filter ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .style(Style::default().fg(Color::White));
    f.render_widget(p, area);
}

fn render_platform_banner(f: &mut Frame, area: Rect) {
    let p = Paragraph::new(Span::styled(
        "  Swap usage per process is not available on this platform",
        Style::default().fg(Color::Yellow),
    ));
    f.render_widget(p, area);
}

fn render_table(f: &mut Frame, area: Rect, state: &AppState) {
    let has_per_process = state.capabilities.has_per_process;

    // Build filtered list
    let lower = state.filter_text.to_lowercase();
    let visible: Vec<&ProcessRow> = if lower.is_empty() {
        state.processes.iter().collect()
    } else {
        state.processes
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .collect()
    };

    if visible.is_empty() {
        let p = Paragraph::new("  No processes found")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }

    let header = Row::new(vec![
        header_cell("PID",  &SortColumn::Pid,  state),
        header_cell("Name", &SortColumn::Name, state),
        header_cell("User", &SortColumn::User, state),
        header_cell("RSS",  &SortColumn::Rss,  state),
        header_cell("Swap", &SortColumn::Swap, state),
        header_cell("CPU%", &SortColumn::Cpu,  state),
    ])
    .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = visible
        .iter()
        .map(|p| {
            let swap_cell = if has_per_process {
                Cell::from(format!("{:>10}", human_bytes(p.swap as f64)))
            } else {
                Cell::from(format!("{:>10}", "—"))
                    .style(Style::default().fg(Color::DarkGray))
            };
            Row::new(vec![
                Cell::from(format!("{:>6}", p.pid)),
                Cell::from(p.name.clone()),
                Cell::from(p.user.clone()),
                Cell::from(format!("{:>10}", human_bytes(p.rss as f64))),
                swap_cell,
                Cell::from(format!("{:>5.1}%", p.cpu_pct)),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(7),
        Constraint::Min(20),
        Constraint::Length(12),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(7),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    let mut table_state = TableState::default();
    table_state.select(Some(state.selected_row));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn header_cell<'a>(label: &'a str, col: &SortColumn, state: &AppState) -> Cell<'a> {
    let indicator = if col == &state.sort_col {
        if state.sort_dir == SortDir::Desc { " ▾" } else { " ▲" }
    } else {
        ""
    };
    Cell::from(format!("{label}{indicator}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn without_filter_mode_returns_full_area_and_no_filter_rect() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, false);
        assert_eq!(table_area, area);
        assert!(filter_area.is_none());
    }

    #[test]
    fn with_filter_mode_splits_top_3_rows_for_filter_bar() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        let filter = filter_area.unwrap();
        assert_eq!(filter.y,          0);
        assert_eq!(filter.height,     3);
        assert_eq!(table_area.y,      3);
        assert_eq!(table_area.height, 37);
    }

    #[test]
    fn filter_and_table_rects_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let (table_area, filter_area) = build_layout(area, true);
        assert_eq!(table_area.width,          120);
        assert_eq!(filter_area.unwrap().width, 120);
    }
}
