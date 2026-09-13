//! PostgreSQL repository implementations.
//!
//! Semua query menggunakan parameterized query (SQLx) untuk mencegah SQL injection.

use sqlx::PgPool;

/// Check database connectivity.
pub async fn ping(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}