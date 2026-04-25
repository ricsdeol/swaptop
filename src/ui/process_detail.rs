use human_bytes::human_bytes;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Axis, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph},
};

use crate::app::AppState;

pub fn render(f: &mut ratatui::Frame, state: &AppState) {
    let area = centered_rect(70, 70, f.area());
    f.render_widget(Clear, area); // clear background

    let block = Block::default()
        .title(" Process Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let layout = build_layout(inner);

    let pid = state.selected_process_detail.unwrap_or(0);

    // Metadata
    render_metadata(f, layout[0], state, pid);

    // Charts
    if layout[1].height >= 5 {
        render_charts(f, layout[1], state, pid);
    }

    // Footer
    render_footer(f, layout[2], state, pid);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

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

fn render_metadata(f: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let proc = find_process(state, pid);
    let (name, user, threads, status, exe_path) = if let Some(p) = proc {
        (
            p.name.clone(),
            p.user.clone(),
            p.threads,
            p.status,
            p.exe_path.clone(),
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

    let p = Paragraph::new(Text::from(lines));
    f.render_widget(p, area);
}

fn render_charts(f: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let parts = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Each chart has Block::bordered() which uses 2 columns (left+right borders)
    let chart_width = (parts[0].width as usize).saturating_sub(2).max(1);

    let start = state.start_time;
    let now_secs = start.elapsed().as_secs_f64();
    let hist = state.process_history.get(&pid);

    // Slice only the last N points that fit visually (1 point ≈ 1 column)
    let (ram_data, swap_data, ram_max, swap_max, visible_points) = if let Some(h) = hist {
        let take = chart_width.min(h.rss_history.len());
        let ram: Vec<(f64, f64)> = h
            .rss_history
            .iter()
            .skip(h.rss_history.len().saturating_sub(take))
            .map(|(t, v)| (t.duration_since(start).as_secs_f64(), *v as f64))
            .collect();
        let swap: Vec<(f64, f64)> = h
            .swap_history
            .iter()
            .skip(h.swap_history.len().saturating_sub(take))
            .map(|(t, v)| (t.duration_since(start).as_secs_f64(), *v as f64))
            .collect();
        let ram_max = ram.iter().map(|(_, y)| *y).fold(1.0, f64::max);
        let swap_max = swap.iter().map(|(_, y)| *y).fold(1.0, f64::max);
        (ram, swap, ram_max, swap_max, take)
    } else {
        (vec![], vec![], 1.0, 1.0, 0)
    };

    // Dynamic window based on how many points actually fit on screen
    let window_secs = visible_points as f64;
    let x_max = now_secs;
    let x_min = (x_max - window_secs).max(0.0);

    let left_label = format_time_label(window_secs);

    // RAM Chart
    let ram_datasets = vec![
        Dataset::default()
            .name("RAM")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&ram_data),
    ];
    let ram_chart = Chart::new(ram_datasets)
        .block(Block::bordered().title(" RAM History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::styled(left_label.clone(), Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, ram_max])
                .labels(vec![
                    Span::styled(human_bytes(0.0), Style::default().fg(Color::DarkGray)),
                    Span::styled(human_bytes(ram_max), Style::default().fg(Color::DarkGray)),
                ]),
        );
    f.render_widget(ram_chart, parts[0]);

    // Swap Chart
    let swap_datasets = vec![
        Dataset::default()
            .name("Swap")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Magenta))
            .data(&swap_data),
    ];
    let swap_chart = Chart::new(swap_datasets)
        .block(Block::bordered().title(" Swap History "))
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::styled(left_label, Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, swap_max])
                .labels(vec![
                    Span::styled(human_bytes(0.0), Style::default().fg(Color::DarkGray)),
                    Span::styled(human_bytes(swap_max), Style::default().fg(Color::DarkGray)),
                ]),
        );
    f.render_widget(swap_chart, parts[1]);

    // Short history message overlay
    if ram_data.len() < 10 {
        let msg = Paragraph::new(format!(
            "Collecting history... ({}/{})",
            ram_data.len(),
            chart_width
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Yellow));
        f.render_widget(msg, parts[0]);
    }
}

/// Format a duration in seconds into a compact human label.
fn format_time_label(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("-{:.0}s", seconds)
    } else if seconds < 3600.0 {
        format!("-{:.0}m", seconds / 60.0)
    } else {
        format!("-{:.0}h", seconds / 3600.0)
    }
}

fn render_footer(f: &mut ratatui::Frame, area: Rect, state: &AppState, pid: u32) {
    let proc = find_process(state, pid);
    let (rss, swap) = if let Some(p) = proc {
        (p.rss, p.swap)
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
        ]
    };

    let p = Paragraph::new(Text::from(lines));
    f.render_widget(p, area);
}

fn find_process(state: &AppState, pid: u32) -> Option<&crate::platform::ProcessRow> {
    state.processes.iter().find(|p| p.pid == pid)
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

    #[test]
    fn format_time_label_seconds() {
        assert_eq!(format_time_label(45.0), "-45s");
    }

    #[test]
    fn format_time_label_minutes() {
        assert_eq!(format_time_label(120.0), "-2m");
    }

    #[test]
    fn format_time_label_hours() {
        assert_eq!(format_time_label(7200.0), "-2h");
    }
}
