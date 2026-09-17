//! Health & readiness endpoints.
//!
//! - `GET /health` — Liveness probe
//! - `GET /ready` — Readiness probe (checks DB + Redis)

use crate::SharedState;
use axum::{extract::State, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
}

#[derive(Serialize)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: String,
    pub redis: String,
}

pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime_seconds: 0, // TODO: track startup time
    })
}

pub async fn readiness_check(
    State(state): State<SharedState>,
) -> Result<Json<ReadinessResponse>, (axum::http::StatusCode, Json<ReadinessResponse>)> {
    let db_status = match sqlx::query("SELECT 1").execute(&state.db_pool).await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    let redis_status = match state.redis.ping().await {
        Ok(_) => "connected",
        Err(_) => "disconnected",
    };

    let ready = db_status == "connected" && redis_status == "connected";

    let response = ReadinessResponse {
        status: if ready {
            "ready".into()
        } else {
            "not_ready".into()
        },
        database: db_status.into(),
        redis: redis_status.into(),
    };

    if ready {
        Ok(Json(response))
    } else {
        Err((axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}
