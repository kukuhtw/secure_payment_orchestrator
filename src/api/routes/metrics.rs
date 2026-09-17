//! Metrics endpoint.
//!
//! - `GET /metrics` — Prometheus metrics

use axum::response::IntoResponse;

pub async fn get_metrics() -> impl IntoResponse {
    crate::observability::metrics::render()
}
