//! Structured JSON logging with tracing.
//!
//! Format JSON dengan fields: timestamp, level, target, message, request_id, correlation_id.

use crate::config::settings::Settings;
use tracing_subscriber::EnvFilter;

/// Initialize global JSON structured logging based on `settings.log_level`.
pub fn init(settings: &Settings) -> anyhow::Result<()> {
    let filter = EnvFilter::try_new(&settings.log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .with_target(true)
        .with_current_span(false)
        .init();

    Ok(())
}
