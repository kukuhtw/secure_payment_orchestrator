//! Payment-related DTOs (request & response).

use crate::domain::payment::Payment;
use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
