//! Metrics endpoint.
//!
//! - `GET /metrics` — Prometheus metrics

use axum::response::IntoResponse;
use metrics_exporter_prometheus::PrometheusBuilder;

pub async fn get_metrics() -> impl IntoResponse {
    // TODO: Implement Prometheus metrics export
    "Metrics endpoint — placeholder"
}
