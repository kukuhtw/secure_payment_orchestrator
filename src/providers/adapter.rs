//! Payment provider trait — adapter pattern untuk menormalkan perbedaan API provider.

use async_trait::async_trait;
use serde_json::Value;

/// Canonical error dari provider.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Provider returned error: {code} - {message}")]
    Provider {
        code: String,
        message: String,
        http_status: u16,
    },

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

    /// Apakah provider saat ini tersedia. Setiap adapter (Alpha/Beta/Gamma/
    /// Nicepay) sendiri hanya cek konfigurasi (mis. Server Key kosong) —
    /// TIDAK tahu apa pun soal circuit breaker. `providers::build_providers`
    /// membungkus tiap adapter dengan `circuit_breaker::CircuitBreakerProvider`
    /// (dekorator, lihat file itu), yang menggabungkan cek konfigurasi ini
    /// DENGAN state CLOSED/OPEN/HALF_OPEN — jadi provider yang dipakai
    /// `PaymentService` (`select_best_provider`) sudah circuit-breaker-aware,
    /// walau trait method ini sendiri, dilihat dari adapter mentah, belum.
    fn is_available(&self) -> bool;

    /// Prioritas seleksi di antara provider yang `is_available()` — nilai
    /// lebih kecil dipilih lebih dulu. Dibaca dari konfigurasi
    /// (`Settings::*_priority`) lewat `providers::build_providers`, bukan
    /// hardcoded di sini, supaya urutan bisa diatur per-deployment tanpa
    /// mengubah kode. Lihat `PaymentService::create_payment`/`retry_payment`
    /// untuk cara ini dipakai.
    fn priority(&self) -> i32;

    /// Buat pembayaran di provider.
    async fn create_payment(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError>;

    /// Dapatkan status pembayaran dari provider.
    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError>;
}
