//! State transition rules and business rules engine.

use crate::domain::error::DomainError;
use crate::domain::status::PaymentStatus;

/// Validasi transisi status berdasarkan transition matrix.
///
/// Payment yang sudah final (`SUCCESS`/`FAILED`/`CANCELLED`) selalu ditolak
/// lebih dulu dengan `DomainError::AlreadyFinal` — kode error API yang lebih
/// spesifik (`PAYMENT_ALREADY_FINAL`) daripada `InvalidTransition` generik,
/// sesuai `documentation/api/API-Contract-Secure-Payment-Orchestrator.md`.
pub fn validate_transition(from: PaymentStatus, to: PaymentStatus) -> Result<(), DomainError> {
    if from.is_final() {
        return Err(DomainError::already_final(from));
    }

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
        // Any non-final (guaranteed by the is_final() check above) → CANCELLED
        (_, PaymentStatus::Cancelled) => true,
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
    matches!(
        error_code,
        "TIMEOUT" | "PROVIDER_UNAVAILABLE" | "RATE_LIMITED"
    )
}

/// Maximum retry count sebelum masuk ke reconciliation.
pub const MAX_RETRY_ATTEMPTS: i32 = 5;

/// Exponential backoff delay (detik) untuk attempt ke-n.
pub fn retry_delay_seconds(attempt: i32) -> u64 {
    let base: u64 = 2;
    let exp = (attempt - 1).max(0) as u32;
    (base.pow(exp)).min(60) // max 60 detik
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_pending_to_processing() {
        assert!(validate_transition(PaymentStatus::Pending, PaymentStatus::Processing).is_ok());
    }

    #[test]
    fn allows_processing_to_success() {
        assert!(validate_transition(PaymentStatus::Processing, PaymentStatus::Success).is_ok());
    }

    #[test]
    fn allows_non_final_to_cancelled() {
        for from in [
            PaymentStatus::Pending,
            PaymentStatus::Processing,
            PaymentStatus::PendingRetry,
            PaymentStatus::PendingReconciliation,
        ] {
            assert!(
                validate_transition(from, PaymentStatus::Cancelled).is_ok(),
                "{from} -> Cancelled should be allowed"
            );
        }
    }

    #[test]
    fn rejects_transition_from_final_status_as_already_final() {
        for from in [
            PaymentStatus::Success,
            PaymentStatus::Failed,
            PaymentStatus::Cancelled,
        ] {
            let err = validate_transition(from, PaymentStatus::Cancelled).unwrap_err();
            assert!(
                matches!(err, DomainError::AlreadyFinal(s) if s == from),
                "{from} -> Cancelled should be AlreadyFinal, got {err:?}"
            );
        }
    }

    #[test]
    fn rejects_disallowed_transition_as_invalid_transition() {
        let err = validate_transition(PaymentStatus::Pending, PaymentStatus::Success).unwrap_err();
        assert!(matches!(
            err,
            DomainError::InvalidTransition {
                from: PaymentStatus::Pending,
                to: PaymentStatus::Success
            }
        ));
    }
}
