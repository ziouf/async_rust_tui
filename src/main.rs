use std::env;

use async_rust_tui::{APPNAME, run};
use ratatui::{init, restore};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Setup logging
    let file_appender = tracing_appender::rolling::daily("logs", format!("{}.log", APPNAME));
    let (non_blocking_appender, _guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(non_blocking_appender)
                .with_ansi(false),
        )
        .init();

    tracing::info!("Application starting");

    let _ = dotenvy::dotenv();
    let api_key = env::var("SNCF_API_KEY")?;

    // Setup terminal
    let mut terminal = init();

    let res = run(&mut terminal, api_key).await;

    // Restore terminal
    restore();
    tracing::info!("Application ending");
    res
}
