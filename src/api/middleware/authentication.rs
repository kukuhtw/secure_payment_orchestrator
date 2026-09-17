//! Authentication middleware.
//!
//! Validates `Authorization: Bearer <api_key>` header.
//! API key dibandingkan menggunakan constant-time comparison terhadap hash di database.

use crate::api::dto::error::ApiError;
use crate::security::api_key::{key_prefix, parse_bearer_token, MerchantContext};
use crate::security::hash::verify_secret;
use crate::SharedState;
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};

/// Axum middleware: validates the `Authorization` Bearer token against the
/// `api_keys` table and attaches a [`MerchantContext`] to request extensions
/// for downstream handlers/services to consume.
pub async fn require_api_key(
    State(state): State<SharedState>,
    mut req: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("Missing Authorization header"))?;

    let token = parse_bearer_token(header)
        .ok_or_else(|| ApiError::unauthorized("Authorization header must be a Bearer token"))?;

    let prefix = key_prefix(token);

    // `find_by_key_prefix` sudah memfilter `revoked_at IS NULL` dan
    // `expires_at > NOW()` di level SQL, jadi NotFound di sini juga mencakup
    // key yang revoked/expired — tidak perlu pengecekan terpisah.
    let api_key_row = state
        .repos
        .api_key
        .find_by_key_prefix(prefix)
        .await
        .map_err(|_| ApiError::unauthorized("Invalid or expired API key"))?;

    if !verify_secret(token, &api_key_row.key_hash) {
        return Err(ApiError::unauthorized("Invalid or expired API key"));
    }

    // `get_merchant` sudah memfilter `status = 'ACTIVE'`.
    state
        .repos
        .api_key
        .get_merchant(api_key_row.merchant_id)
        .await
        .map_err(|_| ApiError::unauthorized("Merchant not found or inactive"))?;

    req.extensions_mut().insert(MerchantContext {
        merchant_id: api_key_row.merchant_id,
        api_key_id: api_key_row.id,
        permissions: api_key_row.permissions,
    });

    Ok(next.run(req).await)
}
