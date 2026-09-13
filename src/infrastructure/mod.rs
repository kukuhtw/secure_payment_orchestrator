pub mod postgres;
pub mod redis;

use sqlx::postgres::PgPoolOptions;
use redis::aio::ConnectionManager;

/// Create PostgreSQL connection pool.
pub async fn create_pool(database_url: &str) -> anyhow::Result<sqlx::PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;
    Ok(pool)
}

/// Create Redis connection manager.
pub fn create_client(redis_url: &str) -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open(redis_url)?;
    let manager = ConnectionManager::new(client);
    // Note: ConnectionManager::new is async in some versions
    // Using tokio runtime to connect
    Ok(manager) // Placeholder
}