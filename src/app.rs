use ratatui::widgets::ListItem;
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

pub struct App {
    pub mode: Mode,
    pub input: InputState,
    pub client: ReqwestClient,
    pub api_key: String,
    pub refresh_task: Option<JoinHandle<()>>,
    pub data_receiver: mpsc::Receiver<String>,
    pub chosen_start: Option<Place>,
    pub chosen_dest: Option<Place>,
    pub config: Option<AppConfig>,
}

pub const CONFIG_PATH: &str = "config.toml";
pub const SUGGESTION_DEBOUNCE_MS: u64 = 350;
pub const MIN_QUERY_LEN: usize = 2;

impl App {
    pub fn new(api_key: String) -> anyhow::Result<Self> {
        let client = sncf::client::ReqwestClient::new();
        let loaded = load_config();
        Ok(Self {
            mode: Mode::InputStart,
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
        })
    }

    pub fn input_title(&self) -> &'static str {
        match self.mode {
            Mode::InputStart => "Start station",
            Mode::InputDest => "Destination station",
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

    pub fn start_refresh_task(&mut self) {
        if self.config.is_none() || self.refresh_task.is_some() {
            tracing::info!("No configuration available.");
            return;
        }

        tracing::info!("Configuration available.");
        let (data_sender, data_receiver) = mpsc::channel::<String>(5);
        let refresh_task = tokio::spawn(async move {
            let mut count = 0;
            tracing::info!("refresh task started");

            loop {
                tracing::info!("sending data");
                let msg = format!("Hello {count}");
                if let Err(e) = data_sender.send(msg).await {
                    tracing::error!("Error sending message: {e}");
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
                count += 1;
            }

            tracing::error!("refresh task terminated");
        });

        self.refresh_task = Some(refresh_task);
        self.data_receiver = Some(data_receiver);
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
