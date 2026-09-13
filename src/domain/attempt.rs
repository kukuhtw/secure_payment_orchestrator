//! Payment attempt entity — records each attempt to call a provider.
//!
//! `PaymentAttemptRow` digunakan untuk query SQL (String fields).
//! `PaymentAttempt` adalah domain entity dengan enum types.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde_json::Value;

/// Row type untuk query SQL — semua field String.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PaymentAttemptRow {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub provider: String,
    pub provider_payment_id: Option<String>,
    pub provider_status: Option<String>,
    pub status: String,
    pub attempt_type: String,
    pub attempt_number: i32,
    pub request_snapshot: Option<Value>,
    pub response_snapshot: Option<Value>,
    pub http_status_code: Option<i32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Domain entity dengan enum types.
#[derive(Debug, Clone)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub payment_id: Uuid,
    pub provider: String,
    pub provider_payment_id: Option<String>,
    pub provider_status: Option<String>,
    pub status: AttemptStatus,
    pub attempt_type: AttemptType,
    pub attempt_number: i32,
    pub request_snapshot: Option<Value>,
    pub response_snapshot: Option<Value>,
    pub http_status_code: Option<i32>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub duration_ms: Option<i64>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl From<PaymentAttemptRow> for PaymentAttempt {
    fn from(row: PaymentAttemptRow) -> Self {
        Self {
            id: row.id,
            payment_id: row.payment_id,
            provider: row.provider,
            provider_payment_id: row.provider_payment_id,
            provider_status: row.provider_status,
            status: AttemptStatus::from(&row.status),
            attempt_type: AttemptType::from(&row.attempt_type),
            attempt_number: row.attempt_number,
            request_snapshot: row.request_snapshot,
            response_snapshot: row.response_snapshot,
            http_status_code: row.http_status_code,
            error_code: row.error_code,
            error_message: row.error_message,
            duration_ms: row.duration_ms,
            started_at: row.started_at,
            completed_at: row.completed_at,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    Success,
    Failed,
    Timeout,
}

impl From<&str> for AttemptStatus {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "SUCCESS" => Self::Success,
            "FAILED" => Self::Failed,
            "TIMEOUT" => Self::Timeout,
            _ => Self::Failed,
        }
    }
}

impl std::fmt::Display for AttemptStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "SUCCESS"),
            Self::Failed => write!(f, "FAILED"),
            Self::Timeout => write!(f, "TIMEOUT"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptType {
    Initial,
    Retry,
    Reconciliation,
    Failover,
}

impl From<&str> for AttemptType {
    fn from(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "INITIAL" => Self::Initial,
            "RETRY" => Self::Retry,
            "RECONCILIATION" => Self::Reconciliation,
            "FAILOVER" => Self::Failover,
            _ => Self::Initial,
        }
    }
}

impl std::fmt::Display for AttemptType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initial => write!(f, "INITIAL"),
            Self::Retry => write!(f, "RETRY"),
            Self::Reconciliation => write!(f, "RECONCILIATION"),
            Self::Failover => write!(f, "FAILOVER"),
        }
    }
}