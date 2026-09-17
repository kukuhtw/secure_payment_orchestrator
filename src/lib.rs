//! Secure Payment Orchestrator library crate
//!
//! # Module Structure
//!
//! - `api` — Axum route handlers, middleware, DTOs
//! - `application` — Service orchestration layer
//! - `domain` — Pure business logic, entities, state machine, repository traits
//! - `infrastructure` — Database, cache, repository implementations
//! - `providers` — Payment provider adapters
//! - `security` — Authentication, HMAC, hashing
//! - `observability` — Logging, metrics, health
//! - `config` — Application configuration

pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod observability;
pub mod providers;
pub mod security;

use crate::infrastructure::Repositories;
use crate::providers::adapter::PaymentProvider;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared application state injected into all route handlers.
pub struct AppState {
    pub repos: Repositories,
    pub redis: redis::aio::ConnectionManager,
    pub providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
    pub settings: config::settings::Settings,
}

impl AppState {
    pub fn new(
        db_pool: sqlx::PgPool,
        redis: redis::aio::ConnectionManager,
        providers: Vec<Box<dyn PaymentProvider>>,
        settings: config::settings::Settings,
    ) -> Self {
        Self {
            repos: Repositories::new(db_pool),
            redis,
            providers: Arc::new(RwLock::new(providers)),
            settings,
        }
    }
}

/// Type alias for shared state in Axum handlers.
pub type SharedState = Arc<AppState>;
