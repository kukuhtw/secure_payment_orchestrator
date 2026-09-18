//! Midtrans provider adapter.
//!
//! The module and Rust type retain the historical `alpha`/`AlphaProvider` names to
//! minimize internal churn. Externally this provider identifies itself as `MIDTRANS`.
//! Payment creation uses Midtrans Snap and status lookup uses the Midtrans Core API.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::providers::adapter::*;

pub struct AlphaProvider {
    name: String,
    server_key: String,
    snap_base_url: String,
    core_base_url: String,
    client: Client,
    timeout_seconds: u64,
    priority: i32,
}

#[derive(Debug, Serialize)]
struct SnapTransactionRequest<'a> {
    transaction_details: TransactionDetails<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callbacks: Option<Callbacks<'a>>,
}

#[derive(Debug, Serialize)]
struct TransactionDetails<'a> {
    order_id: &'a str,
    gross_amount: i64,
}

#[derive(Debug, Serialize)]
struct Callbacks<'a> {
    finish: &'a str,
}

#[derive(Debug, Deserialize)]
struct SnapTransactionResponse {
    redirect_url: String,
}

impl AlphaProvider {
    pub fn new(
        server_key: &str,
        snap_base_url: &str,
        core_base_url: &str,
        timeout_seconds: u64,
        priority: i32,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()?;

        Ok(Self {
            name: "MIDTRANS".into(),
            server_key: server_key.into(),
            snap_base_url: snap_base_url.trim_end_matches('/').into(),
            core_base_url: core_base_url.trim_end_matches('/').into(),
            client,
            timeout_seconds,
            priority,
        })
    }

    fn ensure_configured(&self) -> Result<(), ProviderError> {
        if self.server_key.trim().is_empty() {
            return Err(ProviderError::Unavailable);
        }
        Ok(())
    }

    async fn error_from_response(response: reqwest::Response) -> ProviderError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
            json!({
                "raw_body": body
            })
        });
        let message = parsed
            .get("status_message")
            .and_then(Value::as_str)
            .or_else(|| {
                parsed
                    .get("error_messages")
                    .and_then(Value::as_array)
                    .and_then(|messages| messages.first())
                    .and_then(Value::as_str)
            })
            .unwrap_or("Midtrans request failed")
            .to_owned();

        ProviderError::Provider {
            code: parsed
                .get("status_code")
                .and_then(Value::as_str)
                .unwrap_or(status.as_str())
                .to_owned(),
            message,
            http_status: status.as_u16(),
        }
    }

    fn transport_error(&self, error: reqwest::Error) -> ProviderError {
        if error.is_timeout() {
            ProviderError::Timeout {
                ms: self.timeout_seconds.saturating_mul(1_000),
            }
        } else {
            ProviderError::Network(error.to_string())
        }
    }

    fn normalize_status(status: &str, fraud_status: Option<&str>) -> &'static str {
        match (status, fraud_status) {
            ("capture", Some("accept")) | ("settlement", _) => "COMPLETED",
            ("capture", Some("challenge")) => "PENDING",
            ("capture", Some("deny")) => "FAILED",
            ("capture", _) => "UNKNOWN",
            ("pending", _) => "PENDING",
            ("deny" | "cancel" | "expire" | "failure", _) => "FAILED",
            ("refund" | "partial_refund", _) => "REFUNDED",
            _ => "UNKNOWN",
        }
    }

    fn status_url(&self, order_id: &str) -> Result<Url, ProviderError> {
        let mut url = Url::parse(&self.core_base_url).map_err(|error| {
            ProviderError::Network(format!("Invalid Midtrans Core API URL: {error}"))
        })?;
        url.path_segments_mut()
            .map_err(|_| ProviderError::InvalidResponse)?
            .extend(["v2", order_id, "status"]);
        Ok(url)
    }
}

#[async_trait]
impl PaymentProvider for AlphaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        !self.server_key.trim().is_empty()
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    async fn create_payment(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        if request.currency != "IDR" {
            return Err(ProviderError::Provider {
                code: "UNSUPPORTED_CURRENCY".into(),
                message: "Midtrans adapter currently supports IDR only".into(),
                http_status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            });
        }

        let payload = SnapTransactionRequest {
            transaction_details: TransactionDetails {
                order_id: &request.merchant_reference,
                gross_amount: request.amount,
            },
            callbacks: request
                .callback_url
                .as_deref()
                .map(|finish| Callbacks { finish }),
        };

        let response = self
            .client
            .post(format!("{}/snap/v1/transactions", self.snap_base_url))
            .basic_auth(&self.server_key, Some(""))
            .json(&payload)
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;

        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }

        let raw: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidResponse)?;
        let parsed: SnapTransactionResponse =
            serde_json::from_value(raw.clone()).map_err(|_| ProviderError::InvalidResponse)?;

        let redirect_url = parsed.redirect_url;

        Ok(ProviderResponse {
            // Snap does not return a transaction ID when the token is created. Midtrans
            // status APIs accept the merchant order_id, so it is the stable provider ID.
            provider_payment_id: request.merchant_reference,
            provider_status: "PENDING".into(),
            payment_url: Some(redirect_url.clone()),
            // Do not expose/persist the Snap token through the generic raw response.
            raw_response: Some(json!({
                "redirect_url": redirect_url
            })),
        })
    }

    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        let status_url = self.status_url(provider_payment_id)?;
        let response = self
            .client
            .get(status_url)
            .basic_auth(&self.server_key, Some(""))
            .send()
            .await
            .map_err(|error| self.transport_error(error))?;

        if !response.status().is_success() {
            return Err(Self::error_from_response(response).await);
        }

        let raw: Value = response
            .json()
            .await
            .map_err(|_| ProviderError::InvalidResponse)?;
        let transaction_status = raw
            .get("transaction_status")
            .and_then(Value::as_str)
            .ok_or(ProviderError::InvalidResponse)?;
        let fraud_status = raw.get("fraud_status").and_then(Value::as_str);

        Ok(ProviderResponse {
            provider_payment_id: provider_payment_id.into(),
            provider_status: Self::normalize_status(transaction_status, fraud_status).into(),
            payment_url: None,
            raw_response: Some(raw),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AlphaProvider;

    #[test]
    fn maps_midtrans_transaction_statuses() {
        assert_eq!(
            AlphaProvider::normalize_status("settlement", None),
            "COMPLETED"
        );
        assert_eq!(
            AlphaProvider::normalize_status("capture", Some("accept")),
            "COMPLETED"
        );
        assert_eq!(
            AlphaProvider::normalize_status("capture", Some("challenge")),
            "PENDING"
        );
        assert_eq!(AlphaProvider::normalize_status("capture", None), "UNKNOWN");
        assert_eq!(AlphaProvider::normalize_status("pending", None), "PENDING");
        assert_eq!(AlphaProvider::normalize_status("expire", None), "FAILED");
        assert_eq!(AlphaProvider::normalize_status("refund", None), "REFUNDED");
        assert_eq!(
            AlphaProvider::normalize_status("new_status", None),
            "UNKNOWN"
        );
    }
}
