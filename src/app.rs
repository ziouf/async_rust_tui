use jiff::{Unit, Zoned};
use ratatui::widgets::ListItem;
use sncf::{Journey, fetch_journeys};
use sncf::{client::ReqwestClient, fetch_places};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub use sncf::Place;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SavedPlace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    pub start: SavedPlace,
    pub destination: SavedPlace,
}

pub enum Mode {
    InputStart,
    InputDest,
    Timer,
}

pub struct InputState {
    pub text: String,
    pub cursor: usize,
    pub suggestions: Vec<Place>,
    pub selected: usize,
    pub last_edit_at: Instant,
    pub last_queried: String,
    pub loading: bool,
    pub error: Option<String>,
}

pub struct TimerState {
    pub start: Instant,
    pub duration: Duration,
    pub notified: bool,
    pub zero_at: Option<Instant>,
}

#[derive(Clone)]
struct JourneyKey {
    dep: Zoned,
    arr: Zoned,
    duration_secs: i64,
    nb_transfers: i64,
}

pub struct App {
    pub mode: Mode,
    pub input: InputState,
    pub timer: TimerState,
    pub client: ReqwestClient,
    pub api_key: String,
    pub refresh_task: Option<JoinHandle<()>>,
    pub data_receiver: Option<mpsc::Receiver<Vec<Journey>>>,
    pub chosen_start: Option<Place>,
    pub chosen_dest: Option<Place>,
    pub config: Option<AppConfig>,
    pub journeys: Vec<Journey>,
    pub journeys_selected: usize,
    pub journeys_loading: bool,
}

pub const CONFIG_PATH: &str = "config.toml";
pub const SUGGESTION_DEBOUNCE_MS: u64 = 350;
pub const MIN_QUERY_LEN: usize = 2;

impl App {
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        let client = sncf::client::ReqwestClient::new();
        let loaded = load_config();
        Ok(Self {
            mode: if loaded.is_some() {
                Mode::Timer
            } else {
                Mode::InputStart
            },
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
                duration: Duration::new(3600, 0),
                notified: false,
                zero_at: None,
            },
            client,
            api_key,
            refresh_task: None,
            data_receiver: None,
            chosen_start: loaded.as_ref().map(|c| Place {
                id: c.start.id.clone(),
                name: c.start.name.clone(),
                embedded_type: Some("stop_area".into()),
            }),
            chosen_dest: loaded.as_ref().map(|c| Place {
                id: c.destination.id.clone(),
                name: c.destination.name.clone(),
                embedded_type: Some("stop_area".into()),
            }),
            config: loaded,
            journeys: vec![],
            journeys_selected: 0,
            journeys_loading: true,
        })
    }

    pub fn input_title(&self) -> &'static str {
        match self.mode {
            Mode::InputStart => "Start station",
            Mode::InputDest => "Destination station",
            Mode::Timer => "",
        }
    }

    pub fn suggestion_items(&self) -> Vec<ListItem<'_>> {
        if self.input.loading {
            vec![ListItem::new("Loading...")]
        } else if let Some(err) = &self.input.error {
            vec![ListItem::new(format!("Error: {err}"))]
        } else if self.input.suggestions.is_empty() && self.input.text.len() >= MIN_QUERY_LEN {
            vec![ListItem::new("No results")]
        } else {
            self.input
                .suggestions
                .iter()
                .map(|p| ListItem::new(p.name.clone()))
                .collect()
        }
    }

    pub async fn maybe_fetch_suggestions(&mut self) {
        if self.input.text.len() >= MIN_QUERY_LEN
            && self.input.text != self.input.last_queried
            && self.input.last_edit_at.elapsed() >= Duration::from_millis(SUGGESTION_DEBOUNCE_MS)
        {
            self.input.loading = true;
            let query = self.input.text.clone();
            match fetch_places(&self.client, &self.api_key, &query).await {
                Ok(list) => {
                    self.input.suggestions = list;
                    self.input.selected = 0;
                    self.input.error = None;
                    self.input.last_queried = query;
                }
                Err(e) => {
                    self.input.error = Some(format!("{e}"));
                }
            }
            self.input.loading = false;
        }
    }

    pub fn reset_input(&mut self) {
        self.input.text.clear();
        self.input.cursor = 0;
        self.input.suggestions.clear();
        self.input.selected = 0;
        self.input.last_queried.clear();
        self.input.error = None;
    }

    pub async fn start_refresh_task(&mut self) {
        if self.config.is_none() || self.refresh_task.is_some() {
            tracing::info!("No configuration available.");
            return;
        }

        tracing::info!("Configuration available.");
        let (data_sender, data_receiver) = mpsc::channel::<Vec<Journey>>(5);
        let refresh_task = tokio::spawn(async move {
            tracing::info!("refresh task started");

            loop {
                let config = self.config.expect("Config must be available");
                tracing::info!("sending data");
                let msg = fetch_journeys(
                    &self.client,
                    &self.api_key,
                    &config.start.id,
                    &config.destination.id,
                )
                .await
                .unwrap();
                if let Err(e) = data_sender.send(msg).await {
                    tracing::error!("Error sending message: {e}");
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
            }

            tracing::error!("refresh task terminated");
        });

        self.refresh_task = Some(refresh_task);
        self.data_receiver = Some(data_receiver);
    }

    pub fn remaining_time(&self, elapsed: Duration) -> Duration {
        if elapsed >= self.timer.duration {
            Duration::from_secs(0)
        } else {
            self.timer.duration - elapsed
        }
    }

    pub fn update_timer_from_selection(&mut self) {
        if self.journeys.is_empty() {
            return;
        }
        let sel = self.journeys_selected.min(self.journeys.len() - 1);
        let dep = self.journeys[sel].dep.clone();
        let now = Zoned::now();
        let mut secs = match now.until(&dep) {
            Ok(span) => span.total(Unit::Second).unwrap() as i64,

            Err(_) => 0,
        };
        if secs < 0 {
            secs = 0;
        }
        self.timer.start = Instant::now();
        self.timer.duration = Duration::from_secs(secs as u64);
        self.timer.notified = false;
        self.timer.zero_at = None;
    }

    pub fn replace_journeys(&mut self, data: Vec<Journey>) {
        let selected_key = self.selected_journey_key();
        // Take care your date are probably not sorted.....
        self.journeys = data;
        self.journeys_loading = false;
        if self.journeys.is_empty() {
            self.journeys_selected = 0;
            return;
        }

        if let Some(key) = selected_key {
            if let Some(idx) = self.journeys.iter().position(|j| {
                j.dep == key.dep
                    && j.arr == key.arr
                    && j.duration_secs == key.duration_secs
                    && j.nb_transfers == key.nb_transfers
            }) {
                self.journeys_selected = idx;
            } else {
                self.journeys_selected = self.journeys_selected.min(self.journeys.len() - 1);
            }
        } else {
            self.journeys_selected = 0;
        }

        self.update_timer_from_selection();
    }

    fn selected_journey_key(&self) -> Option<JourneyKey> {
        if self.journeys.is_empty() {
            return None;
        }
        let sel = self.journeys_selected.min(self.journeys.len() - 1);
        let journey = &self.journeys[sel];
        Some(JourneyKey {
            dep: journey.dep.clone(),
            arr: journey.arr.clone(),
            duration_secs: journey.duration_secs,
            nb_transfers: journey.nb_transfers,
        })
    }
}

