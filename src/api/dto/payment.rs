//! Payment-related DTOs (request & response).

use crate::api::dto::error::ApiError;
use crate::domain::payment::Payment;
use axum::http::StatusCode;
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// `merchant_reference` column is `VARCHAR(255)`.
const MERCHANT_REFERENCE_MAX_LEN: usize = 255;
/// No DB limit on `description` (`TEXT`) — this is an operational sanity cap.
const DESCRIPTION_MAX_LEN: usize = 1000;

// ─── Create Payment ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreatePaymentRequest {
    pub merchant_reference: String,
    pub amount: i64,
    pub currency: String,
    pub customer: Option<CustomerInfo>,
    pub description: Option<String>,
    pub metadata: Option<Value>,
}

impl CreatePaymentRequest {
    /// Structural validation beyond what `Money::new` already enforces
    /// (`amount > 0`, `currency` length == 3) — required-field, length,
    /// and light format checks that give a field-specific error instead of
    /// a generic domain validation message.
    ///
    /// NOTE: this does not check `currency` against the authenticated
    /// merchant's `supported_currencies` (per-merchant, stored in the DB) —
    /// that's a business rule for `PaymentService`, not request shape.
    pub fn validate(&self) -> Result<(), ApiError> {
        let mut details: HashMap<String, Value> = HashMap::new();

        let merchant_reference = self.merchant_reference.trim();
        if merchant_reference.is_empty() {
            details.insert(
                "merchant_reference".into(),
                Value::String("must not be empty".into()),
            );
        } else if merchant_reference.len() > MERCHANT_REFERENCE_MAX_LEN {
            details.insert(
                "merchant_reference".into(),
                Value::String(format!(
                    "must be at most {MERCHANT_REFERENCE_MAX_LEN} characters"
                )),
            );
        }

        if self.amount <= 0 {
            details.insert(
                "amount".into(),
                Value::String("must be greater than 0".into()),
            );
        }

        if self.currency.len() != 3 || !self.currency.bytes().all(|b| b.is_ascii_uppercase()) {
            details.insert(
                "currency".into(),
                Value::String("must be a 3-letter uppercase ISO 4217 code".into()),
            );
        }

        if let Some(description) = &self.description {
            if description.len() > DESCRIPTION_MAX_LEN {
                details.insert(
                    "description".into(),
                    Value::String(format!("must be at most {DESCRIPTION_MAX_LEN} characters")),
                );
            }
        }

        if let Some(email) = self.customer.as_ref().and_then(|c| c.email.as_deref()) {
            if !is_plausible_email(email) {
                details.insert(
                    "customer.email".into(),
                    Value::String("must be a valid email address".into()),
                );
            }
        }

        if details.is_empty() {
            Ok(())
        } else {
            Err(ApiError::with_details(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Request validation failed",
                details,
            ))
        }
    }
}

/// Deliberately loose — a sanity check, not full RFC 5322 validation.
fn is_plausible_email(value: &str) -> bool {
    match value.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        None => false,
    }
}

#[derive(Debug, Deserialize)]
pub struct CustomerInfo {
    pub name: Option<String>,
    pub email: Option<String>,
}

// ─── Cancel Payment ─────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CancelPaymentRequest {
    pub reason: Option<String>,
}

// ─── Search Payments ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchPaymentParams {
    pub merchant_reference: Option<String>,
    pub status: Option<String>,
    pub provider: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub page: Option<i32>,
    pub limit: Option<i32>,
}

// ─── Responses ──────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    pub data: PaymentData,
}

#[derive(Debug, Serialize)]
pub struct PaymentData {
    pub payment_id: String,
    pub merchant_reference: String,
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub provider: String,
    pub payment_url: Option<String>,
    pub attempts: Option<Vec<AttemptData>>,
    pub webhook_events: Option<Vec<WebhookEventData>>,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AttemptData {
    pub attempt_number: i32,
    pub provider: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub attempt_type: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebhookEventData {
    pub event_id: String,
    pub event_type: String,
    pub verification_status: String,
    pub processing_status: String,
    pub received_at: String,
}

#[derive(Debug, Serialize)]
pub struct SearchPaymentsResponse {
    pub data: SearchPaymentsData,
}

#[derive(Debug, Serialize)]
pub struct SearchPaymentsData {
    pub payments: Vec<PaymentSummary>,
    pub pagination: Pagination,
}

#[derive(Debug, Serialize)]
pub struct PaymentSummary {
    pub payment_id: String,
    pub merchant_reference: String,
    pub status: String,
    pub amount: i64,
    pub currency: String,
    pub provider: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct Pagination {
    pub page: i32,
    pub limit: i32,
    pub total_items: i64,
    pub total_pages: i32,
}

#[derive(Debug, Serialize)]
pub struct ReconcileResponse {
    pub data: ReconcileData,
}

#[derive(Debug, Serialize)]
pub struct ReconcileData {
    pub payment_id: String,
    pub previous_status: String,
    pub current_status: String,
    pub provider_status: String,
    pub resolution: String,
    pub reconciled_at: String,
}

impl From<Payment> for PaymentData {
    fn from(payment: Payment) -> Self {
        Self {
            payment_id: payment.id.to_string(),
            merchant_reference: payment.merchant_reference,
            status: payment.status.to_string(),
            amount: payment.amount.amount,
            currency: payment.amount.currency,
            provider: payment.provider.unwrap_or_default(),
            payment_url: payment.payment_url,
            // Attempt/webhook-event enrichment on GET is a follow-up — the
            // application service doesn't fetch those yet (see PaymentService).
            attempts: None,
            webhook_events: None,
            created_at: payment
                .created_at
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            updated_at: Some(
                payment
                    .updated_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            ),
            completed_at: payment
                .completed_at
                .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true)),
        }
    }
}

impl From<Payment> for PaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            data: payment.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::payment::Money;
    use uuid::Uuid;

