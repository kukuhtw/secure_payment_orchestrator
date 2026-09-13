//! Domain errors.

use crate::domain::status::PaymentStatus;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DomainError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Invalid payment status: {0}")]
    InvalidStatus(String),

    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition {
        from: PaymentStatus,
        to: PaymentStatus,
    },

    #[error("Payment not found: {0}")]
    NotFound(String),

    #[error("Payment already in final state: {0}")]
    AlreadyFinal(PaymentStatus),

    #[error("Conflict: {0}")]
    Conflict(String),
}

impl DomainError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn invalid_transition(from: PaymentStatus, to: PaymentStatus) -> Self {
        Self::InvalidTransition { from, to }
    }

    pub fn already_final(status: PaymentStatus) -> Self {
        Self::AlreadyFinal(status)
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }
}