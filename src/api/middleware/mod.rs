pub mod authentication;
pub mod idempotency;
pub mod request_id;

use axum::middleware::from_fn_with_state;

pub fn auth_layer() -> tower::layer::util::Identity {
    // Placeholder — implemented in authentication.rs
    tower::layer::util::Identity::new()
}

pub fn idempotency_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}

pub fn request_id_layer() -> tower::layer::util::Identity {
    tower::layer::util::Identity::new()
}