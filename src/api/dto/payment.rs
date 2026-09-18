//! Payment-related DTOs (request & response).

use crate::api::dto::error::ApiError;
use crate::application::payment::SearchPaymentsFilter;
use crate::domain::payment::Payment;
use crate::domain::repositories::{PaginatedResult, PaymentSummaryRow};
use axum::http::StatusCode;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// `merchant_reference` column is `VARCHAR(255)`.
const MERCHANT_REFERENCE_MAX_LEN: usize = 255;
/// No DB limit on `description` (`TEXT`) — this is an operational sanity cap.
const DESCRIPTION_MAX_LEN: usize = 1000;
/// No DB limit on `failure_reason` (`TEXT`, reused for cancel reason) — an
/// operational sanity cap, matching the API contract's "reason melebihi
/// batas karakter" 400 error condition.
const REASON_MAX_LEN: usize = 1000;
const DEFAULT_PAGE: i32 = 1;
const DEFAULT_LIMIT: i32 = 20;
const MAX_LIMIT: i32 = 100;

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

impl CancelPaymentRequest {
    /// Matches the API contract's "400 Bad Request jika reason melebihi
    /// batas karakter" error condition for `POST /payments/{id}/cancel`.
    pub fn validate(&self) -> Result<(), ApiError> {
        let Some(reason) = &self.reason else {
            return Ok(());
        };

        if reason.len() > REASON_MAX_LEN {
            let mut details = HashMap::new();
            details.insert(
                "reason".into(),
                Value::String(format!("must be at most {REASON_MAX_LEN} characters")),
            );
            return Err(ApiError::with_details(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Request validation failed",
                details,
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct CancelPaymentResponse {
    pub data: CancelPaymentData,
}

#[derive(Debug, Serialize)]
pub struct CancelPaymentData {
    pub payment_id: String,
    pub status: String,
    pub cancelled_at: String,
}

impl From<Payment> for CancelPaymentResponse {
    fn from(payment: Payment) -> Self {
        Self {
            data: CancelPaymentData {
                payment_id: payment.id.to_string(),
                status: payment.status.to_string(),
                cancelled_at: payment
                    .completed_at
                    .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
                    .unwrap_or_default(),
            },
        }
    }
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

impl SearchPaymentParams {
    /// Applies defaults (`page=1`, `limit=20`), bounds (`1 <= limit <= 100`),
    /// and parses `from_date`/`to_date` as RFC 3339. Collects all violations
    /// into one `400 VALIDATION_ERROR`, same pattern as
    /// `CreatePaymentRequest::validate`.
    pub fn into_filter(self) -> Result<SearchPaymentsFilter, ApiError> {
        let mut details: HashMap<String, Value> = HashMap::new();

        let page = match self.page {
            None => DEFAULT_PAGE,
            Some(p) if p >= 1 => p,
            Some(_) => {
                details.insert("page".into(), Value::String("must be >= 1".into()));
                DEFAULT_PAGE
            }
        };

        let limit = match self.limit {
            None => DEFAULT_LIMIT,
            Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
            Some(_) => {
                details.insert(
                    "limit".into(),
                    Value::String(format!("must be between 1 and {MAX_LIMIT}")),
                );
                DEFAULT_LIMIT
            }
        };

        let from_date = parse_optional_date(self.from_date.as_deref(), "from_date", &mut details);
        let to_date = parse_optional_date(self.to_date.as_deref(), "to_date", &mut details);

        if !details.is_empty() {
            return Err(ApiError::with_details(
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Request validation failed",
                details,
            ));
        }

        Ok(SearchPaymentsFilter {
            merchant_reference: self.merchant_reference,
            status: self.status,
            provider: self.provider,
            from_date,
            to_date,
            page: page as i64,
            limit: limit as i64,
        })
    }
}

fn parse_optional_date(
    value: Option<&str>,
    field: &str,
    details: &mut HashMap<String, Value>,
) -> Option<DateTime<Utc>> {
    let value = value?;
    match DateTime::parse_from_rfc3339(value) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(_) => {
            details.insert(
                field.to_string(),
                Value::String("must be a valid RFC 3339 date-time".into()),
            );
            None
        }
    }
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

impl From<PaymentSummaryRow> for PaymentSummary {
    fn from(row: PaymentSummaryRow) -> Self {
        Self {
            payment_id: row.id.to_string(),
            merchant_reference: row.merchant_reference,
            status: row.status,
            amount: row.amount,
            currency: row.currency,
            provider: row.provider.unwrap_or_default(),
            created_at: row.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        }
    }
}

impl From<PaginatedResult<PaymentSummaryRow>> for SearchPaymentsResponse {
    fn from(result: PaginatedResult<PaymentSummaryRow>) -> Self {
        let total_pages = if result.limit > 0 {
            ((result.total + result.limit - 1) / result.limit) as i32
        } else {
            0
        };

        Self {
            data: SearchPaymentsData {
                payments: result.items.into_iter().map(PaymentSummary::from).collect(),
                pagination: Pagination {
                    page: result.page as i32,
                    limit: result.limit as i32,
                    total_items: result.total,
                    total_pages,
                },
            },
        }
    }
}

// ─── Retry Payment ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RetryPaymentResponse {
    pub data: RetryPaymentData,
}

#[derive(Debug, Serialize)]
pub struct RetryPaymentData {
    pub payment_id: String,
    pub status: String,
    pub attempt_number: i32,
    pub provider: String,
    pub message: String,
}

impl From<(Payment, i32)> for RetryPaymentResponse {
    fn from((payment, attempt_number): (Payment, i32)) -> Self {
        Self {
            data: RetryPaymentData {
                payment_id: payment.id.to_string(),
                status: payment.status.to_string(),
                attempt_number,
                provider: payment.provider.unwrap_or_default(),
                message: "Retry initiated".into(),
            },
        }
    }
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

impl From<crate::application::payment::ReconciliationOutcome> for ReconcileResponse {
    fn from(outcome: crate::application::payment::ReconciliationOutcome) -> Self {
        Self {
            data: ReconcileData {
                payment_id: outcome.payment_id.to_string(),
                previous_status: outcome.previous_status.to_string(),
                current_status: outcome.current_status.to_string(),
                provider_status: outcome.provider_status,
                resolution: outcome.resolution,
                reconciled_at: outcome
                    .reconciled_at
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
            },
        }
    }
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

    #[test]
    fn cancel_response_maps_status_and_cancelled_at() {
        let money = Money::new(1000, "IDR").unwrap();
        let mut payment = Payment::new(Uuid::new_v4(), "key".into(), "REF".into(), money, None);
        payment.status = crate::domain::status::PaymentStatus::Cancelled;
        payment.completed_at = Some(payment.updated_at);

        let response: CancelPaymentResponse = payment.clone().into();

        assert_eq!(response.data.payment_id, payment.id.to_string());
        assert_eq!(response.data.status, "CANCELLED");
        assert!(response.data.cancelled_at.ends_with('Z'));
    }

    #[test]
    fn cancel_request_accepts_missing_reason() {
        let req = CancelPaymentRequest { reason: None };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn cancel_request_rejects_reason_too_long() {
        let req = CancelPaymentRequest {
            reason: Some("x".repeat(REASON_MAX_LEN + 1)),
        };
        let err = req.validate().unwrap_err();
        assert_eq!(err.status_code, StatusCode::BAD_REQUEST);
        assert!(err.error.details.unwrap().contains_key("reason"));
    }

    #[test]
    fn search_params_applies_defaults() {
        let params = SearchPaymentParams {
            merchant_reference: None,
            status: None,
            provider: None,
            from_date: None,
            to_date: None,
            page: None,
            limit: None,
        };
        let filter = params.into_filter().expect("defaults should be valid");
        assert_eq!(filter.page, 1);
        assert_eq!(filter.limit, 20);
    }

    #[test]
    fn search_params_rejects_invalid_page_and_limit() {
        let params = SearchPaymentParams {
            merchant_reference: None,
            status: None,
            provider: None,
            from_date: None,
            to_date: None,
            page: Some(0),
            limit: Some(500),
        };
        let err = params.into_filter().unwrap_err();
        let details = err.error.details.unwrap();
        assert!(details.contains_key("page"));
        assert!(details.contains_key("limit"));
    }

    #[test]
    fn search_params_parses_valid_dates() {
        let params = SearchPaymentParams {
            merchant_reference: None,
            status: None,
            provider: None,
            from_date: Some("2026-09-01T00:00:00Z".into()),
            to_date: Some("2026-09-17T23:59:59Z".into()),
            page: None,
            limit: None,
        };
        let filter = params.into_filter().expect("valid dates should parse");
        assert!(filter.from_date.is_some());
        assert!(filter.to_date.is_some());
    }

    #[test]
    fn search_params_rejects_malformed_date() {
        let params = SearchPaymentParams {
            merchant_reference: None,
            status: None,
            provider: None,
            from_date: Some("not-a-date".into()),
            to_date: None,
            page: None,
            limit: None,
        };
        let err = params.into_filter().unwrap_err();
        assert!(err.error.details.unwrap().contains_key("from_date"));
    }

    #[test]
    fn maps_paginated_result_to_search_response() {
        let now = chrono::Utc::now();
        let result = PaginatedResult {
            items: vec![PaymentSummaryRow {
                id: Uuid::new_v4(),
                merchant_reference: "ORDER-10001".into(),
                status: "PENDING".into(),
                amount: 250_000,
                currency: "IDR".into(),
                provider: Some("MIDTRANS".into()),
                created_at: now,
            }],
            total: 21,
            page: 1,
            limit: 20,
        };

        let response: SearchPaymentsResponse = result.into();

        assert_eq!(response.data.payments.len(), 1);
        assert_eq!(response.data.payments[0].merchant_reference, "ORDER-10001");
        assert_eq!(response.data.pagination.total_items, 21);
        assert_eq!(response.data.pagination.total_pages, 2);
    }

    #[test]
    fn maps_retry_result_to_response() {
        let money = Money::new(250_000, "IDR").unwrap();
        let mut payment = Payment::new(Uuid::new_v4(), "key".into(), "REF".into(), money, None);
        payment.status = crate::domain::status::PaymentStatus::Processing;
        payment.provider = Some("XENDIT".into());

        let response: RetryPaymentResponse = (payment.clone(), 2).into();

        assert_eq!(response.data.payment_id, payment.id.to_string());
        assert_eq!(response.data.status, "PROCESSING");
        assert_eq!(response.data.attempt_number, 2);
        assert_eq!(response.data.provider, "XENDIT");
        assert_eq!(response.data.message, "Retry initiated");
    }
}
