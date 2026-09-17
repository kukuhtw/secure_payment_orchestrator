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
    pub db_pool: sqlx::PgPool,
    pub redis: redis::aio::ConnectionManager,
    pub providers: Arc<RwLock<Vec<Box<dyn PaymentProvider>>>>,
    pub settings: config::settings::Settings,
    pub payment_service: application::payment::PaymentService,
}

impl AppState {
    pub fn new(
        db_pool: sqlx::PgPool,
        redis: redis::aio::ConnectionManager,
        providers: Vec<Box<dyn PaymentProvider>>,
        settings: config::settings::Settings,
    ) -> Self {
        let repos = Repositories::new(db_pool.clone());
        let providers = Arc::new(RwLock::new(providers));
        let payment_service = application::payment::PaymentService::new(
            repos.payment.clone(),
            repos.attempt.clone(),
            repos.audit_log.clone(),
            providers.clone(),
        );

        Self {
            repos,
            db_pool,
            redis,
            providers,
            settings,
            payment_service,
        }
    }
}

/// Type alias for shared state in Axum handlers.
pub type SharedState = Arc<AppState>;
