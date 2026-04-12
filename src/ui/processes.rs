use human_bytes::human_bytes;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};

use crate::actions::{SortColumn, SortDir};
use crate::app::AppState;
use crate::platform::ProcessRow;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area, state.filter_mode);

    if state.filter_mode {
        render_filter_bar(f, layout[0], state);
    }

    let table_area = layout[1];
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
    render_footer(f, layout[2], state);
}

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

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hint_line = if state.filter_mode {
        Line::from(vec![
            key_span("Enter/Esc"),
            desc_span(" exit filter  "),
            key_span("Backspace"),
            desc_span(" delete char"),
        ])
    } else {
        Line::from(vec![
            key_span("j/k"),
            desc_span(" navigate  "),
            key_span("s"),
            desc_span(" sort  "),
            key_span("/"),
            desc_span(" filter"),
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
        state
            .processes
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&lower))
            .collect()
    };

    if visible.is_empty() {
        let p = Paragraph::new("  No processes found").style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    }

    let header = Row::new(vec![
        header_cell("PID", &SortColumn::Pid, state),
        header_cell("Name", &SortColumn::Name, state),
        header_cell("User", &SortColumn::User, state),
        header_cell("RSS", &SortColumn::Rss, state),
        header_cell("Swap", &SortColumn::Swap, state),
        header_cell("CPU%", &SortColumn::Cpu, state),
    ])
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .height(1);

    let rows: Vec<Row> = visible
        .iter()
        .map(|p| {
            let swap_cell = if has_per_process {
                Cell::from(format!("{:>10}", human_bytes(p.swap as f64)))
            } else {
                Cell::from(format!("{:>10}", "—")).style(Style::default().fg(Color::DarkGray))
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
    let clamped = state.selected_row.min(visible.len() - 1);
    table_state.select(Some(clamped));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn header_cell<'a>(label: &'a str, col: &SortColumn, state: &AppState) -> Cell<'a> {
    let indicator = if col == &state.sort_col {
        if state.sort_dir == SortDir::Desc {
            " ▾"
        } else {
            " ▲"
        }
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
    fn without_filter_mode_footer_is_1_line_and_filter_slot_is_zero() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area, false);
        assert_eq!(layout[0].height, 0); // filter bar hidden
        assert_eq!(layout[1].height, 39); // table fills rest
        assert_eq!(layout[2].height, 1); // footer
        assert_eq!(layout[2].y, 39);
    }

    #[test]
    fn with_filter_mode_filter_is_3_table_shrinks_footer_is_1() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area, true);
        assert_eq!(layout[0].height, 3); // filter bar
        assert_eq!(layout[0].y, 0);
        assert_eq!(layout[1].height, 36); // table
        assert_eq!(layout[2].height, 1); // footer
        assert_eq!(layout[2].y, 39);
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
