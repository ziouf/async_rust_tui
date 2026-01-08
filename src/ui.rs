use std::time::Duration;

use crate::app::App;
use jiff::fmt::strtime;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table};
use tui_big_text::BigText;

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

pub fn draw_timer(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(size);
    if let Some(conf) = &app.config {
        let header = Paragraph::new(Line::from(vec![Span::raw(format!(
            "{} → {}",
            conf.start.name, conf.destination.name
        ))]))
        .block(Block::default().borders(Borders::ALL).title("Config"));
        f.render_widget(header, rows[0]);
    }
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(rows[1]);
    draw_journeys(f, app, cols[0]);

    let elapsed = app.timer.start.elapsed();
    let remaining = app.remaining_time(elapsed);
    let show = if let Some(z) = app.timer.zero_at {
        ((std::time::Instant::now() - z).as_millis() / 500).is_multiple_of(2)
    } else {
        true
    };
    let time_str = format_hhmmss(remaining);
    // Right panel (timer) with a visible border
    let timer_block = Block::default().borders(Borders::ALL).title("Timer");
    let timer_area = cols[1];
    f.render_widget(timer_block.clone(), timer_area);

    let inner = timer_block.inner(timer_area);
    if show {
        let big = BigText::builder()
            .style(Style::default().fg(Color::Cyan))
            .alignment(ratatui::prelude::Alignment::Center)
            .lines(vec![Line::from(time_str)])
            .build();
        f.render_widget(big, inner);
    } else {
        f.render_widget(Clear, inner);
    }
}

pub fn draw_journeys(f: &mut ratatui::Frame, app: &App, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Journeys");
    if app.journeys_loading {
        let p = Paragraph::new("Loading...").block(block);
        f.render_widget(p, area);
        return;
    }
    let header = Row::new(vec![
        Cell::from("Date"),
        Cell::from("Dur"),
        Cell::from("Changes"),
        Cell::from("Dep at"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));
    let rows = app.journeys.iter().map(|j| {
        let dur_min = (j.duration_secs / 60).max(0);
        Row::new(vec![
            Cell::from(j.date_str.clone()),
            Cell::from(format!("{}m", dur_min)),
            Cell::from(format!("{}", j.nb_transfers)),
            Cell::from(strtime::format("%T", &j.dep).expect("We should not have formating error")),
        ])
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(20),
        ],
    )
    .header(header)
    .block(block)
    .highlight_symbol("▶ ");
    let mut state = ratatui::widgets::TableState::default();
    if !app.journeys.is_empty() {
        let sel = app.journeys_selected.min(app.journeys.len() - 1);
        state.select(Some(sel));
    }
    // There might be a bug...
    f.render_widget(table, area);
}

pub fn format_hhmmss(dur: Duration) -> String {
    let secs = dur.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::{draw_input, draw_timer};
    use crate::app::{App, AppConfig, InputState, Mode, SavedPlace, TimerState};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use sncf::client::ReqwestClient;
    use sncf::{Journey, Place, parse_sncf_dt};
    use std::time::{Duration, Instant};

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
            timer: TimerState {
                start: Instant::now(),
                duration: Duration::new(0, 0),
                notified: false,
                zero_at: None,
            },
            client: Arc::new(ReqwestClient::new()),
            api_key: "test".to_string(),
            refresh_task: None,
            data_receiver: None,
            chosen_start: None,
            chosen_dest: None,
            config: None,
            journeys: vec![],
            journeys_selected: 0,
            journeys_loading: false,
        };

        let backend = TestBackend::new(50, 12);
        let mut terminal = Terminal::new(backend).expect("terminal should init");
        terminal
            .draw(|f| draw_input(f, &app))
            .expect("draw should succeed");

        insta::assert_snapshot!("input_start_screen", terminal.backend());
    }

    fn make_journey(
        dep: &str,
        arr: &str,
        date_str: &str,
        duration_secs: i64,
        nb_transfers: i64,
    ) -> Journey {
        Journey {
            dep: parse_sncf_dt(dep).expect("dep parse failed"),
            arr: parse_sncf_dt(arr).expect("arr parse failed"),
            date_str: date_str.to_string(),
            duration_secs,
            nb_transfers,
        }
    }

    #[test]
    fn snapshot_timer_screen() {
        let app = App {
            mode: Mode::Timer,
            input: InputState {
                text: String::new(),
                cursor: 0,
                suggestions: vec![],
                selected: 0,
                last_edit_at: Instant::now(),
                last_queried: String::new(),
                loading: false,
                error: None,
            },
            timer: TimerState {
                start: Instant::now(),
                duration: Duration::new(0, 0),
                notified: false,
                zero_at: None,
            },
            client: Arc::new(ReqwestClient::new()),
            api_key: "test".to_string(),
            refresh_task: None,
            data_receiver: None,
            chosen_start: None,
            chosen_dest: None,
            config: Some(AppConfig {
                start: SavedPlace {
                    id: "stop_area:SNCF:87747006".to_string(),
                    name: "Grenoble (Grenoble)".to_string(),
                },
                destination: SavedPlace {
                    id: "stop_area:SNCF:87747337".to_string(),
                    name: "Lyon Part Dieu".to_string(),
                },
            }),
            journeys: vec![
                make_journey("20260103T080000", "20260103T091000", "2026-01-03", 4200, 0),
                make_journey("20260103T093000", "20260103T105000", "2026-01-03", 4800, 1),
                make_journey("20260103T110000", "20260103T121500", "2026-01-03", 4500, 2),
            ],
            journeys_selected: 1,
            journeys_loading: false,
        };

        let backend = TestBackend::new(200, 40);
        let mut terminal = Terminal::new(backend).expect("terminal should init");
        terminal
            .draw(|f| draw_timer(f, &app))
            .expect("draw should succeed");

        insta::assert_snapshot!("timer_screen", terminal.backend());
    }

    #[test]
    fn snapshot_timer_screen_value() {
        let app = App {
            mode: Mode::Timer,
            input: InputState {
                text: String::new(),
                cursor: 0,
                suggestions: vec![],
                selected: 0,
                last_edit_at: Instant::now(),
                last_queried: String::new(),
                loading: false,
                error: None,
            },
            timer: TimerState {
                start: Instant::now(),
                // Add one extra second to keep the snapshot stable.
                duration: Duration::from_secs(5_330),
                notified: false,
                zero_at: None,
            },
            client: Arc::new(ReqwestClient::new()),
            api_key: "test".to_string(),
            refresh_task: None,
            data_receiver: None,
            chosen_start: None,
            chosen_dest: None,
            config: Some(AppConfig {
                start: SavedPlace {
                    id: "stop_area:SNCF:87747006".to_string(),
                    name: "Grenoble (Grenoble)".to_string(),
                },
                destination: SavedPlace {
                    id: "stop_area:SNCF:87747337".to_string(),
                    name: "Lyon Part Dieu".to_string(),
                },
            }),
            journeys: vec![
                make_journey("20260103T080000", "20260103T091000", "2026-01-03", 4200, 0),
                make_journey("20260103T093000", "20260103T105000", "2026-01-03", 4800, 1),
                make_journey("20260103T110000", "20260103T121500", "2026-01-03", 4500, 2),
            ],
            journeys_selected: 1,
            journeys_loading: false,
        };

        let backend = TestBackend::new(200, 40);
        let mut terminal = Terminal::new(backend).expect("terminal should init");
        terminal
            .draw(|f| draw_timer(f, &app))
            .expect("draw should succeed");

        insta::assert_snapshot!("timer_screen_value", terminal.backend());
    }
}
