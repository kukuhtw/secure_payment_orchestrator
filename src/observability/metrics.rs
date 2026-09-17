//! Prometheus metrics for observability.
//!
//! - `spo_payments_total` — Counter per status + provider
//! - `spo_payment_duration_ms` — Histogram latensi per provider

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the global Prometheus recorder. Must be called once at startup.
pub fn init() -> anyhow::Result<()> {
    let handle = PrometheusBuilder::new().install_recorder()?;
    PROMETHEUS_HANDLE
        .set(handle)
        .map_err(|_| anyhow::anyhow!("metrics recorder already initialized"))?;
    Ok(())
}

/// Render the current Prometheus metrics snapshot as text exposition format.
pub fn render() -> String {
    PROMETHEUS_HANDLE
        .get()
        .map(|handle| handle.render())
        .unwrap_or_default()
}
