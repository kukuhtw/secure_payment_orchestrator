//! Payment status enum with state machine transitions.

use std::fmt;

/// Status yang mungkin dalam lifecycle pembayaran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentStatus {
    Pending,
    Processing,
    Success,
    Failed,
    PendingRetry,
    PendingReconciliation,
    Cancelled,
}

impl PaymentStatus {
    /// Status final tidak dapat diubah lagi.
    pub fn is_final(&self) -> bool {
        matches!(self, Self::Success | Self::Failed | Self::Cancelled)
    }

    /// Status yang memerlukan rekonsiliasi.
    pub fn needs_reconciliation(&self) -> bool {
        matches!(self, Self::PendingReconciliation)
    }

    /// Status yang dapat di-retry.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::PendingRetry | Self::Failed)
    }
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "PENDING"),
            Self::Processing => write!(f, "PROCESSING"),
            Self::Success => write!(f, "SUCCESS"),
            Self::Failed => write!(f, "FAILED"),
            Self::PendingRetry => write!(f, "PENDING_RETRY"),
            Self::PendingReconciliation => write!(f, "PENDING_RECONCILIATION"),
            Self::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

impl TryFrom<&str> for PaymentStatus {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_uppercase().as_str() {
            "PENDING" => Ok(Self::Pending),
            "PROCESSING" => Ok(Self::Processing),
            "SUCCESS" => Ok(Self::Success),
            "FAILED" => Ok(Self::Failed),
            "PENDING_RETRY" => Ok(Self::PendingRetry),
            "PENDING_RECONCILIATION" => Ok(Self::PendingReconciliation),
            "CANCELLED" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid payment status: {}", s)),
        }
    }
}
