//! Payment-related DTOs (request & response).

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