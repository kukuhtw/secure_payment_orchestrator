//! Payment routes.
//!
//! - `POST /payments` — Create payment
//! - `GET /payments/{id}` — Get payment detail
//! - `GET /payments` — Search payments
//! - `POST /payments/{id}/cancel` — Cancel payment
//! - `POST /payments/{id}/retry` — Manual retry (operations)
//! - `POST /payments/{id}/reconcile` — Reconciliation (operations)

use crate::api::dto::error::*;
use crate::api::dto::payment::*;
use crate::SharedState;
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/payments", post(create_payment).get(search_payments))
        .route("/payments/{id}", get(get_payment))
        .route("/payments/{id}/cancel", post(cancel_payment))
        .route("/payments/{id}/retry", post(retry_payment))
        .route("/payments/{id}/reconcile", post(reconcile_payment))
}

async fn create_payment(
    State(state): State<SharedState>,
    Json(req): Json<CreatePaymentRequest>,
) -> Result<Json<PaymentResponse>, ApiError> {
    // TODO: Implement payment creation
    tracing::info!("Create payment: {:?}", req.merchant_reference);
    Err(ApiError::not_implemented("create_payment"))
}

async fn get_payment(
    State(state): State<SharedState>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentResponse>, ApiError> {
    // TODO: Implement get payment
    tracing::info!("Get payment: {}", payment_id);
    Err(ApiError::not_implemented("get_payment"))
}

async fn search_payments(
    State(state): State<SharedState>,
    Query(params): Query<SearchPaymentParams>,
) -> Result<Json<SearchPaymentsResponse>, ApiError> {
    // TODO: Implement search payments
    tracing::info!("Search payments: {:?}", params);
    Err(ApiError::not_implemented("search_payments"))
}

async fn cancel_payment(
    State(state): State<SharedState>,
    Path(payment_id): Path<String>,
    Json(req): Json<CancelPaymentRequest>,
) -> Result<Json<PaymentResponse>, ApiError> {
    // TODO: Implement cancel payment
    tracing::info!("Cancel payment: {}", payment_id);
    Err(ApiError::not_implemented("cancel_payment"))
}

async fn retry_payment(
    State(state): State<SharedState>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentResponse>, ApiError> {
    // TODO: Implement manual retry
    tracing::info!("Retry payment: {}", payment_id);
    Err(ApiError::not_implemented("retry_payment"))
}

async fn reconcile_payment(
    State(state): State<SharedState>,
    Path(payment_id): Path<String>,
) -> Result<Json<ReconcileResponse>, ApiError> {
    // TODO: Implement reconciliation
    tracing::info!("Reconcile payment: {}", payment_id);
    Err(ApiError::not_implemented("reconcile_payment"))
}
