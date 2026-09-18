//! Full SQLx repository implementations for all domain repository traits.
//!
//! Semua query menggunakan parameterized query ($1, $2, ...) untuk SQL injection prevention.

use crate::domain::attempt::{PaymentAttempt, PaymentAttemptRow};
use crate::domain::error::DomainError;
use crate::domain::payment::Payment;
use crate::domain::repositories::*;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

// ─── API Key Repository ─────────────────────────────────

pub struct PgApiKeyRepository {
    pool: PgPool,
}

impl PgApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyRepository for PgApiKeyRepository {
    async fn find_by_key_prefix(&self, key_prefix: &str) -> Result<ApiKeyRow, DomainError> {
        sqlx::query_as::<_, ApiKeyRow>(
            r#"SELECT id, merchant_id, key_hash, key_prefix, label, permissions,
                      expires_at, revoked_at
               FROM api_keys WHERE key_prefix = $1
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > NOW())"#,
        )
        .bind(key_prefix)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound("API key not found or expired".into()))
    }

    async fn get_merchant(&self, merchant_id: Uuid) -> Result<MerchantRow, DomainError> {
        sqlx::query_as::<_, MerchantRow>(
            r#"SELECT id, name, merchant_code, contact_email, status,
                      supported_currencies, config, created_at, updated_at
               FROM merchants WHERE id = $1 AND status = 'ACTIVE'"#,
        )
        .bind(merchant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound("Merchant not found or inactive".into()))
    }
}
// ─── Payment Repository ─────────────────────────────────

pub struct PgPaymentRepository {
    pool: PgPool,
}

impl PgPaymentRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Shared by `PgPaymentRepository::create` (single `&PgPool`) and
/// `PgPaymentTransactionRepository::create_with_attempt_and_audit` (an open
/// `Transaction`) — anything implementing `sqlx::Executor` works.
async fn insert_payment<'e, E>(executor: E, payment: &Payment) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r#"INSERT INTO payments (id, merchant_id, idempotency_key, merchant_reference,
                 currency, amount, description, status, provider, payment_url,
                 created_by_api_key_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
    )
    .bind(payment.id)
    .bind(payment.merchant_id)
    .bind(&payment.idempotency_key)
    .bind(&payment.merchant_reference)
    .bind(&payment.amount.currency)
    .bind(payment.amount.amount)
    .bind(&payment.description)
    .bind(payment.status.to_string())
    .bind(&payment.provider)
    .bind(&payment.payment_url)
    .bind(None::<Uuid>)
    .bind(payment.created_at)
    .bind(payment.updated_at)
    .execute(executor)
    .await?;
    Ok(())
}

fn map_payment_insert_error(e: sqlx::Error) -> DomainError {
    if let sqlx::Error::Database(ref db) = &e {
        if db.constraint() == Some("uq_payments_idempotency") {
            return DomainError::conflict("Idempotency key already exists");
        }
    }
    DomainError::Validation(e.to_string())
}

/// Shared by `PgAttemptRepository::save` and
/// `PgPaymentTransactionRepository::create_with_attempt_and_audit`.
async fn insert_attempt<'e, E>(executor: E, attempt: &PaymentAttempt) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r#"INSERT INTO payment_attempts (id, payment_id, provider, provider_payment_id,
                 provider_status, status, attempt_type, attempt_number,
                 request_snapshot, response_snapshot, http_status_code,
                 error_code, error_message, duration_ms, started_at,
                 completed_at, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"#,
    )
    .bind(attempt.id)
    .bind(attempt.payment_id)
    .bind(&attempt.provider)
    .bind(&attempt.provider_payment_id)
    .bind(&attempt.provider_status)
    .bind(attempt.status.to_string())
    .bind(attempt.attempt_type.to_string())
    .bind(attempt.attempt_number)
    .bind(&attempt.request_snapshot)
    .bind(&attempt.response_snapshot)
    .bind(attempt.http_status_code)
    .bind(&attempt.error_code)
    .bind(&attempt.error_message)
    .bind(attempt.duration_ms)
    .bind(attempt.started_at)
    .bind(attempt.completed_at)
    .bind(attempt.created_at)
    .execute(executor)
    .await?;
    Ok(())
}

/// Shared by `PgAuditLogRepository::log` and
/// `PgPaymentTransactionRepository::create_with_attempt_and_audit`.
async fn insert_audit_log<'e, E>(executor: E, row: &AuditLogRow) -> Result<(), sqlx::Error>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r#"INSERT INTO audit_logs (id, merchant_id, payment_id, entity_id, entity_type,
                 action, actor, field_name, old_value, new_value, metadata,
                 ip_address, correlation_id, created_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
    )
    .bind(row.id)
    .bind(row.merchant_id)
    .bind(row.payment_id)
    .bind(row.entity_id)
    .bind(&row.entity_type)
    .bind(&row.action)
    .bind(&row.actor)
    .bind(&row.field_name)
    .bind(&row.old_value)
    .bind(&row.new_value)
    .bind(&row.metadata)
    .bind(&row.ip_address)
    .bind(&row.correlation_id)
    .bind(row.created_at)
    .execute(executor)
    .await?;
    Ok(())
}

