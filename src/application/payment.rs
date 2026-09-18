//! Payment service — orchestrates payment creation, retrieval, and status updates.
//!
//! This layer sits between the HTTP handlers (`api::routes::payment`) and the
//! domain/infrastructure layers. It is deliberately decoupled from
//! `AppState`/`Repositories` — it takes only the collaborators it needs,
//! which also makes it unit-testable with fakes instead of a real
//! Postgres/provider connection.

use crate::application::ApplicationError;
use crate::domain::attempt::{AttemptStatus, AttemptType, PaymentAttempt};
use crate::domain::payment::{Money, Payment};
use crate::domain::repositories::{
    AttemptRepository, AuditLogRepository, AuditLogRow, IdempotencyRow, PaginatedResult,
    PaymentRepository, PaymentSummaryRow, PaymentTransactionRepository, ReconciliationRecordRow,
    SearchCriteria,
};
use crate::domain::rules::MAX_RETRY_ATTEMPTS;
use crate::domain::status::PaymentStatus;
use crate::providers::adapter::{PaymentProvider, ProviderRequest};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Result of [`PaymentService::reconcile_payment`] — enough for the HTTP
/// handler to build `api::dto::payment::ReconcileResponse` without reaching
/// back into the domain/infrastructure layers.
#[derive(Debug, Clone)]
pub struct ReconciliationOutcome {
    pub payment_id: Uuid,
    pub previous_status: PaymentStatus,
    pub current_status: PaymentStatus,
    pub provider_status: String,
    pub resolution: String,
    pub reconciled_at: DateTime<Utc>,
}

/// Input for [`PaymentService::search_payments`], decoupled from the HTTP
/// query-string DTO (`api::dto::payment::SearchPaymentParams`) — defaults
/// and bounds (e.g. page/limit) are the handler's job to apply before
/// calling this.
#[derive(Debug, Clone)]
pub struct SearchPaymentsFilter {
    pub merchant_reference: Option<String>,
    pub status: Option<String>,
    pub provider: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub page: i64,
    pub limit: i64,
}

/// Input for [`PaymentService::create_payment`], decoupled from the HTTP DTO
/// (`api::dto::payment::CreatePaymentRequest`) — `idempotency_key` and
/// `request_hash` both come from the idempotency middleware (the latter is
/// the SHA-256 hash of the raw request body it already computed), not the
/// JSON body itself.
#[derive(Debug, Clone)]
pub struct CreatePaymentInput {
    pub idempotency_key: String,
    pub request_hash: String,
    pub merchant_reference: String,
    pub amount: i64,
    pub currency: String,
    pub description: Option<String>,
}

pub struct PaymentService {
    payment_repo: Arc<dyn PaymentRepository>,
    payment_tx: Arc<dyn PaymentTransactionRepository>,
    attempt_repo: Arc<dyn AttemptRepository>,
    audit_repo: Arc<dyn AuditLogRepository>,
    providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
}

impl PaymentService {
    pub fn new(
        payment_repo: Arc<dyn PaymentRepository>,
        payment_tx: Arc<dyn PaymentTransactionRepository>,
        attempt_repo: Arc<dyn AttemptRepository>,
        audit_repo: Arc<dyn AuditLogRepository>,
        providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
    ) -> Self {
        Self {
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        }
    }

    /// Validate, pick a provider, create the payment at the provider, then
    /// persist the payment + initial attempt + audit log + idempotency
    /// record atomically (single DB transaction via
    /// `PaymentTransactionRepository`).
    ///
    /// NOTE: provider selection here is a naive "first available" pick — a
    /// placeholder for the still-open P0 item "Provider selection by
    /// availability/priority" (circuit breaker awareness, priority ordering).
    ///
    /// `response_snapshot` builds the JSON body to cache for idempotent
    /// replay (`IdempotencyRow::response_body`) from the finished-but-not-
    /// yet-persisted `Payment`. It's a caller-supplied closure rather than
    /// this layer building `api::dto::payment::PaymentResponse` itself,
    /// because `application` must not depend on `api` — the HTTP handler
    /// owns the response shape and hands this service an opaque JSON value.
    /// The handler is also responsible for keeping the closure's output in
    /// sync with whatever it actually returns on `201 Created`; nothing here
    /// enforces that at compile time.
    pub async fn create_payment(
        &self,
        merchant_id: Uuid,
        actor_api_key_id: Uuid,
        input: CreatePaymentInput,
        response_snapshot: impl FnOnce(&Payment) -> serde_json::Value,
    ) -> Result<Payment, ApplicationError> {
        let amount = Money::new(input.amount, input.currency)?;
        let mut payment = Payment::new(
            merchant_id,
            input.idempotency_key,
            input.merchant_reference,
            amount,
            input.description,
        );

        let providers = self.providers.read().await;
        let provider = providers
            .iter()
            .find(|p| p.is_available())
            .ok_or(ApplicationError::NoProviderAvailable)?;

        let provider_request = ProviderRequest {
            amount: payment.amount.amount,
            currency: payment.amount.currency.clone(),
            merchant_reference: payment.merchant_reference.clone(),
            description: payment.description.clone(),
            callback_url: None,
        };

        let provider_response = provider.create_payment(provider_request).await?;

        payment.provider = Some(provider.name().to_string());
        payment.payment_url = provider_response.payment_url.clone();

        let now = Utc::now();
        let attempt = PaymentAttempt {
            id: Uuid::new_v4(),
            payment_id: payment.id,
            provider: provider.name().to_string(),
            provider_payment_id: Some(provider_response.provider_payment_id.clone()),
            provider_status: Some(provider_response.provider_status.clone()),
            status: AttemptStatus::Success,
            attempt_type: AttemptType::Initial,
            attempt_number: 1,
            request_snapshot: None,
            response_snapshot: provider_response.raw_response.clone(),
            http_status_code: None,
            error_code: None,
            error_message: None,
            duration_ms: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
        };

        // Audit logging is done directly here for now — `application::audit`
        // (a dedicated service that could enrich this with correlation_id/
        // ip_address from request context) is still a skeleton.
        let audit = AuditLogRow {
            id: Uuid::new_v4(),
            merchant_id: Some(merchant_id),
            payment_id: Some(payment.id),
            entity_id: Some(payment.id),
            entity_type: "PAYMENT".into(),
            action: "CREATE".into(),
            actor: actor_api_key_id.to_string(),
            field_name: None,
            old_value: None,
            new_value: Some(payment.status.to_string()),
            metadata: None,
            ip_address: None,
            correlation_id: None,
            created_at: now,
        };

        let idempotency_row = IdempotencyRow {
            idempotency_key: payment.idempotency_key.clone(),
            merchant_id,
            request_hash: input.request_hash,
            payment_id: payment.id,
            // Matches the fixed `201 Created` the HTTP handler returns on
            // this path — create_payment only ever produces that status.
            response_status_code: Some("201".to_string()),
            response_body: Some(response_snapshot(&payment)),
            created_at: now,
            // Matches `idempotency_keys.expires_at`'s DB default (24h) — set
            // explicitly here rather than relying on it, consistent with
            // how the other insert_* helpers bind every column themselves.
            expires_at: now + chrono::Duration::hours(24),
        };

        self.payment_tx
            .create_with_attempt_and_audit(&payment, &attempt, &audit, Some(&idempotency_row))
            .await?;

        Ok(payment)
    }

