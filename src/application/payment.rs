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
use crate::domain::repositories::{AuditLogRow, PaymentRepository, PaymentTransactionRepository};
use crate::providers::adapter::{PaymentProvider, ProviderRequest};
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Input for [`PaymentService::create_payment`], decoupled from the HTTP DTO
/// (`api::dto::payment::CreatePaymentRequest`) — `idempotency_key` in
/// particular comes from the `Idempotency-Key` header, not the JSON body.
#[derive(Debug, Clone)]
pub struct CreatePaymentInput {
    pub idempotency_key: String,
    pub merchant_reference: String,
    pub amount: i64,
    pub currency: String,
    pub description: Option<String>,
}

pub struct PaymentService {
    payment_repo: Arc<dyn PaymentRepository>,
    payment_tx: Arc<dyn PaymentTransactionRepository>,
    providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
}

impl PaymentService {
    pub fn new(
        payment_repo: Arc<dyn PaymentRepository>,
        payment_tx: Arc<dyn PaymentTransactionRepository>,
        providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
    ) -> Self {
        Self {
            payment_repo,
            payment_tx,
            providers,
        }
    }

    /// Validate, pick a provider, create the payment at the provider, then
    /// persist the payment + initial attempt + audit log atomically (single
    /// DB transaction via `PaymentTransactionRepository`).
    ///
    /// NOTE: provider selection here is a naive "first available" pick — a
    /// placeholder for the still-open P0 item "Provider selection by
    /// availability/priority" (circuit breaker awareness, priority ordering).
    ///
    /// NOTE: the `idempotency_keys` row is NOT part of this transaction —
    /// persisting it needs the request hash (computed in the idempotency
    /// middleware) and the serialized response body (built in the HTTP
    /// handler after this returns), neither of which this service has. That
    /// remains a separate follow-up.
    pub async fn create_payment(
        &self,
        merchant_id: Uuid,
        actor_api_key_id: Uuid,
        input: CreatePaymentInput,
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

        self.payment_tx
            .create_with_attempt_and_audit(&payment, &attempt, &audit)
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
    }

    impl FakePaymentRepository {
        fn new() -> Self {
            Self {
                get_by_id_result: Mutex::new(None),
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

        async fn update_status(
            &self,
            _id: Uuid,
            _status: &str,
            _failure_reason: Option<&str>,
        ) -> Result<(), DomainError> {
            unimplemented!("not exercised by these tests")
        }

        async fn search(
            &self,
            _criteria: &SearchCriteria,
        ) -> Result<PaginatedResult<PaymentSummaryRow>, DomainError> {
            unimplemented!("not exercised by these tests")
        }
    }

    struct FakePaymentTransactionRepository {
        calls: Mutex<Vec<(Payment, PaymentAttempt, AuditLogRow)>>,
        fail: bool,
    }

    impl FakePaymentTransactionRepository {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
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
        ) -> Result<(), DomainError> {
            if self.fail {
                return Err(DomainError::Validation("forced failure".into()));
            }
            self.calls
                .lock()
                .unwrap()
                .push((payment.clone(), attempt.clone(), audit.clone()));
            Ok(())
        }
    }

    struct FakeProvider {
        name: &'static str,
        available: bool,
        create_fn: Box<dyn Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync>,
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
            unimplemented!("not exercised by these tests")
        }
    }

    fn sample_input() -> CreatePaymentInput {
        CreatePaymentInput {
            idempotency_key: "checkout-order-10001".into(),
            merchant_reference: "ORDER-10001".into(),
            amount: 250_000,
            currency: "IDR".into(),
            description: Some("Test payment".into()),
        }
    }

    fn available_provider(
        create_fn: impl Fn() -> Result<ProviderResponse, ProviderError> + Send + Sync + 'static,
    ) -> Box<dyn PaymentProvider> {
        Box::new(FakeProvider {
            name: "MIDTRANS",
            available: true,
            create_fn: Box::new(create_fn),
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

        let service = PaymentService::new(payment_repo.clone(), payment_tx.clone(), providers);

        let merchant_id = Uuid::new_v4();
        let actor = Uuid::new_v4();
        let payment = service
            .create_payment(merchant_id, actor, sample_input())
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
        let (tx_payment, tx_attempt, tx_audit) = &calls[0];
        assert_eq!(tx_payment.id, payment.id);
        assert_eq!(tx_attempt.payment_id, payment.id);
        assert_eq!(tx_audit.actor, actor.to_string());
    }

    #[tokio::test]
    async fn create_payment_rejects_invalid_amount_without_touching_transaction() {
        let payment_repo = Arc::new(FakePaymentRepository::new());
        let payment_tx = Arc::new(FakePaymentTransactionRepository::new());
        let providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>> = Arc::new(RwLock::new(vec![]));

        let service = PaymentService::new(payment_repo.clone(), payment_tx.clone(), providers);

        let mut input = sample_input();
        input.amount = 0;

        let result = service
            .create_payment(Uuid::new_v4(), Uuid::new_v4(), input)
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
            })]));

        let service = PaymentService::new(payment_repo, payment_tx, providers);

        let result = service
            .create_payment(Uuid::new_v4(), Uuid::new_v4(), sample_input())
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

        let service = PaymentService::new(payment_repo, payment_tx.clone(), providers);

        let result = service
            .create_payment(Uuid::new_v4(), Uuid::new_v4(), sample_input())
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

        let service = PaymentService::new(payment_repo, payment_tx, providers);

        let result = service
            .create_payment(Uuid::new_v4(), Uuid::new_v4(), sample_input())
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

        let service = PaymentService::new(payment_repo, payment_tx, providers);

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

        let service = PaymentService::new(payment_repo, payment_tx, providers);

        let result = service.get_payment(Uuid::new_v4(), Uuid::new_v4()).await;

        assert!(matches!(
            result,
            Err(ApplicationError::Domain(DomainError::NotFound(_)))
        ));
    }
}
