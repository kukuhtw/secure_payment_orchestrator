//! Secure Payment Orchestrator API
//!
//! Backend service untuk payment orchestration dengan multi-provider support,
//! idempotency, webhook verification, retry, circuit breaker, dan audit trail.

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load configuration
    let settings = spo_api::config::settings::Settings::load()?;

    // 2. Initialize observability (logging, metrics, tracing)
    spo_api::observability::logging::init(&settings)?;
    spo_api::observability::metrics::init()?;

    // 3. Initialize database pool
    let db_pool = spo_api::infrastructure::postgres::create_pool(&settings.database_url).await?;

    // 4. Initialize Redis connection
    let redis_client = spo_api::infrastructure::create_redis_client(&settings.redis_url).await?;

    // 5. Run database migrations
    sqlx::migrate!("./migrations").run(&db_pool).await?;

    // 6. Build provider adapters
    let providers = spo_api::providers::build_providers(&settings)?;

    // 7. Build application state
    let state = spo_api::AppState::new(db_pool, redis_client, providers, settings);

    // 8. Build router
    let app = spo_api::api::routes::build_router(Arc::new(state));

    // 9. Start server
    let addr = format!("0.0.0.0:{}", settings.server_port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