    /// Fetch a payment, scoped to the authenticated merchant (the repository
    /// query itself filters by `merchant_id`, so a payment belonging to
    /// another merchant surfaces as `DomainError::NotFound`, not leaked).
    pub async fn get_payment(
        &self,
        merchant_id: Uuid,
        payment_id: Uuid,
    ) -> Result<Payment, ApplicationError> {
        let row = self.payment_repo.get_by_id(payment_id, merchant_id).await?;
        Ok(Payment::try_from(row)?)
    }

    /// List payments for the authenticated merchant matching `filter`.
    /// `merchant_id` scoping happens at the SQL level (`SearchCriteria`),
    /// same as `get_payment`.
    pub async fn search_payments(
        &self,
        merchant_id: Uuid,
        filter: SearchPaymentsFilter,
    ) -> Result<PaginatedResult<PaymentSummaryRow>, ApplicationError> {
        let criteria = SearchCriteria {
            merchant_id,
            merchant_reference: filter.merchant_reference,
            status: filter.status,
            provider: filter.provider,
            from_date: filter.from_date,
            to_date: filter.to_date,
            page: filter.page,
            limit: filter.limit,
        };

        Ok(self.payment_repo.search(&criteria).await?)
    }

    /// Cancel a payment: fetch (merchant-scoped), validate the state
    /// transition (`DomainError::AlreadyFinal` if it's already
    /// SUCCESS/FAILED/CANCELLED), persist the new status, and best-effort
    /// log an audit entry.
    ///
    /// NOTE: unlike `create_payment`, the status update and audit log are
    /// NOT wrapped in a transaction — `update_status` is a single UPDATE
    /// statement (atomic on its own), and losing the audit entry on a rare
    /// failure is a minor observability gap, not a data-integrity one (the
    /// cancellation itself either fully succeeded or fully failed).
    pub async fn cancel_payment(
        &self,
        merchant_id: Uuid,
        actor_api_key_id: Uuid,
        payment_id: Uuid,
        reason: Option<String>,
    ) -> Result<Payment, ApplicationError> {
        let row = self.payment_repo.get_by_id(payment_id, merchant_id).await?;
        let mut payment = Payment::try_from(row)?;

        payment.transition_to(PaymentStatus::Cancelled)?;
        if reason.is_some() {
            payment.failure_reason = reason;
        }

        self.payment_repo
            .update_status(
                payment.id,
                &payment.status.to_string(),
                payment.failure_reason.as_deref(),
                None,
            )
            .await?;

        let _ = self
            .audit_repo
            .log(&AuditLogRow {
                id: Uuid::new_v4(),
                merchant_id: Some(merchant_id),
                payment_id: Some(payment.id),
                entity_id: Some(payment.id),
                entity_type: "PAYMENT".into(),
                action: "CANCEL".into(),
                actor: actor_api_key_id.to_string(),
                field_name: Some("status".into()),
                old_value: None,
                new_value: Some(payment.status.to_string()),
                metadata: None,
                ip_address: None,
                correlation_id: None,
                created_at: Utc::now(),
            })
            .await;

        Ok(payment)
    }

