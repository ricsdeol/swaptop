use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{AppState, Tab};

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let keys: &[(&str, &str)] = if state.filter_mode {
        &[
            ("Enter/Esc", "exit filter"),
            ("Backspace", "delete char"),
        ]
    } else if state.active_tab == Tab::Processes {
        &[
            ("j/k", "navigate"),
            ("s", "sort"),
            ("/", "filter"),
            ("Tab", "next tab"),
            ("q", "quit"),
        ]
    } else {
        &[
            ("q", "quit"),
            ("Tab", "next tab"),
            ("1-4", "switch tab"),
            ("r", "refresh"),
        ]
    };

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
