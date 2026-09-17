pub mod authentication;
pub mod idempotency;
pub mod request_id;

// `authentication::require_api_key` and `idempotency::require_idempotency_key`
// are wired directly onto the payment routes in `api::build_router` via
// `axum::middleware::from_fn_with_state`, since neither must apply to
// /health, /ready, /metrics, or webhook routes — and idempotency must run
// strictly after authentication (it reads the MerchantContext auth attaches).

pub fn request_id_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}
