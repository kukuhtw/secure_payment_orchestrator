pub mod dto;
pub mod middleware;
pub mod routes;

use crate::SharedState;
use axum::Router;

/// Build the main application router with all routes and middleware.
pub fn build_router(state: SharedState) -> Router {
    // Authentication only guards merchant-facing payment routes — not
    // /health, /ready, /metrics (public), or webhook routes (authenticated
    // separately via HMAC signature verification, not merchant API keys).
    let payment_routes = routes::payment::routes().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        middleware::authentication::require_api_key,
    ));

    Router::new()
        .nest("/api/v1", payment_routes)
        .nest("/api/v1", routes::webhook::routes())
        .route("/health", axum::routing::get(routes::health::health_check))
        .route(
            "/ready",
            axum::routing::get(routes::health::readiness_check),
        )
        .route("/metrics", axum::routing::get(routes::metrics::get_metrics))
        .layer(middleware::idempotency_layer())
        .layer(middleware::request_id_layer())
        .with_state(state)
}
