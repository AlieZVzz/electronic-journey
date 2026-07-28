use axum::{routing::get, Json, Router};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
    version: &'static str,
}

pub fn app() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/status", get(health))
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "electronic-journey-server",
        version: env!("CARGO_PKG_VERSION"),
    })
}
