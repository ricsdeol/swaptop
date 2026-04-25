use std::time::Duration;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::actions::OpStatus;
use crate::app::AppState;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let keys: &[(&str, &str)] = &[
        ("q", "quit"),
        ("Tab", "next tab"),
        ("1-4", "switch tab"),
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
                Span::styled(format!(" {desc}  "), Style::default().fg(Color::DarkGray)),
            ]
        })
        .collect();

    if state.collect_in_progress {
        spans.push(Span::styled(" ⟳ ", Style::default().fg(Color::Yellow)));
    } else if state.last_collect_completed.elapsed() >= Duration::from_secs(3) {
        spans.push(Span::styled(
            " ⚠ stale ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    if let Some(op) = &state.device_op
        && op.status == OpStatus::Running
        && let Some(started) = state.device_op_started
    {
        let elapsed = started.elapsed().as_secs();
        let op_label = match &op.kind {
            crate::actions::DeviceOpKind::On => "swapon",
            crate::actions::DeviceOpKind::Off => "swapoff",
            crate::actions::DeviceOpKind::OffAndDelete => "swapoff+rm",
            crate::actions::DeviceOpKind::DeleteOnly => "rm",
            crate::actions::DeviceOpKind::Reset => "reset",
        };
        spans.push(Span::styled(
            format!(" {op_label} ({elapsed}s) "),
            Style::default().fg(Color::Yellow),
        ));
    }

    if let Some((err, _)) = &state.error_msg {
        spans.push(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
