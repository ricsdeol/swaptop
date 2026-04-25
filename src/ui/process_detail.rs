use human_bytes::human_bytes;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Axis, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph},
};

use crate::app::AppState;

pub fn render(frame: &mut ratatui::Frame, state: &AppState) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area); // clear background

    let block = Block::default()
        .title(" Process Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let layout = build_layout(inner);

    let pid = state.selected_process_detail.unwrap_or(0);

    // Metadata
    render_metadata(frame, layout[0], state, pid);

    // Charts
    if layout[1].height >= 5 {
        render_charts(frame, layout[1], state, pid);
    }

    // Footer
    render_footer(frame, layout[2], state, pid);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // metadata
            Constraint::Min(5),    // charts
            Constraint::Length(3), // footer
        ])
        .split(area)
}

fn render_metadata(frame: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let process_row = find_process(state, pid);
    let (name, user, threads, status, exe_path) = if let Some(process) = process_row {
        (
            process.name.clone(),
            process.user.clone(),
            process.threads,
            process.status,
            process.exe_path.clone(),
        )
    } else {
        ("(process ended)".into(), "?".into(), 0, '?', None)
    };

    let status_desc = match status {
        'R' => "running",
        'S' => "sleeping",
        'D' => "disk sleep",
        'T' => "stopped",
        'Z' => "zombie",
        _ => "unknown",
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("PID: {pid} "),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("| Name: {name} "),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("User: {user} "), Style::default().fg(Color::White)),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("Threads: {threads} "),
                Style::default().fg(Color::White),
            ),
            Span::styled("| ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("Status: {status} ({status_desc})"),
                Style::default().fg(Color::White),
            ),
        ]),
        if let Some(ref exe) = exe_path {
            Line::from(vec![
                Span::styled("Exec: ", Style::default().fg(Color::DarkGray)),
                Span::styled(exe.clone(), Style::default().fg(Color::White)),
            ])
        } else {
            Line::from("")
        },
    ];

    let paragraph = Paragraph::new(Text::from(lines));
    frame.render_widget(paragraph, area);
}

fn render_charts(frame: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .spacing(2)
        .split(area);

    let start = state.start_time;
    let history_option = state.process_history.get(&pid);

    // Convert history to chart data points (x = seconds since start, y = bytes)
    let (
        ram_history_points,
        swap_history_points,
        ram_initial_value,
        ram_maximum_value,
        swap_initial_value,
        swap_maximum_value,
    ) = if let Some(history) = history_option {
        let ram_points: Vec<(f64, f64)> = history
            .rss_history
            .iter()
            .map(|(timestamp, value)| {
                (timestamp.duration_since(start).as_secs_f64(), *value as f64)
            })
            .collect();
        let swap_points: Vec<(f64, f64)> = history
            .swap_history
            .iter()
            .map(|(timestamp, value)| {
                (timestamp.duration_since(start).as_secs_f64(), *value as f64)
            })
            .collect();
        let ram_initial = ram_points.first().map(|(_, value)| *value).unwrap_or(0.0);
        let ram_maximum = ram_points
            .iter()
            .map(|(_, value)| *value)
            .fold(1.0, f64::max);
        let swap_initial = swap_points.first().map(|(_, value)| *value).unwrap_or(0.0);
        let swap_maximum = swap_points
            .iter()
            .map(|(_, value)| *value)
            .fold(1.0, f64::max);
        (
            ram_points,
            swap_points,
            ram_initial,
            ram_maximum,
            swap_initial,
            swap_maximum,
        )
    } else {
        (vec![], vec![], 0.0, 1.0, 0.0, 1.0)
    };

    let elapsed_seconds = start.elapsed().as_secs_f64();
    let window_seconds = 900.0_f64; // 15 minutes in seconds
    let x_axis_maximum = elapsed_seconds.max(window_seconds);
    let x_axis_minimum = (x_axis_maximum - window_seconds).max(0.0);

    // Y-axis bounds with the *initial* value centered (add 20% padding around data range)
    let (ram_axis_minimum, ram_axis_maximum) =
        centered_y_bounds(ram_initial_value, ram_maximum_value);
    let (swap_axis_minimum, swap_axis_maximum) =
        centered_y_bounds(swap_initial_value, swap_maximum_value);

    // RAM Chart
    let ram_datasets = vec![
        Dataset::default()
            .name("RAM")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&ram_history_points),
    ];
    let ram_chart = Chart::new(ram_datasets)
        .block(Block::bordered().title(" RAM History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_axis_minimum, x_axis_maximum])
                .labels(vec![
                    Span::styled("-15m", Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([ram_axis_minimum, ram_axis_maximum])
                .labels(vec![
                    Span::styled(
                        human_bytes(ram_axis_minimum),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        human_bytes(ram_initial_value),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        human_bytes(ram_axis_maximum),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
        );
    frame.render_widget(ram_chart, parts[0]);

    // Swap Chart
    let swap_datasets = vec![
        Dataset::default()
            .name("Swap")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&swap_history_points),
    ];
    let swap_chart = Chart::new(swap_datasets)
        .block(Block::bordered().title(" Swap History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_axis_minimum, x_axis_maximum])
                .labels(vec![
                    Span::styled("-15m", Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([swap_axis_minimum, swap_axis_maximum])
                .labels(vec![
                    Span::styled(
                        human_bytes(swap_axis_minimum),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        human_bytes(swap_initial_value),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        human_bytes(swap_axis_maximum),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
        );
    frame.render_widget(swap_chart, parts[1]);
}

/// Compute Y-axis bounds that place the given center value in the vertical middle.
/// Adds 20% padding around the [0, max] data range.  If the center value is
/// near zero the lower bound is clamped to 0 so the axis never go negative.
fn centered_y_bounds(center: f64, max_value: f64) -> (f64, f64) {
    let range = max_value.max(center * 2.0) * 1.2;
    let half = range / 2.0;
    let y_min = (center - half).max(0.0);
    let y_max = center + half;
    (y_min, y_max)
}

fn render_footer(frame: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let process_row = find_process(state, pid);
    let (rss, swap) = if let Some(process) = process_row {
        (process.rss, process.swap)
    } else {
        (0, 0)
    };

    let lines = if state.process_detail_confirm_kill {
        vec![Line::from(vec![
            Span::styled(
                format!(" Kill PID {pid}? "),
                Style::default().fg(Color::Red),
            ),
            Span::styled(
                "[y]",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("es / ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[n]",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("o / ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "[Esc]",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
        ])]
    } else {
        vec![
            Line::from(vec![
                Span::styled(" Current: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("RSS {} ", human_bytes(rss as f64)),
                    Style::default().fg(Color::White),
                ),
                Span::styled("| ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("Swap {}", human_bytes(swap as f64)),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    " [k]",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" kill  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "[Esc/q]",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" back", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(""), // bottom margin
        ]
    };

    let paragraph = Paragraph::new(Text::from(lines));
    frame.render_widget(paragraph, area);
}

fn find_process(state: &AppState, pid: u32) -> Option<&crate::platform::ProcessRow> {
    state.processes.iter().find(|process| process.pid == pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn centered_rect_produces_reasonable_dimensions() {
        let area = Rect::new(0, 0, 100, 40);
        let popup = centered_rect(70, 70, area);
        assert!(popup.width > 0);
        assert!(popup.height > 0);
        assert!(popup.width <= area.width);
        assert!(popup.height <= area.height);
    }

    #[test]
    fn build_layout_splits_into_three_sections() {
        let area = Rect::new(0, 0, 80, 24);
        let layout = build_layout(area);
        assert_eq!(layout.len(), 3);
    }
}
