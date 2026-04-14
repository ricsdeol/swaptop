use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
};

use crate::actions::{DeviceOpKind, OpStatus};
use crate::app::AppState;
use crate::ui::design;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let layout = build_layout(area);
    render_header(f, layout[0]);
    render_table(f, layout[1], state);
    render_footer(f, layout[2], state);

    if state.confirm_action.is_some() {
        render_modal(f, area, state);
    }

    if state.create_swap_modal.is_some() {
        crate::ui::create_swap::render(f, area, state);
    }
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // [0] header row
            Constraint::Min(0),    // [1] device list
            Constraint::Length(2), // [2] footer hints
        ])
        .spacing(design::INNER_GAP)
        .split(area)
}

fn render_header(f: &mut Frame, area: Rect) {
    let header = Row::new(vec![
        Cell::from("Path").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Type").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Total").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Used").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("%").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Pri").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Cell::from("Status").style(
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let t = Table::new(vec![header], column_widths()).block(Block::default());
    f.render_widget(t, area);
}

fn render_table(f: &mut Frame, area: Rect, state: &AppState) {
    let rows: Vec<Row> = state
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let status_cell = status_cell(dev, state);
            let percent = if dev.total > 0 {
                dev.used as f64 / dev.total as f64 * 100.0
            } else {
                0.0
            };

            let row = Row::new(vec![
                Cell::from(dev.path.to_string_lossy().to_string()),
                Cell::from(dev.kind.to_string()),
                Cell::from(human_bytes::human_bytes(dev.total as f64)),
                Cell::from(human_bytes::human_bytes(dev.used as f64)),
                Cell::from(format!("{percent:.0}%")),
                Cell::from(format!("{}", dev.priority)),
                status_cell,
            ]);

            if i == state.selected_dev {
                row.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                row
            }
        })
        .collect();

    let table = Table::new(rows, column_widths())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Swap Devices ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut table_state = TableState::default().with_selected(Some(state.selected_dev));
    f.render_stateful_widget(table, area, &mut table_state);
}

fn status_cell<'a>(dev: &crate::platform::SwapDevice, state: &AppState) -> Cell<'a> {
    if let Some(op) = state.device_op.as_ref().filter(|op| op.path == dev.path) {
        return match &op.status {
            OpStatus::Running => Cell::from("⏳ ...").style(Style::default().fg(Color::Yellow)),
            OpStatus::Done => Cell::from("✓ OK").style(Style::default().fg(Color::Green)),
            OpStatus::Error(_) => Cell::from("✗ ERROR").style(Style::default().fg(Color::Red)),
        };
    }
    if dev.active {
        Cell::from("ACTIVE").style(Style::default().fg(Color::Green))
    } else {
        Cell::from("INACTIVE").style(Style::default().fg(Color::DarkGray))
    }
}

fn column_widths() -> Vec<Constraint> {
    vec![
        Constraint::Min(20),    // Path
        Constraint::Length(10), // Type
        Constraint::Length(9),  // Total
        Constraint::Length(9),  // Used
        Constraint::Length(5),  // %
        Constraint::Length(5),  // Pri
        Constraint::Length(10), // Status
    ]
}

fn render_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let hint_line = Line::from(vec![
        key_span("o"),
        desc_span(" activate  "),
        key_span("f"),
        desc_span(" deactivate  "),
        key_span("r"),
        desc_span(" reset  "),
        key_span("n"),
        desc_span(" new swap  "),
        key_span("j/k"),
        desc_span(" navigate"),
    ]);

    let warning_line = if !state.capabilities.can_swap_on {
        Line::from(Span::styled(
            "  Managed by dynamic_pager — control unavailable",
            Style::default().fg(Color::Yellow),
        ))
    } else {
        Line::from("")
    };

    f.render_widget(Paragraph::new(vec![hint_line, warning_line]), area);
}

fn key_span(k: &str) -> Span<'static> {
    Span::styled(
        format!(" {k} "),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn desc_span(d: &str) -> Span<'static> {
    Span::styled(d.to_string(), Style::default().fg(Color::DarkGray))
}

fn render_modal(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(kind) = &state.confirm_action else {
        return;
    };

    let modal_width = (area.width * 60 / 100).max(40);
    let modal_height = 7u16;
    let modal_x = area.x + (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = area.y + (area.height.saturating_sub(modal_height)) / 2;
    let modal_rect = Rect::new(modal_x, modal_y, modal_width, modal_height);

    let op_label = match kind {
        DeviceOpKind::On => "Activate",
        DeviceOpKind::Off => "Deactivate",
        DeviceOpKind::Reset => "Reset",
    };

    let dev_path = state
        .devices
        .get(state.selected_dev)
        .map(|d| d.path.to_string_lossy().to_string())
        .unwrap_or_default();

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {op_label} {dev_path}?"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            key_span("s"),
            desc_span(" confirm    "),
            key_span("Esc"),
            desc_span(" cancel"),
        ]),
        Line::from(""),
    ];

    f.render_widget(Clear, modal_rect);
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(Span::styled(
                    " Confirm ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        modal_rect,
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::build_layout;
    use crate::ui::design::INNER_GAP;
    use ratatui::layout::Rect;

    #[test]
    fn header_row_starts_at_top_and_is_one_line() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[0].y, 0);
        assert_eq!(layout[0].height, 1);
    }

    #[test]
    fn footer_is_two_lines_at_bottom() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[2].height, 2);
        assert_eq!(layout[2].y, area.height - 2);
    }

    #[test]
    fn all_sections_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        for rect in layout.iter() {
            assert_eq!(rect.x, 0);
            assert_eq!(rect.width, 120);
        }
    }

    #[test]
    fn layout_stable_on_minimal_terminal() {
        // minimum: 1 (header) + INNER_GAP(1) + 0 (list Min) + INNER_GAP(1) + 2 (footer) = 5
        let area = Rect::new(0, 0, 40, 5);
        let layout = build_layout(area);
        assert_eq!(layout[1].height, 0);
        // suppress unused import lint for INNER_GAP used in the comment assertion above
        let _ = INNER_GAP;
    }
}
