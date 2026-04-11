mod overview;
mod statusbar;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::app::{AppState, Tab};

pub fn render(f: &mut Frame, state: &AppState) {
    let area = f.area();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_tabbar(f, layout[0], state);

    match state.active_tab {
        Tab::Overview => overview::render(f, layout[1], state),
        _ => render_coming_soon(f, layout[1]),
    }

    statusbar::render(f, layout[2], state);
}

fn render_tabbar(f: &mut Frame, area: ratatui::layout::Rect, state: &AppState) {
    let titles = vec![
        Line::from(vec![
            Span::styled("1", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Overview", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("2", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Processes", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("3", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Devices", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("4", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(":Create Swap", Style::default().fg(Color::White)),
        ]),
    ];

    let selected = match state.active_tab {
        Tab::Overview   => 0,
        Tab::Processes  => 1,
        Tab::Devices    => 2,
        Tab::CreateSwap => 3,
    };

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title(Span::styled(
                    " swaptop ",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .select(selected)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .divider(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));

    f.render_widget(tabs, area);
}

fn render_coming_soon(f: &mut Frame, area: ratatui::layout::Rect) {
    let p = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("Coming in a future phase…", Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(p, area);
}
