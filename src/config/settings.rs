//! Application configuration loaded from environment variables.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server_port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub log_level: String,
    pub webhook_secret_alpha: String,
    pub webhook_secret_beta: String,
    pub webhook_secret_gamma: String,
    pub max_retry_attempts: i32,
    pub circuit_breaker_threshold: u32,
    pub circuit_breaker_timeout_seconds: u64,
}

impl Settings {
    /// Load settings from environment variables (with .env file support).
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        Ok(Self {
            server_port: std::env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()?,
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://spo:spo@localhost:5432/spo".into()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".into()),
            log_level: std::env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "info".into()),
            webhook_secret_alpha: std::env::var("WEBHOOK_SECRET_ALPHA")
                .unwrap_or_else(|_| "alpha-secret-dev".into()),
            webhook_secret_beta: std::env::var("WEBHOOK_SECRET_BETA")
                .unwrap_or_else(|_| "beta-secret-dev".into()),
            webhook_secret_gamma: std::env::var("WEBHOOK_SECRET_GAMMA")
                .unwrap_or_else(|_| "gamma-secret-dev".into()),
            max_retry_attempts: std::env::var("MAX_RETRY_ATTEMPTS")
                .unwrap_or_else(|_| "5".into())
                .parse()?,
            circuit_breaker_threshold: std::env::var("CIRCUIT_BREAKER_THRESHOLD")
                .unwrap_or_else(|_| "5".into())
                .parse()?,
            circuit_breaker_timeout_seconds: std::env::var("CIRCUIT_BREAKER_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "30".into())
                .parse()?,
        })
    }
}