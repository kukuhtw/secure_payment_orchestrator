//! Payment provider trait — adapter pattern untuk menormalkan perbedaan API provider.

use async_trait::async_trait;
use serde_json::Value;

/// Canonical error dari provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider returned error: {code} - {message}")]
    Provider { code: String, message: String, http_status: u16 },

    #[error("Request timeout after {ms}ms")]
    Timeout { ms: u64 },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Provider unavailable")]
    Unavailable,

    #[error("Invalid response format from provider")]
    InvalidResponse,
}

/// Canonical response dari provider.
#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub provider_payment_id: String,
    pub provider_status: String,
    pub payment_url: Option<String>,
    pub raw_response: Option<Value>,
}

/// Request yang dikirim ke provider.
#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub amount: i64,
    pub currency: String,
    pub merchant_reference: String,
    pub description: Option<String>,
    pub callback_url: Option<String>,
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    /// Nama provider (digunakan untuk routing dan logging).
    fn name(&self) -> &str;

    /// Apakah provider saat ini tersedia (circuit breaker check).
    fn is_available(&self) -> bool;

    /// Buat pembayaran di provider.
    async fn create_payment(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;

    /// Dapatkan status pembayaran dari provider.
    async fn get_payment_status(&self, provider_payment_id: &str) -> Result<ProviderResponse, ProviderError>;
}