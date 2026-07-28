use std::net::SocketAddr;

use anyhow::Context;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::new("electronic_journey_server=info,tower_http=info")
            }),
        )
        .init();

    let address = std::env::var("EJOURNEY_SERVER_ADDRESS")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_owned())
        .parse::<SocketAddr>()
        .context("EJOURNEY_SERVER_ADDRESS is not a valid socket address")?;
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("failed to bind server to {address}"))?;

    info!(%address, "Electronic Journey API listening");
    axum::serve(listener, electronic_journey_server::app())
        .await
        .context("API server stopped unexpectedly")
}
