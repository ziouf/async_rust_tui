use std::time::Instant;

use crossterm::event::{self};

use crate::app::{App, AppConfig, Mode, SavedPlace, save_config};

#[derive(Debug, PartialEq)]
pub(crate) enum QuitApp {
    Yes,
    No,
}

pub async fn handle_keys(app: &mut App, key: event::KeyEvent) -> Result<QuitApp, anyhow::Error> {
    match app.mode {
        Mode::InputStart | Mode::InputDest => handle_station_keys(app, key.code),
    }
}

pub fn handle_station_keys(
    app: &mut App,
    code: crossterm::event::KeyCode,
) -> Result<QuitApp, anyhow::Error> {
    use crossterm::event::KeyCode::*;
    match code {
        Char('q') | Esc => return Ok(QuitApp::Yes),
        Enter => {
            if let Some(place) = app.input.suggestions.get(app.input.selected).cloned() {
                match app.mode {
                    Mode::InputStart => {
                        app.chosen_start = Some(place);
                        app.reset_input();
                        tracing::info!("Start station selected {:?}", app.chosen_start);
                        app.mode = Mode::InputDest;
                    }
                    Mode::InputDest => {
                        app.chosen_dest = Some(place);
                        app.reset_input();
                        tracing::info!("Destination station selected {:?}", app.chosen_dest);
                        if let (Some(start), Some(dest)) =
                            (app.chosen_start.clone(), app.chosen_dest.clone())
                        {
                            let conf = AppConfig {
                                start: SavedPlace {
                                    id: start.id,
                                    name: start.name,
                                },
                                destination: SavedPlace {
                                    id: dest.id,
                                    name: dest.name,
                                },
                            };
                            let _ = save_config(&conf);
                            app.config = Some(conf);
                            return Ok(QuitApp::Yes);
                        }
                    }
                }
            }
        }
        Backspace => {
            if app.input.cursor > 0 && app.input.cursor <= app.input.text.len() {
                app.input.text.remove(app.input.cursor - 1);
                app.input.cursor -= 1;
                app.input.last_edit_at = Instant::now();
            }
        }
        Left => {
            if app.input.cursor > 0 {
                app.input.cursor -= 1;
            }
        }
        Right => {
            if app.input.cursor < app.input.text.len() {
                app.input.cursor += 1;
            }
        }
        Up => {
            if app.input.selected > 0 {
                app.input.selected -= 1;
            }
        }
        Down => {
            if app.input.selected + 1 < app.input.suggestions.len() {
                app.input.selected += 1;
            }
        }
        Char(c) => {
            app.input.text.insert(app.input.cursor, c);
            app.input.cursor += 1;
            app.input.last_edit_at = Instant::now();
        }
        _ => {}
    }
    Ok(QuitApp::No)
}

#[cfg(test)]
mod tests {
    use super::{QuitApp, handle_station_keys};
    use crate::app::{App, AppConfig, CONFIG_PATH, Mode};
    use sncf::Place;
    use crossterm::event::KeyCode;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    struct CwdGuard {
        original: PathBuf,
        temp: PathBuf,
    }

    impl CwdGuard {
        fn new() -> anyhow::Result<Self> {
            let original = std::env::current_dir()?;
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let temp = std::env::temp_dir()
                .join(format!("async_rust_tui_test_{nanos}_{}", std::process::id()));
            std::fs::create_dir_all(&temp)?;
            std::env::set_current_dir(&temp)?;
            Ok(Self { original, temp })
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
            let _ = std::fs::remove_dir_all(&self.temp);
        }
    }

    #[test]
    fn saves_config_after_destination_selection() {
        let _lock = CWD_LOCK.lock().expect("cwd lock poisoned");
        let _guard = CwdGuard::new().expect("failed to setup temp cwd");

        let mut app = App::new("test".to_string()).expect("app init failed");
        app.mode = Mode::InputDest;
        app.chosen_start = Some(Place {
            id: "stop_area:SNCF:87747006".to_string(),
            name: "Grenoble (Grenoble)".to_string(),
            embedded_type: Some("stop_area".to_string()),
        });
        app.input.suggestions = vec![Place {
            id: "stop_area:SNCF:87747337".to_string(),
            name: "Lyon Part Dieu".to_string(),
            embedded_type: Some("stop_area".to_string()),
        }];
        app.input.selected = 0;

        let exit = handle_station_keys(&mut app, crossterm::event::KeyCode::Enter)
            .expect("handle_station_keys failed");
        assert_eq!(exit, QuitApp::Yes);

        let saved = std::fs::read_to_string(CONFIG_PATH).expect("config not saved");
        let parsed: AppConfig = toml::from_str(&saved).expect("invalid config format");

        let conf = app.config.expect("app config not set");
        assert_eq!(conf.start.id, parsed.start.id);
        assert_eq!(conf.start.name, parsed.start.name);
        assert_eq!(conf.destination.id, parsed.destination.id);
        assert_eq!(conf.destination.name, parsed.destination.name);
    }

    #[test]
    fn up_down_keys_change_selection() {
        let mut app = App::new("test".to_string()).expect("app init failed");
        app.mode = Mode::InputStart;
        app.input.suggestions = vec![
            Place {
                id: "stop_area:SNCF:1".to_string(),
                name: "Alpha".to_string(),
                embedded_type: Some("stop_area".to_string()),
            },
            Place {
                id: "stop_area:SNCF:2".to_string(),
                name: "Beta".to_string(),
                embedded_type: Some("stop_area".to_string()),
            },
            Place {
                id: "stop_area:SNCF:3".to_string(),
                name: "Gamma".to_string(),
                embedded_type: Some("stop_area".to_string()),
            },
        ];
        app.input.selected = 0;

        handle_station_keys(&mut app, KeyCode::Down).expect("down should work");
        assert_eq!(app.input.selected, 1);

        handle_station_keys(&mut app, KeyCode::Down).expect("down should work");
        assert_eq!(app.input.selected, 2);

        handle_station_keys(&mut app, KeyCode::Down).expect("down should clamp");
        assert_eq!(app.input.selected, 2);

        handle_station_keys(&mut app, KeyCode::Up).expect("up should work");
        assert_eq!(app.input.selected, 1);

        handle_station_keys(&mut app, KeyCode::Up).expect("up should work");
        assert_eq!(app.input.selected, 0);

        handle_station_keys(&mut app, KeyCode::Up).expect("up should clamp");
        assert_eq!(app.input.selected, 0);
    }
}
