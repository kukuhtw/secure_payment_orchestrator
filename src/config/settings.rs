//! Application configuration loaded from environment variables.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub server_port: u16,
    pub database_url: String,
    pub redis_url: String,
    pub log_level: String,
    pub midtrans_server_key: String,
    pub midtrans_snap_base_url: String,
    pub midtrans_core_base_url: String,
    pub midtrans_timeout_seconds: u64,
    pub xendit_secret_key: String,
    pub xendit_callback_token: String,
    pub xendit_base_url: String,
    pub xendit_timeout_seconds: u64,
    pub doku_client_id: String,
    pub doku_secret_key: String,
    pub doku_base_url: String,
    pub doku_timeout_seconds: u64,
    pub nicepay_imid: String,
    pub nicepay_merchant_key: String,
    pub nicepay_base_url: String,
    pub nicepay_pay_method: String,
    pub nicepay_timeout_seconds: u64,
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
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()),
            midtrans_server_key: std::env::var("MIDTRANS_SERVER_KEY").unwrap_or_default(),
            midtrans_snap_base_url: std::env::var("MIDTRANS_SNAP_BASE_URL")
                .unwrap_or_else(|_| "https://app.sandbox.midtrans.com".into()),
            midtrans_core_base_url: std::env::var("MIDTRANS_CORE_BASE_URL")
                .unwrap_or_else(|_| "https://api.sandbox.midtrans.com".into()),
            midtrans_timeout_seconds: std::env::var("MIDTRANS_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "10".into())
                .parse()?,
            xendit_secret_key: std::env::var("XENDIT_SECRET_KEY").unwrap_or_default(),
            xendit_callback_token: std::env::var("XENDIT_CALLBACK_TOKEN")
                .unwrap_or_default(),
            xendit_base_url: std::env::var("XENDIT_BASE_URL")
                .unwrap_or_else(|_| "https://api.xendit.co".into()),
            xendit_timeout_seconds: std::env::var("XENDIT_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "10".into())
                .parse()?,
            doku_client_id: std::env::var("DOKU_CLIENT_ID").unwrap_or_default(),
            doku_secret_key: std::env::var("DOKU_SECRET_KEY").unwrap_or_default(),
            doku_base_url: std::env::var("DOKU_BASE_URL")
                .unwrap_or_else(|_| "https://api-sandbox.doku.com".into()),
            doku_timeout_seconds: std::env::var("DOKU_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "10".into())
                .parse()?,
            nicepay_imid: std::env::var("NICEPAY_IMID").unwrap_or_default(),
            nicepay_merchant_key: std::env::var("NICEPAY_MERCHANT_KEY").unwrap_or_default(),
            nicepay_base_url: std::env::var("NICEPAY_BASE_URL")
                .unwrap_or_else(|_| "https://dev.nicepay.co.id".into()),
            nicepay_pay_method: std::env::var("NICEPAY_PAY_METHOD")
                .unwrap_or_else(|_| "01".into()),
            nicepay_timeout_seconds: std::env::var("NICEPAY_TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "10".into())
                .parse()?,
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