#[async_trait]
impl PaymentRepository for PgPaymentRepository {
    async fn create(&self, payment: &Payment) -> Result<(), DomainError> {
        insert_payment(&self.pool, payment)
            .await
            .map_err(map_payment_insert_error)
    }

    async fn get_by_id(&self, id: Uuid, merchant_id: Uuid) -> Result<PaymentRow, DomainError> {
        sqlx::query_as::<_, PaymentRow>(
            r#"SELECT id, merchant_id, idempotency_key, merchant_reference, currency,
                      amount, description, status, provider, failure_reason, payment_url,
                      created_by_api_key_id, created_at, updated_at, completed_at
               FROM payments WHERE id = $1 AND merchant_id = $2"#,
        )
        .bind(id)
        .bind(merchant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound("Payment not found".into()))
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        failure_reason: Option<&str>,
    ) -> Result<(), DomainError> {
        let completed_at =
            matches!(status, "SUCCESS" | "FAILED" | "CANCELLED").then(|| chrono::Utc::now());

        sqlx::query(
            r#"UPDATE payments SET status = $1, failure_reason = $2,
                      completed_at = COALESCE($3, completed_at),
                      updated_at = NOW() WHERE id = $4"#,
        )
        .bind(status)
        .bind(failure_reason)
        .bind(completed_at)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;
        Ok(())
    }

    async fn search(
        &self,
        criteria: &SearchCriteria,
    ) -> Result<PaginatedResult<PaymentSummaryRow>, DomainError> {
        let offset = (criteria.page - 1) * criteria.limit;

        let items = sqlx::query_as::<_, PaymentSummaryRow>(
            r#"SELECT id, merchant_reference, status, amount, currency, provider, created_at
               FROM payments
               WHERE merchant_id = $1
                 AND ($2::text IS NULL OR merchant_reference = $2)
                 AND ($3::text IS NULL OR status = $3)
                 AND ($4::text IS NULL OR provider = $4)
                 AND ($5::timestamptz IS NULL OR created_at >= $5)
                 AND ($6::timestamptz IS NULL OR created_at <= $6)
               ORDER BY created_at DESC
               LIMIT $7 OFFSET $8"#,
        )
        .bind(criteria.merchant_id)
        .bind(&criteria.merchant_reference)
        .bind(&criteria.status)
        .bind(&criteria.provider)
        .bind(criteria.from_date)
        .bind(criteria.to_date)
        .bind(criteria.limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;

        let (total,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM payments WHERE merchant_id = $1
               AND ($2::text IS NULL OR merchant_reference = $2)
               AND ($3::text IS NULL OR status = $3)
               AND ($4::text IS NULL OR provider = $4)
               AND ($5::timestamptz IS NULL OR created_at >= $5)
               AND ($6::timestamptz IS NULL OR created_at <= $6)"#,
        )
        .bind(criteria.merchant_id)
        .bind(&criteria.merchant_reference)
        .bind(&criteria.status)
        .bind(&criteria.provider)
        .bind(criteria.from_date)
        .bind(criteria.to_date)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;

        Ok(PaginatedResult {
            items,
            total,
            page: criteria.page,
            limit: criteria.limit,
        })
    }
}
// ─── Attempt Repository ─────────────────────────────────

pub struct PgAttemptRepository {
    pool: PgPool,
}

impl PgAttemptRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AttemptRepository for PgAttemptRepository {
    async fn save(&self, attempt: &PaymentAttempt) -> Result<(), DomainError> {
        insert_attempt(&self.pool, attempt)
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))
    }

    async fn get_by_payment_id(
        &self,
        payment_id: Uuid,
    ) -> Result<Vec<PaymentAttempt>, DomainError> {
        let rows = sqlx::query_as::<_, PaymentAttemptRow>(
            r#"SELECT id, payment_id, provider, provider_payment_id, provider_status,
                      status, attempt_type, attempt_number, request_snapshot, response_snapshot,
                      http_status_code, error_code, error_message, duration_ms,
                      started_at, completed_at, created_at
               FROM payment_attempts WHERE payment_id = $1
               ORDER BY attempt_number ASC"#,
        )
        .bind(payment_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    async fn count_attempts(&self, payment_id: Uuid) -> Result<i32, DomainError> {
        let (count,): (i32,) =
            sqlx::query_as(r#"SELECT COUNT(*) FROM payment_attempts WHERE payment_id = $1"#)
                .bind(payment_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::Validation(e.to_string()))?;
        Ok(count)
    }
}

// ─── Idempotency Repository ─────────────────────────────

pub struct PgIdempotencyRepository {
    pool: PgPool,
}

impl PgIdempotencyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
// ─── Audit Log Repository ───────────────────────────────

pub struct PgAuditLogRepository {
    pool: PgPool,
}

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn log(&self, row: &AuditLogRow) -> Result<(), DomainError> {
        insert_audit_log(&self.pool, row)
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))
    }

    async fn get_by_payment_id(&self, payment_id: Uuid) -> Result<Vec<AuditLogRow>, DomainError> {
        sqlx::query_as::<_, AuditLogRow>(
            r#"SELECT * FROM audit_logs WHERE payment_id = $1
               ORDER BY created_at DESC"#,
        )
        .bind(payment_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))
    }
}

