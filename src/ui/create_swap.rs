use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::AppState;
use crate::create_swap::{CreateSwapField, CreateSwapModal, CreateSwapMode};
use crate::platform::StepStatus;

pub fn render(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(modal) = state.create_swap_modal.as_ref() else {
        return;
    };

    let modal_rect = centered_rect(area, 64, 18);
    f.render_widget(Clear, modal_rect);

    match &modal.mode {
        CreateSwapMode::Form { focused_field } => {
            render_form(f, modal_rect, modal, *focused_field);
        }
        CreateSwapMode::Progress { steps } => {
            render_progress(f, modal_rect, modal, steps);
        }
        CreateSwapMode::ConfirmActivateOnly { path, size_bytes } => {
            render_confirm_activate(f, modal_rect, path, *size_bytes);
        }
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

fn render_form(f: &mut Frame, area: Rect, modal: &CreateSwapModal, focused: CreateSwapField) {
    let block = Block::default()
        .title(Span::styled(
            " New Swap File ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Path
            Constraint::Length(1), // Size + unit
            Constraint::Length(1), // Priority
            Constraint::Length(1), // Activate
            Constraint::Length(1), // spacer
            Constraint::Length(1), // Submit
            Constraint::Length(1), // spacer
            Constraint::Length(1), // validation error or hint
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(field_line(
            "Path:    ",
            modal.path_input.value(),
            focused == CreateSwapField::Path,
        )),
        rows[0],
    );
    let size_line = Line::from(vec![
        label_span("Size:    "),
        Span::raw(" "),
        value_span(modal.size_input.value(), focused == CreateSwapField::Size),
        Span::raw(" "),
        unit_span(
            modal.size_unit.label(),
            focused == CreateSwapField::SizeUnit,
        ),
    ]);
    f.render_widget(Paragraph::new(size_line), rows[1]);
    f.render_widget(
        Paragraph::new(field_line(
            "Priority:",
            modal.priority_input.value(),
            focused == CreateSwapField::Priority,
        )),
        rows[2],
    );

    let checkbox = if modal.activate_after { "[x]" } else { "[ ]" };
    let activate_line = Line::from(vec![
        label_span("Activate:"),
        Span::raw(" "),
        Span::styled(
            format!("{checkbox} activate after create"),
            if focused == CreateSwapField::ActivateAfter {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        ),
    ]);
    f.render_widget(Paragraph::new(activate_line), rows[3]);

    let submit_label = if focused == CreateSwapField::Submit {
        Span::styled(
            " ▶ [  Create  ] ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("   [  Create  ] ", Style::default().fg(Color::White))
    };
    f.render_widget(Paragraph::new(Line::from(submit_label)), rows[5]);

    let hint_or_error = if let Some(err) = &modal.validation_error {
        Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::from(Span::styled(
            "  ↑/↓ navigate · Space toggle · Enter submit · Esc cancel",
            Style::default().fg(Color::DarkGray),
        ))
    };
    f.render_widget(Paragraph::new(hint_or_error), rows[7]);

    // Place the real terminal cursor on the focused text field.
    let cursor_input: Option<(&tui_input::Input, u16)> = match focused {
        CreateSwapField::Path => Some((&modal.path_input, rows[0].y)),
        CreateSwapField::Size => Some((&modal.size_input, rows[1].y)),
        CreateSwapField::Priority => Some((&modal.priority_input, rows[2].y)),
        _ => None,
    };
    if let Some((input, row_y)) = cursor_input {
        // label is 9 chars + 1 space = 10, then "[" bracket = 1, then visual cursor offset
        let label_width = 9_u16 + 1; // label + space
        let bracket = 1_u16; // opening "["
        // Clamp to the visible field width (30 chars) so the cursor stays inside the "[]" span.
        let vis_col = cursor_visual_col(input.value(), input.cursor()).min(30);
        let cursor_x = inner.x + label_width + bracket + vis_col;
        f.set_cursor_position((cursor_x, row_y));
    }

    // Render autocomplete popup if completions are showing.
    if !modal.completions.is_empty() {
        // Anchor = the Path value span area (row 0), offset for label+space+bracket
        let popup_anchor = Rect::new(inner.x + 10, rows[0].y, 32, 1);
        render_completions_popup(f, popup_anchor, &modal.completions, modal.completion_sel);
    }
}

fn field_line<'a>(label: &'a str, value: &'a str, focused: bool) -> Line<'a> {
    Line::from(vec![
        label_span(label),
        Span::raw(" "),
        value_span(value, focused),
    ])
}

fn label_span(s: &str) -> Span<'_> {
    Span::styled(s.to_string(), Style::default().fg(Color::DarkGray))
}

fn value_span<'a>(s: &'a str, focused: bool) -> Span<'a> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    let padded = format!("[{:<30}]", s);
    Span::styled(padded, style)
}

fn unit_span(s: &str, focused: bool) -> Span<'_> {
    let style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    Span::styled(format!(" {s} "), style)
}

fn render_progress(
    f: &mut Frame,
    area: Rect,
    modal: &CreateSwapModal,
    steps: &[crate::create_swap::CreateSwapStep],
) {
    let title = format!(" Creating {} ", modal.path_input.value());
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = steps
        .iter()
        .map(|s| {
            let (icon, color) = match &s.status {
                StepStatus::Pending => ("·", Color::DarkGray),
                StepStatus::Running => ("⏳", Color::Yellow),
                StepStatus::Done => ("✓", Color::Green),
                StepStatus::Error(_) => ("✗", Color::Red),
            };
            let mut spans = vec![
                Span::raw("  "),
                Span::styled(icon.to_string(), Style::default().fg(color)),
                Span::raw("  "),
                Span::styled(s.label.clone(), Style::default().fg(Color::White)),
            ];
            if let StepStatus::Error(msg) = &s.status {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(msg.clone(), Style::default().fg(Color::Red)));
            }
            Line::from(spans)
        })
        .collect();

    let has_error = steps
        .iter()
        .any(|s| matches!(s.status, StepStatus::Error(_)));
    let footer = if has_error {
        Line::from(Span::styled(
            "  Esc return to form",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(Span::styled(
            "  Esc cancel (before file write)",
            Style::default().fg(Color::DarkGray),
        ))
    };

    let mut full = lines;
    full.push(Line::from(""));
    full.push(footer);

    f.render_widget(Paragraph::new(full), inner);
}

fn render_confirm_activate(f: &mut Frame, area: Rect, path: &std::path::Path, size_bytes: u64) {
    let block = Block::default()
        .title(Span::styled(
            " Already a swap file ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let pretty_size = human_bytes::human_bytes(size_bytes as f64);
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", path.display()),
            Style::default().fg(Color::White),
        )),
        Line::from(Span::styled(
            format!("  already contains a {pretty_size} swap area."),
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " c ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" activate    ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                " Esc ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
        ]),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

fn render_completions_popup(
    f: &mut Frame,
    anchor: Rect,
    completions: &[String],
    sel: Option<usize>,
) {
    if completions.is_empty() {
        return;
    }
    let visible = completions.len().min(6);
    let popup_width = 32_u16; // matches value span width
    let popup_height = visible as u16 + 2; // +2 for border
    let popup_x = anchor.x;
    let popup_y = anchor.y + 1; // directly below the path row
    let popup_rect = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let items: Vec<Line> = completions
        .iter()
        .take(visible)
        .enumerate()
        .map(|(i, path)| {
            let style = if Some(i) == sel {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            // Truncate to fit within popup width minus borders.
            // Use char-boundary-safe slicing to handle non-ASCII paths.
            let max_chars = (popup_width - 2) as usize;
            let char_count = path.chars().count();
            let display: String = if char_count > max_chars {
                let tail: String = path.chars().skip(char_count - (max_chars - 2)).collect();
                format!("..{tail}")
            } else {
                path.clone()
            };
            Line::styled(display, style)
        })
        .collect();

    f.render_widget(Clear, popup_rect);
    f.render_widget(Paragraph::new(items).block(block), popup_rect);
}

fn cursor_visual_col(value: &str, byte_cursor: usize) -> u16 {
    value[..byte_cursor].chars().count() as u16
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn cursor_visual_col_ascii() {
        assert_eq!(cursor_visual_col("/var/swap", 4), 4);
        assert_eq!(cursor_visual_col("/var/swap", 0), 0);
        assert_eq!(cursor_visual_col("/var/swap", 9), 9);
    }

    #[test]
    fn cursor_visual_col_empty() {
        assert_eq!(cursor_visual_col("", 0), 0);
    }

    #[test]
    fn centered_rect_stays_within_bounds() {
        let area = Rect::new(0, 0, 100, 40);
        let r = centered_rect(area, 60, 20);
        assert_eq!(r.width, 60);
        assert_eq!(r.height, 20);
        assert!(r.x + r.width <= area.x + area.width);
        assert!(r.y + r.height <= area.y + area.height);
    }

    #[test]
    fn centered_rect_clamps_when_area_smaller_than_requested() {
        let area = Rect::new(0, 0, 30, 10);
        let r = centered_rect(area, 60, 20);
        assert_eq!(r.width, 30);
        assert_eq!(r.height, 10);
    }

    #[test]
    fn completion_display_truncates_long_paths() {
        let long_path = "/very/long/path/that/exceeds/thirty/characters/swapfile";
        let max_chars = 30_usize;
        let char_count = long_path.chars().count();
        let display: String = if char_count > max_chars {
            let tail: String = long_path
                .chars()
                .skip(char_count - (max_chars - 2))
                .collect();
            format!("..{tail}")
        } else {
            long_path.to_string()
        };
        assert!(display.chars().count() <= max_chars);
        assert!(display.starts_with(".."));
    }

    #[test]
    fn completion_display_truncates_non_ascii_paths() {
        // Non-ASCII path — byte-based slicing would panic; char-based must not.
        let path = "/home/rénata/swap/fichier-échange-très-long-nom";
        let max_chars = 30_usize;
        let char_count = path.chars().count();
        let display: String = if char_count > max_chars {
            let tail: String = path.chars().skip(char_count - (max_chars - 2)).collect();
            format!("..{tail}")
        } else {
            path.to_string()
        };
        assert!(display.chars().count() <= max_chars);
        assert!(display.starts_with(".."));
    }
}
