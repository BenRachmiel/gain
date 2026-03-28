mod config;
mod lyrics;
mod routes;
mod sources;
mod state;
mod tagging;
mod transcode;
mod util;
mod worker;

use axum::{
    Router,
    routing::{get, post},
};
use tracing_subscriber::EnvFilter;

use state::AppState;

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("LOG_LEVEL").unwrap_or_else(|_| EnvFilter::new("debug")),
        )
        .init();

    let config = config::Config::from_env();
    let bind_addr = config.bind_addr;
    let state = AppState::new(config);

    worker::spawn_worker(state.clone());

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/search", get(routes::search))
        .route("/api/start", post(routes::start_job))
        .route("/api/jobs/{job_id}/tracks", post(routes::append_tracks))
        .route("/api/jobs/{job_id}/resolve", post(routes::mark_resolved))
        .route("/api/jobs", get(routes::get_jobs))
        .route("/api/jobs/clear", post(routes::clear_jobs))
        .route("/api/status", get(routes::status_stream))
        .route("/api/resolve/{album_id}", get(routes::resolve_album))
        .with_state(state);

    tracing::info!("listening on {bind_addr}");
    let listener = tokio::net::TcpListener::bind(bind_addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
    tracing::info!("shutdown complete");
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut sigterm =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }
    tracing::info!("shutdown signal received");
}
