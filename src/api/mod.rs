pub mod dto;
pub mod middleware;
pub mod routes;

use crate::SharedState;
use axum::Router;

/// Build the main application router with all routes and middleware.
pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .nest("/api/v1", routes::payment::routes())
        .nest("/api/v1", routes::webhook::routes())
        .route("/health", axum::routing::get(routes::health::health_check))
        .route(
            "/ready",
            axum::routing::get(routes::health::readiness_check),
        )
        .route("/metrics", axum::routing::get(routes::metrics::get_metrics))
        .layer(middleware::authentication::auth_layer())
        .layer(middleware::idempotency::idempotency_layer())
        .layer(middleware::request_id::request_id_layer())
        .with_state(state)
}