    /// Manually retry a failed payment (operations-only, enforced by the
    /// caller via `MerchantContext::is_operations`). Merchant-unscoped —
    /// operations acts across all merchants, unlike the self-service
    /// `get_payment`/`cancel_payment`.
    ///
    /// Eligibility is `PaymentStatus::is_retryable()` (FAILED or
    /// PENDING_RETRY) — deliberately NOT the generic
    /// `domain::rules::validate_transition` state machine, which treats
    /// FAILED as final and would reject every retry. Bounded by
    /// `MAX_RETRY_ATTEMPTS` via `AttemptRepository::count_attempts`.
    ///
    /// NOTE: provider selection is the same naive "first available" as
    /// `create_payment` — no failover-aware selection yet. A provider
    /// failure during retry propagates without persisting anything (same
    /// as `create_payment`), so a failed retry attempt does NOT count
    /// against `MAX_RETRY_ATTEMPTS` (only previously-successful attempts
    /// are persisted and counted).
    pub async fn retry_payment(
        &self,
        actor_api_key_id: Uuid,
        payment_id: Uuid,
    ) -> Result<(Payment, i32), ApplicationError> {
        let row = self.payment_repo.get_by_id_unscoped(payment_id).await?;
        let payment = Payment::try_from(row)?;

        if !payment.status.is_retryable() {
            return Err(crate::domain::error::DomainError::invalid_transition(
                payment.status,
                PaymentStatus::Processing,
            )
            .into());
        }

        let attempt_count = self.attempt_repo.count_attempts(payment_id).await?;
        if attempt_count >= MAX_RETRY_ATTEMPTS {
            return Err(ApplicationError::MaxRetryReached(MAX_RETRY_ATTEMPTS));
        }

        let providers = self.providers.read().await;
        let provider = providers
            .iter()
            .find(|p| p.is_available())
            .ok_or(ApplicationError::NoProviderAvailable)?;

        let provider_request = ProviderRequest {
            amount: payment.amount.amount,
            currency: payment.amount.currency.clone(),
            merchant_reference: payment.merchant_reference.clone(),
            description: payment.description.clone(),
            callback_url: None,
        };

        let provider_response = provider.create_payment(provider_request).await?;

        let mut payment = payment;
        payment.provider = Some(provider.name().to_string());
        payment.status = PaymentStatus::Processing;
        payment.updated_at = Utc::now();

        let now = Utc::now();
        let attempt_number = attempt_count + 1;
        let attempt = PaymentAttempt {
            id: Uuid::new_v4(),
            payment_id: payment.id,
            provider: provider.name().to_string(),
            provider_payment_id: Some(provider_response.provider_payment_id.clone()),
            provider_status: Some(provider_response.provider_status.clone()),
            status: AttemptStatus::Success,
            attempt_type: AttemptType::Retry,
            attempt_number,
            request_snapshot: None,
            response_snapshot: provider_response.raw_response.clone(),
            http_status_code: None,
            error_code: None,
            error_message: None,
            duration_ms: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
        };

        let audit = AuditLogRow {
            id: Uuid::new_v4(),
            merchant_id: Some(payment.merchant_id),
            payment_id: Some(payment.id),
            entity_id: Some(payment.id),
            entity_type: "PAYMENT".into(),
            action: "RETRY".into(),
            actor: actor_api_key_id.to_string(),
            field_name: Some("status".into()),
            old_value: None,
            new_value: Some(payment.status.to_string()),
            metadata: None,
            ip_address: None,
            correlation_id: None,
            created_at: now,
        };

        self.payment_tx
            .update_status_with_attempt_and_audit(
                payment.id,
                &payment.status.to_string(),
                None,
                payment.provider.as_deref(),
                &attempt,
                &audit,
            )
            .await?;

        Ok((payment, attempt_number))
    }

