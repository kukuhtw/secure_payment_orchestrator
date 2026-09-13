//! State transition rules and business rules engine.

use crate::domain::status::PaymentStatus;
use crate::domain::error::DomainError;

/// Validasi transisi status berdasarkan transition matrix.
pub fn validate_transition(
    from: PaymentStatus,
    to: PaymentStatus,
) -> Result<(), DomainError> {
    let allowed = match (from, to) {
        // PENDING → PROCESSING
        (PaymentStatus::Pending, PaymentStatus::Processing) => true,
        // PROCESSING → any
        (PaymentStatus::Processing, PaymentStatus::Success) => true,
        (PaymentStatus::Processing, PaymentStatus::Failed) => true,
        (PaymentStatus::Processing, PaymentStatus::PendingRetry) => true,
        (PaymentStatus::Processing, PaymentStatus::PendingReconciliation) => true,
        // PENDING_RETRY → PROCESSING
        (PaymentStatus::PendingRetry, PaymentStatus::Processing) => true,
        // PENDING_RECONCILIATION → any
        (PaymentStatus::PendingReconciliation, PaymentStatus::Processing) => true,
        (PaymentStatus::PendingReconciliation, PaymentStatus::Success) => true,
        (PaymentStatus::PendingReconciliation, PaymentStatus::Failed) => true,
        // Any non-final → CANCELLED (kecuali SUCCESS/FAILED)
        (_, PaymentStatus::Cancelled)
            if from != PaymentStatus::Success && from != PaymentStatus::Failed => true,
        _ => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(DomainError::invalid_transition(from, to))
    }
}

/// Klasifikasi apakah error dapat di-retry.
pub fn is_retryable_http_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 502 | 503 | 504)
}

pub fn is_retryable_error(error_code: &str) -> bool {
    matches!(error_code, "TIMEOUT" | "PROVIDER_UNAVAILABLE" | "RATE_LIMITED")
}

/// Maximum retry count sebelum masuk ke reconciliation.
pub const MAX_RETRY_ATTEMPTS: i32 = 5;

/// Exponential backoff delay (detik) untuk attempt ke-n.
pub fn retry_delay_seconds(attempt: i32) -> u64 {
    let base: u64 = 2;
    let exp = (attempt - 1).max(0) as u32;
    (base.pow(exp)).min(60) // max 60 detik
}