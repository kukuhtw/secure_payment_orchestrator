//! Payment entity — core domain aggregate root.
//!
//! Representasi status pembayaran, nilai (amount + currency), dan lifecycle.
//! Domain layer PURE — tidak ada dependensi ke axum, sqlx, atau redis.

use crate::domain::status::PaymentStatus;
use crate::domain::error::DomainError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Payment {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub idempotency_key: String,
    pub merchant_reference: String,
    pub amount: Money,
    pub description: Option<String>,
    pub status: PaymentStatus,
    pub provider: Option<String>,
    pub payment_url: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct Money {
    pub amount: i64,
    pub currency: String,
}

impl Money {
    pub fn new(amount: i64, currency: impl Into<String>) -> Result<Self, DomainError> {
        let currency = currency.into();
        if amount <= 0 {
            return Err(DomainError::validation("Amount must be greater than 0"));
        }
        if currency.len() != 3 {
            return Err(DomainError::validation("Currency must be ISO 4217 (3 chars)"));
        }
        Ok(Self { amount, currency })
    }
}

impl Payment {
    pub fn new(
        merchant_id: Uuid,
        idempotency_key: String,
        merchant_reference: String,
        amount: Money,
        description: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            merchant_id,
            idempotency_key,
            merchant_reference,
            amount,
            description,
            status: PaymentStatus::Pending,
            provider: None,
            payment_url: None,
            failure_reason: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Transition payment to a new status, validating against transition rules.
    pub fn transition_to(&mut self, new_status: PaymentStatus) -> Result<(), DomainError> {
        crate::domain::rules::validate_transition(self.status, new_status)?;
        self.status = new_status;
        self.updated_at = Utc::now();

        if new_status.is_final() {
            self.completed_at = Some(Utc::now());
        }

        Ok(())
    }

    pub fn is_owner(&self, merchant_id: Uuid) -> bool {
        self.merchant_id == merchant_id
    }
}