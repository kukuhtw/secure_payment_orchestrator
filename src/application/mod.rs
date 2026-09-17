pub mod audit;
pub mod payment;
pub mod provider;
pub mod reconciliation;
pub mod webhook;

use crate::domain::error::DomainError;
use crate::providers::adapter::ProviderError;

/// Errors surfaced by the application (orchestration) layer — a superset of
/// domain and provider errors, plus orchestration-specific failure modes.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("Provider error: {0}")]
    Provider(#[from] ProviderError),

    #[error("No payment provider is currently available")]
    NoProviderAvailable,
}
