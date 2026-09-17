//! Distributed lock implementation using Redis.
//!
//! Mencegah race condition pada operasi concurrent:
//! - Create payment (idempotency key lock)
//! - Process webhook (event ID lock)
//! - Reconcile payment (payment ID lock)

use redis::aio::ConnectionManager;
use uuid::Uuid;

const LOCK_TTL_SECONDS: usize = 30;

/// Acquire a distributed lock with Redis.
/// Returns `true` if lock was acquired, `false` if already locked.
pub async fn acquire_lock(
    redis: &mut ConnectionManager,
    lock_key: &str,
    lock_value: &str,
) -> Result<bool, redis::RedisError> {
    // SET lock_key lock_value NX EX 30
    let result: Option<String> = redis::Cmd::new()
        .arg("SET")
        .arg(lock_key)
        .arg(lock_value)
        .arg("NX")
        .arg("EX")
        .arg(LOCK_TTL_SECONDS)
        .query_async(redis)
        .await?;

    Ok(result.is_some())
}

/// Release lock only if we own it (compare value before delete).
pub async fn release_lock(
    redis: &mut ConnectionManager,
    lock_key: &str,
    lock_value: &str,
) -> Result<(), redis::RedisError> {
    // Lua script: only delete if value matches
    let script = redis::Script::new(
        r#"
        if redis.call("GET", KEYS[1]) == ARGV[1] then
            return redis.call("DEL", KEYS[1])
        else
            return 0
        end
        "#,
    );

    let _: () = script
        .key(lock_key)
        .arg(lock_value)
        .invoke_async(redis)
        .await?;

    Ok(())
}

/// Generate unique lock value (instance ID + UUID).
pub fn generate_lock_value() -> String {
    format!("{}:{}", std::process::id(), Uuid::new_v4())
}
