mod overview;
mod statusbar;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::app::{AppState, Tab};

pub fn render(f: &mut Frame, state: &AppState) {
    // 15-cell outer margin: tabbar, content, and statusbar all inset from terminal edges
    let area = f.area().inner(Margin { horizontal: 15, vertical: 0 });

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(1), // status bar
        ])
        .split(area);

    render_tabbar(f, layout[0], state);

    // 10-cell additional margin for the active tab content area
    let content_area = layout[1].inner(Margin { horizontal: 10, vertical: 0 });

    match state.active_tab {
        Tab::Overview => overview::render(f, content_area, state),
        _ => render_coming_soon(f, content_area),
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

#[cfg(test)]
mod tests {
    use ratatui::layout::{Margin, Rect};

    #[test]
    fn outer_margin_shrinks_width_by_twice_margin() {
        let full = Rect::new(0, 0, 120, 40);
        let inner = full.inner(Margin { horizontal: 15, vertical: 0 });
        assert_eq!(inner.x,      15);
        assert_eq!(inner.width,  90);   // 120 - 15*2
        assert_eq!(inner.y,      0);
        assert_eq!(inner.height, 40);   // unchanged
    }

    #[test]
    fn outer_margin_clamps_on_narrow_terminal() {
        let narrow = Rect::new(0, 0, 20, 40);
        let inner  = narrow.inner(Margin { horizontal: 15, vertical: 0 });
        assert_eq!(inner.width, 0);
    }

    #[test]
    fn content_margin_further_shrinks_content_area() {
        let after_outer = Rect::new(15, 0, 90, 36);
        let content = after_outer.inner(Margin { horizontal: 10, vertical: 0 });
        assert_eq!(content.x,      25);  // 15 + 10
        assert_eq!(content.width,  70);  // 90 - 10*2
        assert_eq!(content.y,      0);
        assert_eq!(content.height, 36);
    }

    #[test]
    fn content_margin_clamps_on_narrow_content_area() {
        let narrow_content = Rect::new(15, 0, 10, 36);
        let content = narrow_content.inner(Margin { horizontal: 10, vertical: 0 });
        assert_eq!(content.width, 0);
    }
}