    fn valid_request() -> CreatePaymentRequest {
        CreatePaymentRequest {
            merchant_reference: "ORDER-10001".into(),
            amount: 250_000,
            currency: "IDR".into(),
            customer: Some(CustomerInfo {
                name: Some("Budi Santoso".into()),
                email: Some("budi@example.com".into()),
            }),
            description: Some("Pembayaran ORDER-10001".into()),
            metadata: None,
        }
    }

    #[test]
    fn valid_request_passes() {
        assert!(valid_request().validate().is_ok());
    }

    #[test]
    fn rejects_empty_merchant_reference() {
        let mut req = valid_request();
        req.merchant_reference = "   ".into();

        let err = req.validate().unwrap_err();
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert!(err
            .error
            .details
            .unwrap()
            .contains_key("merchant_reference"));
    }

    #[test]
    fn rejects_merchant_reference_too_long() {
        let mut req = valid_request();
        req.merchant_reference = "x".repeat(MERCHANT_REFERENCE_MAX_LEN + 1);

        let err = req.validate().unwrap_err();
        assert!(err
            .error
            .details
            .unwrap()
            .contains_key("merchant_reference"));
    }

    #[test]
    fn rejects_non_positive_amount() {
        let mut req = valid_request();
        req.amount = 0;

        let err = req.validate().unwrap_err();
        assert!(err.error.details.unwrap().contains_key("amount"));
    }

    #[test]
    fn rejects_malformed_currency() {
        let mut req = valid_request();
        req.currency = "idr".into(); // must be uppercase

        let err = req.validate().unwrap_err();
        assert!(err.error.details.unwrap().contains_key("currency"));
    }

    #[test]
    fn rejects_description_too_long() {
        let mut req = valid_request();
        req.description = Some("x".repeat(DESCRIPTION_MAX_LEN + 1));

        let err = req.validate().unwrap_err();
        assert!(err.error.details.unwrap().contains_key("description"));
    }

    #[test]
    fn rejects_implausible_customer_email() {
        let mut req = valid_request();
        req.customer = Some(CustomerInfo {
            name: None,
            email: Some("not-an-email".into()),
        });

        let err = req.validate().unwrap_err();
        assert!(err.error.details.unwrap().contains_key("customer.email"));
    }

    #[test]
    fn collects_multiple_field_errors_at_once() {
        let mut req = valid_request();
        req.merchant_reference = "".into();
        req.amount = -1;
        req.currency = "X".into();

        let err = req.validate().unwrap_err();
        let details = err.error.details.unwrap();
        assert!(details.contains_key("merchant_reference"));
        assert!(details.contains_key("amount"));
        assert!(details.contains_key("currency"));
    }

    #[test]
    fn maps_payment_domain_entity_to_response_dto() {
        let money = Money::new(250_000, "IDR").unwrap();
        let mut payment = Payment::new(
            Uuid::new_v4(),
            "checkout-order-10001".into(),
            "ORDER-10001".into(),
            money,
            Some("Test payment".into()),
        );
        payment.provider = Some("MIDTRANS".into());
        payment.payment_url = Some("https://pay.example/abc".into());

        let response: PaymentResponse = payment.clone().into();

        assert_eq!(response.data.payment_id, payment.id.to_string());
        assert_eq!(response.data.merchant_reference, "ORDER-10001");
        assert_eq!(response.data.status, "PENDING");
        assert_eq!(response.data.amount, 250_000);
        assert_eq!(response.data.currency, "IDR");
        assert_eq!(response.data.provider, "MIDTRANS");
        assert_eq!(
            response.data.payment_url.as_deref(),
            Some("https://pay.example/abc")
        );
        assert!(response.data.created_at.ends_with('Z'));
        assert!(response.data.completed_at.is_none());
    }

    #[test]
    fn falls_back_to_empty_provider_when_unset() {
        let money = Money::new(1000, "IDR").unwrap();
        let payment = Payment::new(Uuid::new_v4(), "key".into(), "REF".into(), money, None);

        let response: PaymentResponse = payment.into();

        assert_eq!(response.data.provider, "");
    }
}
