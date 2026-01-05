use crate::app::App;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
pub fn draw_input(f: &mut ratatui::Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(centered_rect(80, 70, area));

    let cursor_pos = app.input.cursor.min(app.input.text.len());
    let (left, right) = app.input.text.split_at(cursor_pos);
    let input_line = Line::from(vec![
        Span::raw(left),
        Span::styled("|", Style::default()),
        Span::raw(right),
    ]);
    let title = app.input_title();
    let input =
        Paragraph::new(input_line).block(Block::default().borders(Borders::ALL).title(title));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = app.suggestion_items();
    let list_len = items.len();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Suggestions"))
        .highlight_symbol("▶ ");
    let mut state = ratatui::widgets::ListState::default();
    if list_len > 0 {
        let sel = app.input.selected.min(list_len - 1);
        state.select(Some(sel));
    }
    f.render_stateful_widget(list, chunks[1], &mut state);
}

pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1]);
    horizontal[1]
}

#[cfg(test)]
mod tests {
    use super::draw_input;
    use crate::app::{App, InputState, Mode};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use sncf::Place;
    use sncf::client::ReqwestClient;
    use std::time::Instant;

    #[test]
    fn snapshot_input_start_screen() {
        let app = App {
            mode: Mode::InputStart,
            input: InputState {
                text: "Gre".to_string(),
                cursor: 3,
                suggestions: vec![
                    Place {
                        id: "stop_area:SNCF:87747006".to_string(),
                        name: "Grenoble (Grenoble)".to_string(),
                        embedded_type: Some("stop_area".to_string()),
                    },
                    Place {
                        id: "stop_area:SNCF:87751003".to_string(),
                        name: "Grenoble UGI".to_string(),
                        embedded_type: Some("stop_area".to_string()),
                    },
                    Place {
                        id: "stop_area:SNCF:87751201".to_string(),
                        name: "Grenoble-Universites".to_string(),
                        embedded_type: Some("stop_area".to_string()),
                    },
                ],
                selected: 1,
                last_edit_at: Instant::now(),
                last_queried: String::new(),
                loading: false,
                error: None,
            },
            client: ReqwestClient::new(),
            api_key: "test".to_string(),
            refresh_task: None,
            data_receiver: None,
            chosen_start: None,
            chosen_dest: None,
            config: None,
        };

        let backend = TestBackend::new(50, 12);
        let mut terminal = Terminal::new(backend).expect("terminal should init");
        terminal
            .draw(|f| draw_input(f, &app))
            .expect("draw should succeed");

        insta::assert_snapshot!("input_start_screen", terminal.backend());
    }
}
