//! Webhook routes.
//!
//! - `POST /webhooks/{provider}` — Receive webhook from provider

use crate::api::dto::error::ApiError;
use crate::SharedState;
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde_json::Value;

pub fn routes() -> Router<SharedState> {
    Router::new().route("/webhooks/{provider}", post(receive_webhook))
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
