//! Standardized API error response.
//!
//! Semua error dikembalikan dalam format konsisten:
//! ```json
//! { "error": { "code": "ERROR_CODE", "message": "...", "details": {} } }
//! ```

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct ApiError {
    #[serde(skip)]
    pub status_code: StatusCode,
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, serde_json::Value>>,
}

impl ApiError {
    pub fn new(
        status_code: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            status_code,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: None,
            },
        }
    }

    pub fn with_details(
        status_code: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        details: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            status_code,
            error: ErrorDetail {
                code: code.into(),
                message: message.into(),
                details: Some(details),
            },
        }
    }

    pub fn not_implemented(feature: &str) -> Self {
        Self::new(
            StatusCode::NOT_IMPLEMENTED,
            "NOT_IMPLEMENTED",
            format!("Endpoint {} belum diimplementasikan", feature),
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "VALIDATION_ERROR", message)
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "AUTHENTICATION_FAILED", message)
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        Self::with_details(
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
            format!("{} not found: {}", entity, id),
            HashMap::from([("id".into(), serde_json::Value::String(id.into()))]),
        )
    }

    pub fn conflict(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn unprocessable(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "INVALID_STATUS_TRANSITION",
            message,
        )
    }

    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "Internal server error",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code;
        let body = Json(self);
        (status, body).into_response()
    }
}
