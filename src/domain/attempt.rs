//! Payment attempt entity — records each attempt to call a provider.

use chrono::{DateTime, Utc};
use uuid::Uuid;
use serde_json::Value;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptStatus {
    Success,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptType {
    Initial,
    Retry,
    Reconciliation,
    Failover,
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