// ─── Webhook Event Repository ───────────────────────────

pub struct PgWebhookEventRepository {
    pool: PgPool,
}

impl PgWebhookEventRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WebhookEventRepository for PgWebhookEventRepository {
    async fn save(&self, event: &WebhookEventRow) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO webhook_events (id, payment_id, provider, event_id, event_type,
                     signature, raw_body, verification_status, processing_status,
                     verification_error, received_at, verified_at, processed_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        )
        .bind(event.id)
        .bind(event.payment_id)
        .bind(&event.provider)
        .bind(&event.event_id)
        .bind(&event.event_type)
        .bind(&event.signature)
        .bind(&event.raw_body)
        .bind(&event.verification_status)
        .bind(&event.processing_status)
        .bind(&event.verification_error)
        .bind(event.received_at)
        .bind(event.verified_at)
        .bind(event.processed_at)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db) = &e {
                if db.constraint() == Some("uq_webhook_events_provider_event") {
                    return DomainError::conflict("Duplicate webhook event");
                }
            }
            DomainError::Validation(e.to_string())
        })?;
        Ok(())
    }

    async fn find_by_event_id(
        &self,
        provider: &str,
        event_id: &str,
    ) -> Result<Option<WebhookEventRow>, DomainError> {
        sqlx::query_as::<_, WebhookEventRow>(
            r#"SELECT * FROM webhook_events
               WHERE provider = $1 AND event_id = $2"#,
        )
        .bind(provider)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))
    }

    async fn update_processing_status(&self, id: Uuid, status: &str) -> Result<(), DomainError> {
        sqlx::query(
            r#"UPDATE webhook_events SET processing_status = $1,
                      processed_at = CASE WHEN $1 = 'PROCESSED' THEN NOW() ELSE processed_at END
               WHERE id = $2"#,
        )
        .bind(status)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;
        Ok(())
    }
}

/// Check database connectivity (utility function).
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

#[async_trait]
impl IdempotencyRepository for PgIdempotencyRepository {
    async fn find_by_key(
        &self,
        key: &str,
        merchant_id: Uuid,
    ) -> Result<Option<IdempotencyRow>, DomainError> {
        sqlx::query_as::<_, IdempotencyRow>(
            r#"SELECT idempotency_key, merchant_id, request_hash, payment_id,
                      response_status_code, response_body, created_at, expires_at
               FROM idempotency_keys
               WHERE idempotency_key = $1 AND merchant_id = $2
               AND expires_at > NOW()"#,
        )
        .bind(key)
        .bind(merchant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))
    }

    async fn save(&self, row: &IdempotencyRow) -> Result<(), DomainError> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (idempotency_key, merchant_id, request_hash,
                     payment_id, response_status_code, response_body, created_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
        )
        .bind(&row.idempotency_key)
        .bind(row.merchant_id)
        .bind(&row.request_hash)
        .bind(row.payment_id)
        .bind(&row.response_status_code)
        .bind(&row.response_body)
        .bind(row.created_at)
        .bind(row.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::Validation(e.to_string()))?;
        Ok(())
    }
}

// ─── Payment Transaction Repository (atomic create) ─────

pub struct PgPaymentTransactionRepository {
    pool: PgPool,
}

impl PgPaymentTransactionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PaymentTransactionRepository for PgPaymentTransactionRepository {
    async fn create_with_attempt_and_audit(
        &self,
        payment: &Payment,
        attempt: &PaymentAttempt,
        audit: &AuditLogRow,
    ) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))?;

        insert_payment(&mut *tx, payment)
            .await
            .map_err(map_payment_insert_error)?;

        insert_attempt(&mut *tx, attempt)
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))?;

        insert_audit_log(&mut *tx, audit)
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DomainError::Validation(e.to_string()))?;

        Ok(())
    }
}
