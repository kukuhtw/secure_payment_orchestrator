//! Secure Payment Orchestrator library crate
//!
//! # Module Structure
//!
//! - `api` — Axum route handlers, middleware, DTOs
//! - `application` — Service orchestration layer
//! - `domain` — Pure business logic, entities, state machine
//! - `infrastructure` — Database, cache, external dependencies
//! - `providers` — Payment provider adapters
//! - `security` — Authentication, HMAC, hashing
//! - `observability` — Logging, metrics, health
//! - `config` — Application configuration

pub mod api;
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod providers;
pub mod security;
pub mod observability;
pub mod config;

use std::sync::Arc;
use sqlx::PgPool;
use redis::aio::ConnectionManager;
use tokio::sync::RwLock;

/// Shared application state injected into all route handlers.
pub struct AppState {
    pub db_pool: PgPool,
    pub redis: ConnectionManager,
    pub providers: Arc<RwLock<Vec<Box<dyn providers::adapter::PaymentProvider>>>>,
    pub settings: config::settings::Settings,
}

impl AppState {
    pub fn new(
        db_pool: PgPool,
        redis: ConnectionManager,
        providers: Vec<Box<dyn providers::adapter::PaymentProvider>>,
        settings: config::settings::Settings,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            db_pool,
            redis,
            providers: Arc::new(RwLock::new(providers)),
            settings,
        })
    }
}

/// Type alias for shared state in Axum handlers.
pub type SharedState = Arc<AppState>;