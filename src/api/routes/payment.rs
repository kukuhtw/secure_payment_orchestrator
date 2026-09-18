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
use crate::api::middleware::idempotency::IdempotencyKey;
use crate::application::ApplicationError;
use crate::domain::error::DomainError;
use crate::security::api_key::MerchantContext;
use crate::SharedState;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;
use uuid::Uuid;

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
    Extension(merchant): Extension<MerchantContext>,
    Extension(IdempotencyKey(idempotency_key)): Extension<IdempotencyKey>,
    Json(req): Json<CreatePaymentRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiError> {
    tracing::info!("Create payment: {:?}", req.merchant_reference);

    req.validate()?;

    let input = crate::application::payment::CreatePaymentInput {
        idempotency_key,
        merchant_reference: req.merchant_reference,
        amount: req.amount,
        currency: req.currency,
        description: req.description,
    };

    let payment = state
        .payment_service
        .create_payment(merchant.merchant_id, merchant.api_key_id, input)
        .await
        .map_err(map_application_error)?;

    Ok((StatusCode::CREATED, Json(payment.into())))
}

async fn get_payment(
    State(state): State<SharedState>,
    Extension(merchant): Extension<MerchantContext>,
    Path(payment_id): Path<String>,
) -> Result<Json<PaymentResponse>, ApiError> {
    tracing::info!("Get payment: {}", payment_id);

    let payment_id = Uuid::parse_str(&payment_id)
        .map_err(|_| ApiError::bad_request("Invalid payment_id format"))?;

    let payment = state
        .payment_service
        .get_payment(merchant.merchant_id, payment_id)
        .await
        .map_err(|err| map_payment_lookup_error(err, payment_id))?;

    Ok(Json(payment.into()))
}

/// Maps orchestration-layer errors to the API's standard error envelope.
fn map_application_error(err: ApplicationError) -> ApiError {
    match err {
        ApplicationError::Domain(DomainError::Validation(msg)) => ApiError::bad_request(msg),
        ApplicationError::Domain(DomainError::Conflict(msg)) => ApiError::conflict("CONFLICT", msg),
        ApplicationError::Domain(DomainError::AlreadyFinal(status)) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "PAYMENT_ALREADY_FINAL",
            format!("Payment is already in a final state: {status}"),
        ),
        ApplicationError::Domain(DomainError::InvalidTransition { from, to }) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_STATUS_TRANSITION",
            format!("Cannot transition payment from {from} to {to}"),
        ),
        ApplicationError::Domain(other) => {
            tracing::error!("unexpected domain error in payment handler: {other}");
            ApiError::internal()
        }
        ApplicationError::Provider(provider_err) => {
            tracing::error!("provider error handling payment: {provider_err}");
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "PROVIDER_ERROR",
                "Payment provider did not respond successfully",
            )
        }
        ApplicationError::NoProviderAvailable => ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "PROVIDER_UNAVAILABLE",
            "No payment provider is currently available",
        ),
    }
}

/// Like [`map_application_error`], but reshapes `NotFound` into the
/// `PAYMENT_NOT_FOUND` code/details documented for `GET /payments/{id}` and
/// `POST /payments/{id}/cancel`.
fn map_payment_lookup_error(err: ApplicationError, payment_id: Uuid) -> ApiError {
    match err {
        ApplicationError::Domain(DomainError::NotFound(_)) => {
            let mut details = HashMap::new();
            details.insert(
                "payment_id".to_string(),
                serde_json::Value::String(payment_id.to_string()),
            );
            ApiError::with_details(
                StatusCode::NOT_FOUND,
                "PAYMENT_NOT_FOUND",
                "Payment not found with the given ID",
                details,
            )
        }
        other => map_application_error(other),
    }
}

async fn search_payments(
    State(state): State<SharedState>,
    Extension(merchant): Extension<MerchantContext>,
    Query(params): Query<SearchPaymentParams>,
) -> Result<Json<SearchPaymentsResponse>, ApiError> {
    tracing::info!("Search payments: {:?}", params);

    let filter = params.into_filter()?;

    let result = state
        .payment_service
        .search_payments(merchant.merchant_id, filter)
        .await
        .map_err(map_application_error)?;

    Ok(Json(result.into()))
}

async fn cancel_payment(
    State(state): State<SharedState>,
    Extension(merchant): Extension<MerchantContext>,
    Path(payment_id): Path<String>,
    Json(req): Json<CancelPaymentRequest>,
) -> Result<Json<CancelPaymentResponse>, ApiError> {
    tracing::info!("Cancel payment: {}", payment_id);

    req.validate()?;

    let payment_id = Uuid::parse_str(&payment_id)
        .map_err(|_| ApiError::bad_request("Invalid payment_id format"))?;

    let payment = state
        .payment_service
        .cancel_payment(
            merchant.merchant_id,
            merchant.api_key_id,
            payment_id,
            req.reason,
        )
        .await
        .map_err(|err| map_payment_lookup_error(err, payment_id))?;

    Ok(Json(payment.into()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::adapter::ProviderError;

    #[test]
    fn maps_validation_error_to_400() {
        let err = ApplicationError::Domain(DomainError::Validation("bad amount".into()));
        let api_err = map_application_error(err);

        assert_eq!(api_err.status_code, StatusCode::BAD_REQUEST);
        assert_eq!(api_err.error.code, "VALIDATION_ERROR");
    }

    #[test]
    fn maps_provider_error_to_502() {
        let err = ApplicationError::Provider(ProviderError::Unavailable);
        let api_err = map_application_error(err);

        assert_eq!(api_err.status_code, StatusCode::BAD_GATEWAY);
        assert_eq!(api_err.error.code, "PROVIDER_ERROR");
    }

    #[test]
    fn maps_no_provider_available_to_503() {
        let api_err = map_application_error(ApplicationError::NoProviderAvailable);

        assert_eq!(api_err.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(api_err.error.code, "PROVIDER_UNAVAILABLE");
    }

    #[test]
    fn maps_not_found_to_payment_not_found_with_details() {
        let payment_id = Uuid::new_v4();
        let err = ApplicationError::Domain(DomainError::NotFound("Payment not found".into()));
        let api_err = map_payment_lookup_error(err, payment_id);

        assert_eq!(api_err.status_code, StatusCode::NOT_FOUND);
        assert_eq!(api_err.error.code, "PAYMENT_NOT_FOUND");
        assert_eq!(
            api_err.error.details.unwrap().get("payment_id"),
            Some(&serde_json::Value::String(payment_id.to_string()))
        );
    }

    #[test]
    fn payment_lookup_error_falls_through_to_generic_mapping() {
        let err = ApplicationError::NoProviderAvailable;
        let api_err = map_payment_lookup_error(err, Uuid::new_v4());

        assert_eq!(api_err.status_code, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(api_err.error.code, "PROVIDER_UNAVAILABLE");
    }

    #[test]
    fn maps_already_final_to_422() {
        let err = ApplicationError::Domain(DomainError::AlreadyFinal(
            crate::domain::status::PaymentStatus::Success,
        ));
        let api_err = map_application_error(err);

        assert_eq!(api_err.status_code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(api_err.error.code, "PAYMENT_ALREADY_FINAL");
    }

    #[test]
    fn maps_invalid_transition_to_422() {
        let err = ApplicationError::Domain(DomainError::InvalidTransition {
            from: crate::domain::status::PaymentStatus::Pending,
            to: crate::domain::status::PaymentStatus::Success,
        });
        let api_err = map_application_error(err);

        assert_eq!(api_err.status_code, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(api_err.error.code, "INVALID_STATUS_TRANSITION");
    }
}
