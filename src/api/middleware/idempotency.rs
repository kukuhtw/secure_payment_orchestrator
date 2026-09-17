//! Idempotency middleware.
//!
//! Memeriksa `Idempotency-Key` header untuk mencegah duplikasi request.
//! Menggunakan Redis lock + database UNIQUE constraint.
//!
//! Middleware ini menjaga bagian *baca*: mendeteksi request duplikat yang
//! sudah tersimpan (return cached response), menolak payload berbeda dengan
//! key yang sama (409 `IDEMPOTENCY_MISMATCH`), dan mencegah dua request
//! konkuren dengan key yang sama diproses bersamaan (Redis lock). Bagian
//! *tulis* — menyimpan `IdempotencyRow` (butuh `payment_id`, harus atomic
//! dengan insert payment) — sengaja belum dilakukan di sini; itu tanggung
//! jawab payment application service saat item P0 "Atomic transaction"
//! dikerjakan, supaya payment + idempotency record + audit log tersimpan
//! dalam satu transaksi database yang sama.

use crate::api::dto::error::ApiError;
use crate::domain::repositories::IdempotencyRow;
use crate::infrastructure::redis::lock::{acquire_lock, generate_lock_value, release_lock};
use crate::security::api_key::MerchantContext;
use crate::SharedState;
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
/// Request bodies larger than this are rejected before hashing/buffering.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Axum middleware: enforces `Idempotency-Key` on POST payment routes.
///
/// Must run *after* [`crate::api::middleware::authentication::require_api_key`]
/// (relies on [`MerchantContext`] already being attached to request extensions),
/// and is a no-op for non-POST methods since only mutating endpoints require it.
pub async fn require_idempotency_key(
    State(state): State<SharedState>,
    req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if req.method() != Method::POST {
        return Ok(next.run(req).await);
    }

    let merchant = req
        .extensions()
        .get::<MerchantContext>()
        .cloned()
        .ok_or_else(ApiError::internal)?;

    let idempotency_key = req
        .headers()
        .get(IDEMPOTENCY_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| ApiError::bad_request("Idempotency-Key header is required"))?;

    let (parts, body) = req.into_parts();
    let body_bytes = to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| ApiError::bad_request("Request body too large or unreadable"))?;
    let request_hash = compute_request_hash(&body_bytes);

    if let Some(existing) = state
        .repos
        .idempotency
        .find_by_key(&idempotency_key, merchant.merchant_id)
        .await
        .map_err(|_| ApiError::internal())?
    {
        if existing.request_hash == request_hash {
            return Ok(build_cached_response(&existing));
        }

        let mut details = HashMap::new();
        details.insert(
            "existing_payment_id".to_string(),
            Value::String(existing.payment_id.to_string()),
        );
        return Err(ApiError::with_details(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_MISMATCH",
            "Idempotency key already used with different request body",
            details,
        ));
    }

    let lock_key = format!(
        "idempotency-lock:{}:{}",
        merchant.merchant_id, idempotency_key
    );
    let lock_value = generate_lock_value();
    let mut redis_conn = state.redis.clone();

    let acquired = acquire_lock(&mut redis_conn, &lock_key, &lock_value)
        .await
        .map_err(|_| ApiError::internal())?;

    if !acquired {
        return Err(ApiError::conflict(
            "IDEMPOTENCY_IN_PROGRESS",
            "A request with this Idempotency-Key is already being processed",
        ));
    }

    let req = Request::from_parts(parts, Body::from(body_bytes));
    let response = next.run(req).await;

    // Best-effort release — the lock also expires via TTL, so a failed
    // release here does not leave the key permanently stuck.
    let _ = release_lock(&mut redis_conn, &lock_key, &lock_value).await;

    Ok(response)
}

/// SHA-256 hex digest of the raw request body, matching `idempotency_keys.request_hash VARCHAR(64)`.
fn compute_request_hash(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    hex::encode(digest)
}

/// Reconstruct the HTTP response that was returned the first time this
/// idempotency key was used, from the persisted [`IdempotencyRow`].
fn build_cached_response(row: &IdempotencyRow) -> Response {
    let status = row
        .response_status_code
        .as_deref()
        .and_then(|code| code.parse::<u16>().ok())
        .and_then(|code| StatusCode::from_u16(code).ok())
        .unwrap_or(StatusCode::OK);

    let body = row.response_body.clone().unwrap_or(Value::Null);

    (status, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn sample_row(status_code: Option<&str>, body: Option<Value>) -> IdempotencyRow {
        IdempotencyRow {
            idempotency_key: "checkout-order-10001".into(),
            merchant_id: Uuid::nil(),
            request_hash: compute_request_hash(b"{}"),
            payment_id: Uuid::nil(),
            response_status_code: status_code.map(str::to_string),
            response_body: body,
            created_at: Utc::now(),
            expires_at: Utc::now(),
        }
    }

    #[test]
    fn hashes_request_body_deterministically() {
        let a = compute_request_hash(b"{\"amount\":1000}");
        let b = compute_request_hash(b"{\"amount\":1000}");
        let c = compute_request_hash(b"{\"amount\":2000}");

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64); // fits idempotency_keys.request_hash VARCHAR(64)
    }

    #[tokio::test]
    async fn builds_cached_response_from_row() {
        let row = sample_row(
            Some("201"),
            Some(serde_json::json!({"data": {"payment_id": "pay_1"}})),
        );

        let response = build_cached_response(&row);
        assert_eq!(response.status(), StatusCode::CREATED);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["payment_id"], "pay_1");
    }

    #[test]
    fn falls_back_to_200_for_missing_status_code() {
        let row = sample_row(None, None);
        let response = build_cached_response(&row);
        assert_eq!(response.status(), StatusCode::OK);
    }
}
