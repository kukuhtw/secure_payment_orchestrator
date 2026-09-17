//! Repository traits — persistence contracts defined in domain layer.
//!
//! Domain layer mendefinisikan interface (trait), infrastruktur mengimplementasikannya.
//! Ini memastikan domain tidak bergantung pada framework atau database driver.

use async_trait::async_trait;
use uuid::Uuid;
use crate::domain::payment::Payment;
use crate::domain::attempt::PaymentAttempt;
use crate::domain::error::DomainError;

// ─── Merchant & API Key ─────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MerchantRow {
    pub id: Uuid,
    pub name: String,
    pub merchant_code: String,
    pub contact_email: String,
    pub status: String,
    pub supported_currencies: serde_json::Value,
    pub config: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub key_hash: String,
    pub key_prefix: String,
    pub label: Option<String>,
    pub permissions: String,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn find_by_key_prefix(&self, key_prefix: &str) -> Result<ApiKeyRow, DomainError>;
    async fn get_merchant(&self, merchant_id: Uuid) -> Result<MerchantRow, DomainError>;
}

// ─── Payment ────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PaymentRow {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub idempotency_key: String,
    pub merchant_reference: String,
    pub currency: String,
    pub amount: i64,
    pub description: Option<String>,
    pub status: String,
    pub provider: Option<String>,
    pub failure_reason: Option<String>,
    pub payment_url: Option<String>,
    pub created_by_api_key_id: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PaymentSummaryRow {
    pub id: Uuid,
    pub merchant_reference: String,
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub provider: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct SearchCriteria {
    pub merchant_id: Uuid,
    pub merchant_reference: Option<String>,
    pub status: Option<String>,
    pub provider: Option<String>,
    pub from_date: Option<chrono::DateTime<chrono::Utc>>,
    pub to_date: Option<chrono::DateTime<chrono::Utc>>,
    pub page: i64,
    pub limit: i64,
}

#[derive(Debug, Clone)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}
// ─── Payment Attempt ────────────────────────────────────

#[async_trait]
pub trait AttemptRepository: Send + Sync {
    async fn save(&self, attempt: &PaymentAttempt) -> Result<(), DomainError>;
    async fn get_by_payment_id(&self, payment_id: Uuid) -> Result<Vec<PaymentAttempt>, DomainError>;
    async fn count_attempts(&self, payment_id: Uuid) -> Result<i32, DomainError>;
}

// ─── Idempotency ────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdempotencyRow {
    pub idempotency_key: String,
    pub merchant_id: Uuid,
    pub request_hash: String,
    pub payment_id: Uuid,
    pub response_status_code: Option<String>,
    pub response_body: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait IdempotencyRepository: Send + Sync {
    async fn find_by_key(&self, key: &str, merchant_id: Uuid) -> Result<Option<IdempotencyRow>, DomainError>;
    async fn save(&self, row: &IdempotencyRow) -> Result<(), DomainError>;
}

// ─── Audit ──────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: Uuid,
    pub merchant_id: Option<Uuid>,
    pub payment_id: Option<Uuid>,
    pub entity_id: Option<Uuid>,
    pub entity_type: String,
    pub action: String,
    pub actor: String,
    pub field_name: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub ip_address: Option<String>,
    pub correlation_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn log(&self, row: &AuditLogRow) -> Result<(), DomainError>;
    async fn get_by_payment_id(&self, payment_id: Uuid) -> Result<Vec<AuditLogRow>, DomainError>;
}

// ─── Webhook ────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WebhookEventRow {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub provider: String,
    pub event_id: String,
    pub event_type: String,
    pub signature: String,
    pub raw_body: String,
    pub verification_status: String,
    pub processing_status: String,
    pub verification_error: Option<String>,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub processed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait WebhookEventRepository: Send + Sync {
    async fn save(&self, event: &WebhookEventRow) -> Result<(), DomainError>;
    async fn find_by_event_id(&self, provider: &str, event_id: &str) -> Result<Option<WebhookEventRow>, DomainError>;
    async fn update_processing_status(&self, id: Uuid, status: &str) -> Result<(), DomainError>;
}
#[async_trait]
pub trait PaymentRepository: Send + Sync {
    async fn create(&self, payment: &Payment) -> Result<(), DomainError>;
    async fn get_by_id(&self, id: Uuid, merchant_id: Uuid) -> Result<PaymentRow, DomainError>;
    async fn update_status(&self, id: Uuid, status: &str, failure_reason: Option<&str>) -> Result<(), DomainError>;
    async fn search(&self, criteria: &SearchCriteria) -> Result<PaginatedResult<PaymentSummaryRow>, DomainError>;
}