    /// Reconcile a payment stuck in `PENDING_RECONCILIATION` by querying the
    /// provider that handled its most recent attempt. Operations-only,
    /// enforced by the caller via `MerchantContext::is_operations`.
    /// Merchant-unscoped, same reasoning as `retry_payment`.
    ///
    /// Eligibility is `PaymentStatus::needs_reconciliation()` (i.e. status ==
    /// `PENDING_RECONCILIATION`) — currently nothing in this codebase ever
    /// transitions a payment INTO that status (no timeout/retry-exhaustion
    /// worker exists yet), so this endpoint is correctly implemented but not
    /// yet reachable through any other flow. Documented as a known gap.
    ///
    /// Unlike `create_payment`/`retry_payment`, a provider error here does
    /// NOT propagate — reconciliation's whole purpose is to record what was
    /// learned (or that nothing could be learned), so a provider failure is
    /// captured as an `UNCERTAIN` resolution with the error in `details`
    /// rather than losing the fact that reconciliation was attempted.
    ///
    /// NOTE: the architecture doc's `reconcile:{payment_id}` Redis lock
    /// (preventing concurrent reconciliation of the same payment) is NOT
    /// implemented — `PaymentService` has no Redis dependency today, and
    /// adding one just for this would be a bigger structural change than
    /// this endpoint alone warrants.
    pub async fn reconcile_payment(
        &self,
        payment_id: Uuid,
    ) -> Result<ReconciliationOutcome, ApplicationError> {
        let row = self.payment_repo.get_by_id_unscoped(payment_id).await?;
        let payment = Payment::try_from(row)?;

        if !payment.status.needs_reconciliation() {
            return Err(ApplicationError::NotReconcilable(payment.status));
        }

        let attempts = self.attempt_repo.get_by_payment_id(payment_id).await?;
        let latest_attempt = attempts
            .into_iter()
            .max_by_key(|a| a.attempt_number)
            .ok_or_else(|| {
                ApplicationError::Domain(crate::domain::error::DomainError::Validation(
                    "Payment has no attempts to reconcile against".into(),
                ))
            })?;

        let provider_payment_id = latest_attempt.provider_payment_id.clone().ok_or_else(|| {
            ApplicationError::Domain(crate::domain::error::DomainError::Validation(
                "Latest attempt has no provider_payment_id".into(),
            ))
        })?;

        let providers = self.providers.read().await;
        let provider = providers
            .iter()
            .find(|p| p.name() == latest_attempt.provider)
            .ok_or(ApplicationError::NoProviderAvailable)?;

        let query_result = provider.get_payment_status(&provider_payment_id).await;

        let (resolved_status, resolution, provider_status, details) = match query_result {
            Ok(response) => {
                let (status, resolution) = match response.provider_status.as_str() {
                    "COMPLETED" => (Some(PaymentStatus::Success), "COMPLETED"),
                    "FAILED" => (Some(PaymentStatus::Failed), "FAILED"),
                    _ => (None, "UNCERTAIN"),
                };
                (
                    status,
                    resolution,
                    Some(response.provider_status.clone()),
                    response.raw_response.clone(),
                )
            }
            Err(err) => (
                None,
                "UNCERTAIN",
                None,
                Some(serde_json::json!({ "error": err.to_string() })),
            ),
        };

        let now = Utc::now();
        let resolved_status_str = resolved_status.map(|s| s.to_string());
        let record = ReconciliationRecordRow {
            id: Uuid::new_v4(),
            payment_id,
            payment_attempt_id: Some(latest_attempt.id),
            reconciliation_type: "MANUAL".into(),
            previous_status: payment.status.to_string(),
            current_status: resolved_status_str.clone(),
            provider_status,
            resolution: resolution.to_string(),
            details,
            created_at: now,
        };

        self.payment_tx
            .reconcile(payment_id, resolved_status_str.as_deref(), &record)
            .await?;

        Ok(ReconciliationOutcome {
            payment_id,
            previous_status: payment.status,
            current_status: resolved_status.unwrap_or(payment.status),
            provider_status: record.provider_status.clone().unwrap_or_default(),
            resolution: resolution.to_string(),
            reconciled_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::DomainError;
    use crate::domain::repositories::{
        PaginatedResult, PaymentRow, PaymentSummaryRow, SearchCriteria,
    };
    use crate::providers::adapter::{ProviderError, ProviderResponse};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FakePaymentRepository {
        get_by_id_result: Mutex<Option<Result<PaymentRow, ()>>>,
        update_status_calls: Mutex<Vec<(Uuid, String, Option<String>, Option<String>)>>,
        search_result: Mutex<Option<PaginatedResult<PaymentSummaryRow>>>,
    }

    impl FakePaymentRepository {
        fn new() -> Self {
            Self {
                get_by_id_result: Mutex::new(None),
                update_status_calls: Mutex::new(Vec::new()),
                search_result: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl PaymentRepository for FakePaymentRepository {
        async fn create(&self, _payment: &Payment) -> Result<(), DomainError> {
            unimplemented!("create_payment now goes through PaymentTransactionRepository")
        }

        async fn get_by_id(
            &self,
            _id: Uuid,
            _merchant_id: Uuid,
        ) -> Result<PaymentRow, DomainError> {
            match self.get_by_id_result.lock().unwrap().take() {
                Some(Ok(row)) => Ok(row),
                _ => Err(DomainError::NotFound("Payment not found".into())),
            }
        }

        async fn get_by_id_unscoped(&self, _id: Uuid) -> Result<PaymentRow, DomainError> {
            match self.get_by_id_result.lock().unwrap().take() {
                Some(Ok(row)) => Ok(row),
                _ => Err(DomainError::NotFound("Payment not found".into())),
            }
        }

        async fn update_status(
            &self,
            id: Uuid,
            status: &str,
            failure_reason: Option<&str>,
            provider: Option<&str>,
        ) -> Result<(), DomainError> {
            self.update_status_calls.lock().unwrap().push((
                id,
                status.to_string(),
                failure_reason.map(str::to_string),
                provider.map(str::to_string),
            ));
            Ok(())
        }

        async fn search(
            &self,
            _criteria: &SearchCriteria,
        ) -> Result<PaginatedResult<PaymentSummaryRow>, DomainError> {
            self.search_result
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| DomainError::Validation("no search_result configured".into()))
        }
    }

    struct FakeAttemptRepository {
        count: i32,
        attempts: Vec<PaymentAttempt>,
    }

    impl FakeAttemptRepository {
        fn new(count: i32) -> Self {
            Self {
                count,
                attempts: Vec::new(),
            }
        }

        fn with_attempts(count: i32, attempts: Vec<PaymentAttempt>) -> Self {
            Self { count, attempts }
        }
    }

    #[async_trait]
    impl AttemptRepository for FakeAttemptRepository {
        async fn save(&self, _attempt: &PaymentAttempt) -> Result<(), DomainError> {
            unimplemented!("not exercised by these tests")
        }

        async fn get_by_payment_id(
            &self,
            _payment_id: Uuid,
        ) -> Result<Vec<PaymentAttempt>, DomainError> {
            Ok(self.attempts.clone())
        }

        async fn count_attempts(&self, _payment_id: Uuid) -> Result<i32, DomainError> {
            Ok(self.count)
        }
    }

    struct FakeAuditLogRepository {
        logged: Mutex<Vec<AuditLogRow>>,
    }

    impl FakeAuditLogRepository {
        fn new() -> Self {
            Self {
                logged: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl AuditLogRepository for FakeAuditLogRepository {
        async fn log(&self, row: &AuditLogRow) -> Result<(), DomainError> {
            self.logged.lock().unwrap().push(row.clone());
            Ok(())
        }

        async fn get_by_payment_id(
            &self,
            _payment_id: Uuid,
        ) -> Result<Vec<AuditLogRow>, DomainError> {
            unimplemented!("not exercised by these tests")
        }
    }

    struct FakePaymentTransactionRepository {
        calls: Mutex<Vec<(Payment, PaymentAttempt, AuditLogRow, Option<IdempotencyRow>)>>,
        retry_calls: Mutex<
            Vec<(
                Uuid,
                String,
                Option<String>,
                Option<String>,
                PaymentAttempt,
                AuditLogRow,
            )>,
        >,
        reconcile_calls: Mutex<Vec<(Uuid, Option<String>, ReconciliationRecordRow)>>,
        fail: bool,
    }

    impl FakePaymentTransactionRepository {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                retry_calls: Mutex::new(Vec::new()),
                reconcile_calls: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                retry_calls: Mutex::new(Vec::new()),
                reconcile_calls: Mutex::new(Vec::new()),
                fail: true,
            }
        }
    }

    #[async_trait]
    impl PaymentTransactionRepository for FakePaymentTransactionRepository {
        async fn create_with_attempt_and_audit(
            &self,
            payment: &Payment,
            attempt: &PaymentAttempt,
            audit: &AuditLogRow,
            idempotency: Option<&IdempotencyRow>,
        ) -> Result<(), DomainError> {
            if self.fail {
                return Err(DomainError::Validation("forced failure".into()));
            }
            self.calls.lock().unwrap().push((
                payment.clone(),
                attempt.clone(),
                audit.clone(),
                idempotency.cloned(),
            ));
            Ok(())
        }

        async fn update_status_with_attempt_and_audit(
            &self,
            payment_id: Uuid,
            status: &str,
            failure_reason: Option<&str>,
            provider: Option<&str>,
            attempt: &PaymentAttempt,
            audit: &AuditLogRow,
        ) -> Result<(), DomainError> {
            if self.fail {
                return Err(DomainError::Validation("forced failure".into()));
            }
            self.retry_calls.lock().unwrap().push((
                payment_id,
                status.to_string(),
                failure_reason.map(str::to_string),
                provider.map(str::to_string),
                attempt.clone(),
                audit.clone(),
            ));
            Ok(())
        }

        async fn reconcile(
            &self,
            payment_id: Uuid,
            resolved_status: Option<&str>,
            record: &ReconciliationRecordRow,
        ) -> Result<(), DomainError> {
            if self.fail {
                return Err(DomainError::Validation("forced failure".into()));
            }
            self.reconcile_calls.lock().unwrap().push((
                payment_id,
                resolved_status.map(str::to_string),
                record.clone(),
            ));
            Ok(())
        }
    }

    struct FakeProvider {
        name: &'static str,
        available: bool,
        create_fn: Box<dyn Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync>,
        status_fn: Option<Box<dyn Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync>>,
    }

    #[async_trait]
    impl PaymentProvider for FakeProvider {
        fn name(&self) -> &str {
            self.name
        }

        fn is_available(&self) -> bool {
            self.available
        }

        async fn create_payment(
            &self,
            _request: ProviderRequest,
        ) -> Result<ProviderResponse, ProviderError> {
            (self.create_fn)()
        }

        async fn get_payment_status(
            &self,
            _provider_payment_id: &str,
        ) -> Result<ProviderResponse, ProviderError> {
            match &self.status_fn {
                Some(f) => f(),
                None => unimplemented!("not exercised by these tests"),
            }
        }
    }

    fn sample_input() -> CreatePaymentInput {
        CreatePaymentInput {
            idempotency_key: "checkout-order-10001".into(),
            request_hash: "a".repeat(64),
            merchant_reference: "ORDER-10001".into(),
            amount: 250_000,
            currency: "IDR".into(),
            description: Some("Test payment".into()),
        }
    }

    /// Matches the shape the real HTTP handler would build from
    /// `api::dto::payment::PaymentResponse` closely enough for tests — the
    /// exact fields don't matter here, only that a value is produced.
    fn fake_response_snapshot(payment: &Payment) -> serde_json::Value {
        serde_json::json!({"data": {"payment_id": payment.id.to_string()}})
    }

    fn available_provider(
        create_fn: impl Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync + 'static,
    ) -> Box<dyn PaymentProvider> {
        Box::new(FakeProvider {
            name: "MIDTRANS",
            available: true,
            create_fn: Box::new(create_fn),
            status_fn: None,
        })
    }

    fn provider_with_status(
        name: &'static str,
        status_fn: impl Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync + 'static,
    ) -> Box<dyn PaymentProvider> {
        Box::new(FakeProvider {
            name,
            available: true,
            create_fn: Box::new(|| unreachable!("reconcile must not call create_payment")),
            status_fn: Some(Box::new(status_fn)),
        })
    }

    #[tokio::test]
    async fn create_payment_persists_payment_attempt_and_audit_atomically() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![available_provider(|| {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_123".into(),
                    provider_status: "PENDING".into(),
                    payment_url: Some("https://pay.example/prov_123".into()),
                    raw_response: None,
                })
            })]));

        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let service = PaymentService::new(
            payment_repo.clone(),
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let merchant_id = Uuid::new_v4();
        let actor = Uuid::new_v4();
        let payment = service
            .create_payment(merchant_id, actor, sample_input(), fake_response_snapshot)
            .await
            .expect("create_payment should succeed");

        assert_eq!(payment.merchant_id, merchant_id);
        assert_eq!(payment.provider.as_deref(), Some("MIDTRANS"));
        assert_eq!(
            payment.payment_url.as_deref(),
            Some("https://pay.example/prov_123")
        );

        let calls = payment_tx.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let (tx_payment, tx_attempt, tx_audit, tx_idempotency) = &calls[0];
        assert_eq!(tx_payment.id, payment.id);
        assert_eq!(tx_attempt.payment_id, payment.id);
        assert_eq!(tx_audit.actor, actor.to_string());

        let idempotency = tx_idempotency
            .as_ref()
            .expect("idempotency row should be persisted atomically with the payment");
        assert_eq!(idempotency.idempotency_key, "checkout-order-10001");
        assert_eq!(idempotency.request_hash, "a".repeat(64));
        assert_eq!(idempotency.payment_id, payment.id);
        assert_eq!(idempotency.response_status_code.as_deref(), Some("201"));
        assert_eq!(
            idempotency.response_body,
            Some(serde_json::json!({"data": {"payment_id": payment.id.to_string()}}))
        );
    }

    #[tokio::test]
    async fn create_payment_rejects_invalid_amount_without_touching_transaction() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));

        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let service = PaymentService::new(
            payment_repo.clone(),
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let mut input = sample_input();
        input.amount = 0;

        let result = service
            .create_payment(
                Uuid::new_v4(),
                Uuid::new_v4(),
                input,
                fake_response_snapshot,
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Domain(_))));
        assert_eq!(payment_tx.calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_payment_fails_when_no_provider_available() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![Box::new(FakeProvider {
                name: "MIDTRANS",
                available: false,
                create_fn: Box::new(|| unreachable!("unavailable provider must not be called")),
                status_fn: None,
            })]));

        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .create_payment(
                Uuid::new_v4(),
                Uuid::new_v4(),
                sample_input(),
                fake_response_snapshot,
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::NoProviderAvailable)));
    }

    #[tokio::test]
    async fn create_payment_propagates_provider_error_without_persisting() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![available_provider(|| {
                Err(ProviderError::Unavailable)
            })]));

        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .create_payment(
                Uuid::new_v4(),
                Uuid::new_v4(),
                sample_input(),
                fake_response_snapshot,
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Provider(_))));
        assert_eq!(payment_tx.calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_payment_propagates_transaction_failure() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::failing());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![available_provider(|| {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_123".into(),
                    provider_status: "PENDING".into(),
                    payment_url: Some("https://pay.example/prov_123".into()),
                    raw_response: None,
                })
            })]));

        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .create_payment(
                Uuid::new_v4(),
                Uuid::new_v4(),
                sample_input(),
                fake_response_snapshot,
            )
            .await;

        assert!(matches!(result, Err(ApplicationError::Domain(_))));
    }

    #[tokio::test]
    async fn get_payment_converts_row_to_domain_payment() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let merchant_id = Uuid::new_v4();
        let payment_id = Uuid::new_v4();
        let now = Utc::now();

        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(PaymentRow {
            id: payment_id,
            merchant_id,
            idempotency_key: "checkout-order-10001".into(),
            merchant_reference: "ORDER-10001".into(),
            currency: "IDR".into(),
            amount: 250_000,
            description: None,
            status: "PENDING".into(),
            provider: Some("MIDTRANS".into()),
            failure_reason: None,
            payment_url: Some("https://pay.example/prov_123".into()),
            created_by_api_key_id: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));

        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let payment = service
            .get_payment(merchant_id, payment_id)
            .await
            .expect("get_payment should succeed");

        assert_eq!(payment.id, payment_id);
        assert_eq!(payment.amount.amount, 250_000);
    }

    #[tokio::test]
    async fn get_payment_propagates_not_found() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));

        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.get_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::NotFound(_)))
        ));
    }

    fn empty_filter() -> SearchPaymentsFilter {
        SearchPaymentsFilter {
            merchant_reference: None,
            status: None,
            provider: None,
            from_date: None,
            to_date: None,
            page: 1,
            limit: 20,
        }
    }

    #[tokio::test]
    async fn search_payments_returns_paginated_result() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let now = Utc::now();
        *payment_repo.search_result.lock().unwrap() = Some(PaginatedResult {
            items: vec![PaymentSummaryRow {
                id: Uuid::new_v4(),
                merchant_reference: "ORDER-10001".into(),
                status: "PENDING".into(),
                amount: 250_000,
                currency: "IDR".into(),
                provider: Some("MIDTRANS".into()),
                created_at: now,
            }],
            total: 1,
            page: 1,
            limit: 20,
        });

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .search_payments(Uuid::new_v4(), empty_filter())
            .await
            .expect("search_payments should succeed");

        assert_eq!(result.total, 1);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].merchant_reference, "ORDER-10001");
    }

    #[tokio::test]
    async fn cancel_payment_transitions_to_cancelled_and_logs_audit() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let merchant_id = Uuid::new_v4();
        let payment_id = Uuid::new_v4();
        let now = Utc::now();

        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(PaymentRow {
            id: payment_id,
            merchant_id,
            idempotency_key: "checkout-order-10001".into(),
            merchant_reference: "ORDER-10001".into(),
            currency: "IDR".into(),
            amount: 250_000,
            description: None,
            status: "PENDING".into(),
            provider: Some("MIDTRANS".into()),
            failure_reason: None,
            payment_url: Some("https://pay.example/prov_123".into()),
            created_by_api_key_id: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo.clone(),
            payment_tx,
            attempt_repo,
            audit_repo.clone(),
            providers,
        );

        let actor = Uuid::new_v4();
        let payment = service
            .cancel_payment(
                merchant_id,
                actor,
                payment_id,
                Some("Customer changed mind".into()),
            )
            .await
            .expect("cancel_payment should succeed");

        assert_eq!(payment.status, PaymentStatus::Cancelled);

        let update_calls = payment_repo.update_status_calls.lock().unwrap();
        assert_eq!(update_calls.len(), 1);
        assert_eq!(update_calls[0].0, payment_id);
        assert_eq!(update_calls[0].1, "CANCELLED");
        assert_eq!(update_calls[0].2.as_deref(), Some("Customer changed mind"));

        let logged = audit_repo.logged.lock().unwrap();
        assert_eq!(logged.len(), 1);
        assert_eq!(logged[0].action, "CANCEL");
        assert_eq!(logged[0].actor, actor.to_string());
    }

    #[tokio::test]
    async fn cancel_payment_rejects_already_final_payment() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let merchant_id = Uuid::new_v4();
        let payment_id = Uuid::new_v4();
        let now = Utc::now();

        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(PaymentRow {
            id: payment_id,
            merchant_id,
            idempotency_key: "checkout-order-10001".into(),
            merchant_reference: "ORDER-10001".into(),
            currency: "IDR".into(),
            amount: 250_000,
            description: None,
            status: "SUCCESS".into(),
            provider: Some("MIDTRANS".into()),
            failure_reason: None,
            payment_url: Some("https://pay.example/prov_123".into()),
            created_by_api_key_id: None,
            created_at: now,
            updated_at: now,
            completed_at: Some(now),
        }));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo.clone(),
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .cancel_payment(merchant_id, Uuid::new_v4(), payment_id, None)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::AlreadyFinal(
                PaymentStatus::Success
            )))
        ));
        assert_eq!(payment_repo.update_status_calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn cancel_payment_propagates_not_found() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service
            .cancel_payment(Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), None)
            .await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::NotFound(_)))
        ));
    }

    fn sample_payment_row(status: &str) -> PaymentRow {
        let now = Utc::now();
        PaymentRow {
            id: Uuid::new_v4(),
            merchant_id: Uuid::new_v4(),
            idempotency_key: "checkout-order-10001".into(),
            merchant_reference: "ORDER-10001".into(),
            currency: "IDR".into(),
            amount: 250_000,
            description: None,
            status: status.into(),
            provider: Some("MIDTRANS".into()),
            failure_reason: None,
            payment_url: Some("https://pay.example/prov_123".into()),
            created_by_api_key_id: None,
            created_at: now,
            updated_at: now,
            completed_at: None,
        }
    }

    #[tokio::test]
    async fn retry_payment_succeeds_for_failed_payment() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(sample_payment_row("FAILED")));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(1));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![available_provider(|| {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_456".into(),
                    provider_status: "PENDING".into(),
                    payment_url: Some("https://pay.example/prov_456".into()),
                    raw_response: None,
                })
            })]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let actor = Uuid::new_v4();
        let (payment, attempt_number) = service
            .retry_payment(actor, Uuid::new_v4())
            .await
            .expect("retry_payment should succeed");

        assert_eq!(payment.status, PaymentStatus::Processing);
        assert_eq!(payment.provider.as_deref(), Some("MIDTRANS"));
        assert_eq!(attempt_number, 2);

        let calls = payment_tx.retry_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "PROCESSING");
        assert_eq!(calls[0].4.attempt_number, 2);
        assert_eq!(calls[0].5.actor, actor.to_string());
    }

    #[tokio::test]
    async fn retry_payment_rejects_non_retryable_status() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(sample_payment_row("PENDING")));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.retry_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(
                DomainError::InvalidTransition { .. }
            ))
        ));
        assert_eq!(payment_tx.retry_calls.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn retry_payment_fails_when_max_attempts_reached() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(sample_payment_row("FAILED")));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(MAX_RETRY_ATTEMPTS));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.retry_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::MaxRetryReached(n)) if n == MAX_RETRY_ATTEMPTS
        ));
    }

    #[tokio::test]
    async fn retry_payment_fails_when_no_provider_available() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(sample_payment_row("FAILED")));

        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.retry_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(result, Err(ApplicationError::NoProviderAvailable)));
    }

    #[tokio::test]
    async fn retry_payment_propagates_not_found() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.retry_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::NotFound(_)))
        ));
    }

    fn sample_attempt(payment_id: Uuid, attempt_number: i32, provider: &str) -> PaymentAttempt {
        let now = Utc::now();
        PaymentAttempt {
            id: Uuid::new_v4(),
            payment_id,
            provider: provider.into(),
            provider_payment_id: Some("prov_999".into()),
            provider_status: Some("PENDING".into()),
            status: AttemptStatus::Success,
            attempt_type: AttemptType::Initial,
            attempt_number,
            request_snapshot: None,
            response_snapshot: None,
            http_status_code: None,
            error_code: None,
            error_message: None,
            duration_ms: None,
            started_at: Some(now),
            completed_at: Some(now),
            created_at: now,
        }
    }

    #[tokio::test]
    async fn reconcile_payment_resolves_to_success_when_provider_reports_completed() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_id = Uuid::new_v4();
        *payment_repo.get_by_id_result.lock().unwrap() =
            Some(Ok(sample_payment_row("PENDING_RECONCILIATION")));

        let attempt_repo = Arc::new(FakeAttemptRepository::with_attempts(
            1,
            vec![sample_attempt(payment_id, 1, "MIDTRANS")],
        ));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![provider_with_status("MIDTRANS", || {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_999".into(),
                    provider_status: "COMPLETED".into(),
                    payment_url: None,
                    raw_response: None,
                })
            })]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let outcome = service
            .reconcile_payment(Uuid::new_v4())
            .await
            .expect("reconcile_payment should succeed");

        assert_eq!(outcome.current_status, PaymentStatus::Success);
        assert_eq!(outcome.resolution, "COMPLETED");

        let calls = payment_tx.reconcile_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1.as_deref(), Some("SUCCESS"));
        assert_eq!(calls[0].2.resolution, "COMPLETED");
    }

    #[tokio::test]
    async fn reconcile_payment_resolves_to_failed_when_provider_reports_failed() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_id = Uuid::new_v4();
        *payment_repo.get_by_id_result.lock().unwrap() =
            Some(Ok(sample_payment_row("PENDING_RECONCILIATION")));

        let attempt_repo = Arc::new(FakeAttemptRepository::with_attempts(
            1,
            vec![sample_attempt(payment_id, 1, "MIDTRANS")],
        ));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![provider_with_status("MIDTRANS", || {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_999".into(),
                    provider_status: "FAILED".into(),
                    payment_url: None,
                    raw_response: None,
                })
            })]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let outcome = service
            .reconcile_payment(Uuid::new_v4())
            .await
            .expect("reconcile_payment should succeed");

        assert_eq!(outcome.current_status, PaymentStatus::Failed);
        assert_eq!(outcome.resolution, "FAILED");
    }

    #[tokio::test]
    async fn reconcile_payment_stays_uncertain_when_provider_reports_pending() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_id = Uuid::new_v4();
        *payment_repo.get_by_id_result.lock().unwrap() =
            Some(Ok(sample_payment_row("PENDING_RECONCILIATION")));

        let attempt_repo = Arc::new(FakeAttemptRepository::with_attempts(
            1,
            vec![sample_attempt(payment_id, 1, "MIDTRANS")],
        ));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![provider_with_status("MIDTRANS", || {
                Ok(ProviderResponse {
                    provider_payment_id: "prov_999".into(),
                    provider_status: "PENDING".into(),
                    payment_url: None,
                    raw_response: None,
                })
            })]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx.clone(),
            attempt_repo,
            audit_repo,
            providers,
        );

        let outcome = service
            .reconcile_payment(Uuid::new_v4())
            .await
            .expect("reconcile_payment should succeed");

        assert_eq!(outcome.current_status, PaymentStatus::PendingReconciliation);
        assert_eq!(outcome.resolution, "UNCERTAIN");

        let calls = payment_tx.reconcile_calls.lock().unwrap();
        assert_eq!(calls[0].1, None); // no status update — still uncertain
    }

    #[tokio::test]
    async fn reconcile_payment_records_uncertain_on_provider_error() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_id = Uuid::new_v4();
        *payment_repo.get_by_id_result.lock().unwrap() =
            Some(Ok(sample_payment_row("PENDING_RECONCILIATION")));

        let attempt_repo = Arc::new(FakeAttemptRepository::with_attempts(
            1,
            vec![sample_attempt(payment_id, 1, "MIDTRANS")],
        ));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> =
            Arc::new(RwLock::new(vec![provider_with_status("MIDTRANS", || {
                Err(ProviderError::Unavailable)
            })]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let outcome = service
            .reconcile_payment(Uuid::new_v4())
            .await
            .expect("provider errors should not fail reconcile_payment");

        assert_eq!(outcome.resolution, "UNCERTAIN");
        assert_eq!(outcome.current_status, PaymentStatus::PendingReconciliation);
    }

    #[tokio::test]
    async fn reconcile_payment_rejects_non_reconcilable_status() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        *payment_repo.get_by_id_result.lock().unwrap() = Some(Ok(sample_payment_row("PENDING")));

        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.reconcile_payment(Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::NotReconcilable(PaymentStatus::Pending))
        ));
    }

    #[tokio::test]
    async fn reconcile_payment_propagates_not_found() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let attempt_repo = Arc::new(FakeAttemptRepository::new(0));
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let audit_repo = Arc::new(FakeAuditLogRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));
        let service = PaymentService::new(
            payment_repo,
            payment_tx,
            attempt_repo,
            audit_repo,
            providers,
        );

        let result = service.reconcile_payment(Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::NotFound(_)))
        ));
    }
}
