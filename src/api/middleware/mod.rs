pub mod authentication;
pub mod idempotency;
pub mod request_id;

// `authentication::require_api_key` is wired directly onto the payment
// routes in `api::build_router` via `axum::middleware::from_fn_with_state`,
// since it must NOT apply to /health, /ready, /metrics, or webhook routes.

pub fn idempotency_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}

pub fn request_id_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}
