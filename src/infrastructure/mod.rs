pub mod postgres;
pub mod redis;

use crate::domain::repositories::*;
use redis::aio::ConnectionManager;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

/// All repository implementations bundled together for easy access.
pub struct Repositories {
    pub api_key: Arc<dyn ApiKeyRepository>,
    pub payment: Arc<dyn PaymentRepository>,
    pub attempt: Arc<dyn AttemptRepository>,
    pub idempotency: Arc<dyn IdempotencyRepository>,
    pub audit_log: Arc<dyn AuditLogRepository>,
    pub webhook_event: Arc<dyn WebhookEventRepository>,
}

impl Repositories {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            api_key: Arc::new(postgres::repositories::PgApiKeyRepository::new(
                pool.clone(),
            )),
            payment: Arc::new(postgres::repositories::PgPaymentRepository::new(
                pool.clone(),
            )),
            attempt: Arc::new(postgres::repositories::PgAttemptRepository::new(
                pool.clone(),
            )),
            idempotency: Arc::new(postgres::repositories::PgIdempotencyRepository::new(
                pool.clone(),
            )),
            audit_log: Arc::new(postgres::repositories::PgAuditLogRepository::new(
                pool.clone(),
            )),
            webhook_event: Arc::new(postgres::repositories::PgWebhookEventRepository::new(pool)),
        }
    }
}

/// Create PostgreSQL connection pool.
pub async fn create_pool(database_url: &str) -> anyhow::Result<sqlx::PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Create Redis connection manager.
pub async fn create_redis_client(redis_url: &str) -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open(redis_url)?;
    let manager = ConnectionManager::new(client).await?;
    Ok(manager)
}