pub fn load_config() -> Option<AppConfig> {
    std::fs::read_to_string(CONFIG_PATH)
        .ok()
        .and_then(|d| toml::from_str(&d).ok())
}
pub fn save_config(conf: &AppConfig) -> anyhow::Result<()> {
    let data = toml::to_string_pretty(conf)?;
    std::fs::write(CONFIG_PATH, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{App, Journey};
    use sncf::parse_sncf_dt;
    use std::time::Duration;

    fn make_journey(dep: &str, arr: &str) -> Journey {
        Journey {
            dep: parse_sncf_dt(dep).expect("dep parse failed"),
            arr: parse_sncf_dt(arr).expect("arr parse failed"),
            date_str: "2026-01-03".to_string(),
            duration_secs: 3600,
            nb_transfers: 0,
        }
    }

    #[test]
    fn replace_journeys_sorts_and_preserves_selection() {
        let mut app = App::new("test".to_string()).expect("app init failed");
        let j1 = make_journey("20260103T080000", "20260103T090000");
        let j2 = make_journey("20260103T100000", "20260103T110000");
        let j3 = make_journey("20260103T120000", "20260103T130000");

        app.journeys = vec![j1.clone(), j2.clone(), j3.clone()];
        app.journeys_selected = 2;

        app.replace_journeys(vec![j3.clone(), j1.clone(), j2.clone()]);

        assert!(app.journeys.windows(2).all(|w| w[0].dep <= w[1].dep));
        let selected = &app.journeys[app.journeys_selected];
        assert_eq!(selected.dep, j3.dep);
        assert_eq!(selected.arr, j3.arr);
    }

    #[test]
    fn remaining_time_never_negative() {
        let mut app = App::new("test".to_string()).expect("app init failed");
        app.timer.duration = Duration::from_secs(5);

        let remaining = app.remaining_time(Duration::from_secs(10));
        assert_eq!(remaining, Duration::from_secs(0));
    }
}
