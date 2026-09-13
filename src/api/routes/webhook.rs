//! Webhook routes.
//!
//! - `POST /webhooks/{provider}` — Receive webhook from provider

use axum::{Router, routing::post, extract::{Path, State}, Json};
use crate::SharedState;
use crate::api::dto::error::ApiError;
use serde_json::Value;

pub fn routes() -> Router<SharedState> {
    Router::new()
        .route("/webhooks/{provider}", post(receive_webhook))
}

async fn receive_webhook(
    State(state): State<SharedState>,
    Path(provider): Path<String>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    // TODO: Implement webhook verification and processing
    tracing::info!("Webhook from provider: {}", provider);
    Err(ApiError::not_implemented("receive_webhook"))
}