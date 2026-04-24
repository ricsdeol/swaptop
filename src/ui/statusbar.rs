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

    if let Some((err, _)) = &state.error_msg {
        spans.push(Span::styled(
            format!("  ⚠ {err}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}
