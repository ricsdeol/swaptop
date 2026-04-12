mod design;
mod devices;
mod overview;
mod statusbar;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    prelude::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Tabs},
};

use crate::app::{AppState, Tab};

pub fn render(f: &mut Frame, state: &AppState) {
    let layout = build_layout(f.area());

    render_tabbar(f, layout[0], state);

    match state.active_tab {
        Tab::Overview => overview::render(f, layout[1], state),
        Tab::Devices  => devices::render(f, layout[1], state),
        _             => render_coming_soon(f, layout[1]),
    }

    statusbar::render(f, layout[2], state);
}

fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // [0] tab bar
            Constraint::Min(0),    // [1] content
            Constraint::Length(1), // [2] status bar
        ])
        .spacing(design::OUTER_GAP)
        .split(area)
}

fn render_tabbar(f: &mut Frame, area: Rect, state: &AppState) {
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

fn render_coming_soon(f: &mut Frame, area: Rect) {
    let p = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled("Coming in a future phase…", Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    use crate::ui::design::{INNER_GAP, OUTER_GAP};

    fn build_layout(area: Rect) -> std::rc::Rc<[Rect]> {
        super::build_layout(area)
    }

    #[test]
    fn tabbar_starts_at_top_and_has_correct_height() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[0].y,      0);
        assert_eq!(layout[0].height, 3);
    }

    #[test]
    fn content_starts_after_tabbar_plus_outer_gap() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[1].y, 3 + OUTER_GAP);
    }

    #[test]
    fn statusbar_is_last_row() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        assert_eq!(layout[2].y,      area.height - 1);
        assert_eq!(layout[2].height, 1);
    }

    #[test]
    fn sections_span_full_width() {
        let area = Rect::new(0, 0, 120, 40);
        let layout = build_layout(area);
        for rect in layout.iter() {
            assert_eq!(rect.x,     0);
            assert_eq!(rect.width, 120);
        }
    }

    #[test]
    fn layout_is_stable_on_minimal_terminal() {
        // minimum rows: 3 (tabbar) + OUTER_GAP(2) + 0 (content) + OUTER_GAP(2) + 1 (statusbar) = 8
        let area = Rect::new(0, 0, 40, 8);
        let layout = build_layout(area);
        assert_eq!(layout[1].height, 0);
    }

    #[test]
    fn inner_gap_accessible_and_half_of_outer() {
        assert_eq!(INNER_GAP * 2, OUTER_GAP);
    }

    #[test]
    fn spacing_produces_correct_positions() {
        let area = Rect::new(0, 0, 120, 40);
        let with_spacing = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .spacing(OUTER_GAP)
            .split(area);
        assert_eq!(with_spacing[0].y, 0);
        assert_eq!(with_spacing[1].y, 3 + OUTER_GAP);
        assert_eq!(with_spacing[2].y, area.height - 1);
    }
}
