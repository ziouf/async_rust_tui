mod app;
mod events;
mod ui;

use crate::app::{App, Mode};
use crate::events::{QuitApp, handle_keys};
use crossterm::event;
use crossterm::event::Event;
use notify_rust::{Hint, Notification, Timeout, Urgency};
use ratatui::{Terminal, prelude::CrosstermBackend};
use std::{io, time::Duration};
use tokio::sync::mpsc::error;
use tokio::time::Instant;

pub const APPNAME: &str = env!("CARGO_PKG_NAME");

pub async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    api_key: String,
) -> anyhow::Result<()> {
    let mut app = App::new(api_key)?;
    // We start the external task here if we have a config defined.
    app.start_refresh_task().await;

    // Following loop simulate the TUI main loop.
    let tick_rate = Duration::from_millis(120);
    let mut last_tick = Instant::now();
    loop {
        // Check that the external task is running, if not return an error.
        if let Some(handle) = app.refresh_task.as_mut()
            && handle.is_finished()
        {
            let handle = app.refresh_task.take().expect("refresh_task should exist");
            return refresh_task_result_to_err(handle.await);
        }

        // Manage message from refresh task
        if let Some(receiver) = app.data_receiver.as_mut() {
            match receiver.try_recv() {
                Err(error::TryRecvError::Empty) => {}
                Err(error::TryRecvError::Disconnected) => {}
                Ok(data) => {
                    tracing::info!("data received");
                    tracing::debug!("Data: {data:?}");
                    app.replace_journeys(data);
                }
            };
        }

        terminal.draw(|f| match app.mode {
            Mode::InputStart | Mode::InputDest => ui::draw_input(f, &app),
            Mode::Timer => ui::draw_timer(f, &app),
        })?;

        match app.mode {
            Mode::InputStart | Mode::InputDest => {
                app.maybe_fetch_suggestions().await;
            }
            Mode::Timer => {
                // After reaching zero, send notification once
                let elapsed = app.timer.start.elapsed();
                let remaining = app.remaining_time(elapsed);
                if remaining.is_zero() && !app.timer.notified {
                    let _ = tokio::task::spawn_blocking(|| {
                        Notification::new()
                            .summary("Timer finished")
                            .body("Train is leaving.")
                            .icon("dialog-information")
                            .appname(APPNAME)
                            .timeout(Timeout::Never)
                            .hint(Hint::Resident(true))
                            .hint(Hint::Transient(false))
                            .hint(Hint::Urgency(Urgency::Normal))
                            .hint(Hint::SoundName("complete".to_owned()))
                            .hint(Hint::SuppressSound(false))
                            .show()
                    })
                    .await;
                    app.timer.notified = true;
                    app.timer.zero_at = Some(std::time::Instant::now());
                }
            }
        }

        // Handle input events
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            let exit = handle_keys(&mut app, key).await?;
            if exit == QuitApp::Yes {
                break;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn refresh_task_result_to_err(res: Result<(), tokio::task::JoinError>) -> anyhow::Result<()> {
    match res {
        Ok(_) => {
            tracing::warn!("⚠️ refresh task stopped.");
            Err(anyhow::anyhow!(
                "😵 refresh task stopped unexpectedly. See logs for details."
            ))
        }
        Err(e) => {
            tracing::error!("💥 refresh task panicked : {e}");
            Err(anyhow::anyhow!(
                "💥 refresh task panicked: {e}. See logs for details."
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use sncf::client::ReqwestClient;
    use sncf::{SncfAPIError, fetch_journeys, fetch_places};

    use super::*;

    #[test]
    fn fake() {
        assert_eq!(1, 1);
    }

    #[test]
    #[should_panic(expected = "Houston, we have a problem !")]
    fn fake_panic() {
        panic!("Houston, we have a problem !");
    }

    #[tokio::test]
    async fn refresh_task_result_ok_becomes_error() {
        let err = refresh_task_result_to_err(Ok(())).unwrap_err();
        assert_eq!(
            err.to_string(),
            "😵 refresh task stopped unexpectedly. See logs for details."
        );
    }

    #[tokio::test]
    async fn refresh_task_result_join_error_panics_message() {
        let handle = tokio::spawn(async { panic!("boom") });
        let join_err = handle.await.unwrap_err();

        let err = refresh_task_result_to_err(Err(join_err)).unwrap_err();
        assert!(
            err.to_string().contains("💥 refresh task panicked:"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    #[ignore = "hits live SNCF API"]
    // Take care you need to export the SNCF_API_KEY
    async fn test_fetch_places_live_api() {
        let api_key =
            std::env::var("SNCF_API_KEY").expect("set SNCF_API_KEY to run the live API test");
        let client = ReqwestClient::new();

        let results = fetch_places(&client, &api_key, "Grenoble")
            .await
            .expect("expected live SNCF API to return places");

        dbg!(&results);

        assert!(
            !results.is_empty(),
            "expected at least one stop_area place from live API"
        );
        assert!(
            results.iter().any(|place| {
                place.id == "stop_area:SNCF:87747006" && place.name == "Grenoble (Grenoble)"
            }),
            "expected Grenoble (Grenoble) stop_area in live API results"
        );
    }

    #[tokio::test]
    #[ignore = "hits live SNCF API"]
    // Take care you need to export the SNCF_API_KEY
    async fn test_fetch_journeys_live_api() {
        let api_key =
            std::env::var("SNCF_API_KEY").expect("set SNCF_API_KEY to run the live API test");
        let client = ReqwestClient::new();

        // fetch_journeys should return 25 items.
        let results = fetch_journeys(
            &client,
            &api_key,
            "stop_area:SNCF:87747006",
            "stop_area:SNCF:87747337",
        )
        .await
        .expect("expected live SNCF API to return journeys");

        dbg!(&results);

        assert!(
            !results.is_empty(),
            "expected at least one journey from live API"
        );

        assert_eq!(25, results.len());
    }

    #[tokio::test]
    #[ignore = "hits live SNCF API"]
    async fn fetch_places_live_api_invalid_api_key() {
        let client = ReqwestClient::new();
        let err = fetch_places(&client, "invalid_api_key", "Grenoble")
            .await
            .expect_err("expected invalid api key to return an error");

        match err {
            SncfAPIError::ApiError { status, message } => {
                assert!(
                    status == 401 || status == 403,
                    "unexpected status for invalid api key: {status}"
                );
                assert!(
                    message.contains("Token absent"),
                    "unexpected api error message: {message}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
