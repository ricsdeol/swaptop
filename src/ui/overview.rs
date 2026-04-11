use human_bytes::human_bytes;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, Paragraph},
};

use crate::app::AppState;

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

// ── Gauges ────────────────────────────────────────────────────────────────────

fn render_gauges(f: &mut Frame, area: Rect, state: &AppState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let Some(snap) = &state.current else {
        let p = Paragraph::new(" Waiting for first tick…")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, area);
        return;
    };

    // RAM
    let ram_ratio = (snap.ram.percent as f64 / 100.0).clamp(0.0, 1.0);
    let ram_color = usage_color(snap.ram.percent);
    let ram_label = format!(
        " RAM   {}  /  {}   ({:.0}%)",
        human_bytes(snap.ram.used as f64),
        human_bytes(snap.ram.total as f64),
        snap.ram.percent,
    );
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(ram_color).bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .ratio(ram_ratio)
            .label(Span::styled(ram_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        rows[0],
    );

    // Swap
    let swap_ratio = (snap.swap.percent as f64 / 100.0).clamp(0.0, 1.0);
    let swap_color = usage_color(snap.swap.percent);
    let swap_label = if snap.swap.total == 0 {
        " Swap  no swap configured".to_string()
    } else {
        format!(
            " Swap  {}  /  {}   ({:.0}%)",
            human_bytes(snap.swap.used as f64),
            human_bytes(snap.swap.total as f64),
            snap.swap.percent,
        )
    };
    f.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(swap_color).bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .ratio(swap_ratio)
            .label(Span::styled(swap_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        rows[1],
    );
}

fn usage_color(percent: f32) -> Color {
    if percent < 60.0 {
        Color::Green
    } else if percent < 80.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

// ── Combined history chart ────────────────────────────────────────────────────

fn render_chart(f: &mut Frame, area: Rect, state: &AppState) {
    let start = state.start_time;
    let now_secs = start.elapsed().as_secs_f64();

    let ram_total = state.current.as_ref().map(|s| s.ram.total).unwrap_or(1).max(1);
    let swap_total = state.current.as_ref().map(|s| s.swap.total).unwrap_or(1).max(1);

    // Convert history to percentage points for a common 0-100 Y axis
    let ram_data: Vec<(f64, f64)> = state
        .ram_history
        .iter()
        .map(|(t, bytes)| {
            let x = t.duration_since(start).as_secs_f64();
            let y = (*bytes as f64 / ram_total as f64 * 100.0).clamp(0.0, 100.0);
            (x, y)
        })
        .collect();

    let swap_data: Vec<(f64, f64)> = state
        .swap_history
        .iter()
        .map(|(t, bytes)| {
            let x = t.duration_since(start).as_secs_f64();
            let y = (*bytes as f64 / swap_total as f64 * 100.0).clamp(0.0, 100.0);
            (x, y)
        })
        .collect();

    // Show a rolling 120-second window
    let window = 120.0_f64;
    let x_max = now_secs.max(window);
    let x_min = (x_max - window).max(0.0);

    let datasets = vec![
        Dataset::default()
            .name("RAM")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&ram_data),
        Dataset::default()
            .name("Swap")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Yellow))
            .data(&swap_data),
    ];

    let elapsed = now_secs as u64;
    let window_label = if elapsed < window as u64 {
        format!("-{elapsed}s")
    } else {
        format!("-{:.0}s", window)
    };

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("Memory History", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled("━━", Style::default().fg(Color::Cyan)),
                    Span::styled(" RAM  ", Style::default().fg(Color::Cyan)),
                    Span::styled("━━", Style::default().fg(Color::Yellow)),
                    Span::styled(" Swap ", Style::default().fg(Color::Yellow)),
                ]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::styled(window_label, Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, 100.0])
                .labels(vec![
                    Span::styled("  0%", Style::default().fg(Color::DarkGray)),
                    Span::styled(" 50%", Style::default().fg(Color::DarkGray)),
                    Span::styled("100%", Style::default().fg(Color::DarkGray)),
                ]),
        );

    f.render_widget(chart, area);
}

// ── Device summary line ───────────────────────────────────────────────────────

fn render_device_summary(f: &mut Frame, area: Rect, state: &AppState) {
    let (count, total_bytes) = state
        .current
        .as_ref()
        .map(|s| {
            let n: usize = s.devices.len();
            let t: u64 = s.devices.iter().map(|d| d.total).sum();
            (n, t)
        })
        .unwrap_or((0, 0));

    let uptime = state.start_time.elapsed().as_secs();
    let (h, m, s) = (uptime / 3600, (uptime % 3600) / 60, uptime % 60);

    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("Devices active: ", Style::default().fg(Color::DarkGray)),
        Span::styled(count.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("   Total swap: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            human_bytes(total_bytes as f64),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled("   Uptime: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{h:02}:{m:02}:{s:02}"),
            Style::default().fg(Color::Cyan),
        ),
    ]);

    f.render_widget(Paragraph::new(line), area);
}
