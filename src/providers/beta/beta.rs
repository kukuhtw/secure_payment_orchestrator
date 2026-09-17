//! Xendit Payment Link (Invoice) provider adapter.
//!
//! The module and Rust type retain the historical `beta`/`BetaProvider` names to
//! minimize internal churn. Externally this provider identifies itself as `XENDIT`.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::providers::adapter::*;

pub struct BetaProvider {
    name: String,
    secret_key: String,
    base_url: String,
    client: Client,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
struct CreateInvoiceRequest<'a> {
    external_id: &'a str,
    amount: i64,
    currency: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    success_redirect_url: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
struct InvoiceResponse {
    id: String,
    status: String,
    #[serde(default)]
    invoice_url: Option<String>,
}

impl BetaProvider {
    pub fn new(secret_key: &str, base_url: &str, timeout_seconds: u64) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()?;

        Ok(Self {
            name: "XENDIT".into(),
            secret_key: secret_key.into(),
            base_url: base_url.trim_end_matches('/').into(),
            client,
            timeout_seconds,
        })
    }

    fn ensure_configured(&self) -> Result<(), ProviderError> {
        if self.secret_key.trim().is_empty() {
            return Err(ProviderError::Unavailable);
        }
        Ok(())
    }

    fn invoice_url(&self, invoice_id: Option<&str>) -> Result<Url, ProviderError> {
        let mut url = Url::parse(&self.base_url)
            .map_err(|error| ProviderError::Network(format!("Invalid Xendit API URL: {error}")))?;
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ProviderError::InvalidResponse)?;
        segments.extend(["v2", "invoices"]);
        if let Some(invoice_id) = invoice_id {
            segments.push(invoice_id);
        }
        drop(segments);
        Ok(url)
    }

    async fn error_from_response(response: reqwest::Response) -> ProviderError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| {
            json!({
                "raw_body": body
            })
        });

        ProviderError::Provider {
            code: parsed
                .get("error_code")
                .and_then(Value::as_str)
                .unwrap_or(status.as_str())
                .to_owned(),
            message: parsed
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Xendit request failed")
                .to_owned(),
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

    fn normalize_status(status: &str) -> &'static str {
        match status {
            "PAID" | "SETTLED" => "COMPLETED",
            "PENDING" => "PENDING",
            "EXPIRED" | "FAILED" => "FAILED",
            _ => "UNKNOWN",
        }
    }
}

#[async_trait]
impl PaymentProvider for BetaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        !self.secret_key.trim().is_empty()
    }

    async fn create_payment(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        if request.currency != "IDR" {
            return Err(ProviderError::Provider {
                code: "UNSUPPORTED_CURRENCY".into(),
                message: "Xendit adapter currently supports IDR only".into(),
                http_status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            });
        }

        let payload = CreateInvoiceRequest {
            external_id: &request.merchant_reference,
            amount: request.amount,
            currency: &request.currency,
            description: request.description.as_deref(),
            success_redirect_url: request.callback_url.as_deref(),
        };

        let response = self
            .client
            .post(self.invoice_url(None)?)
            .basic_auth(&self.secret_key, Some(""))
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
        let parsed: InvoiceResponse =
            serde_json::from_value(raw).map_err(|_| ProviderError::InvalidResponse)?;

        let payment_url = parsed.invoice_url.ok_or(ProviderError::InvalidResponse)?;
        Ok(ProviderResponse {
            provider_payment_id: parsed.id,
            provider_status: Self::normalize_status(&parsed.status).into(),
            payment_url: Some(payment_url.clone()),
            raw_response: Some(json!({
                "invoice_url": payment_url,
                "status": parsed.status
            })),
        })
    }

    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        let response = self
            .client
            .get(self.invoice_url(Some(provider_payment_id))?)
            .basic_auth(&self.secret_key, Some(""))
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
        let parsed: InvoiceResponse =
            serde_json::from_value(raw.clone()).map_err(|_| ProviderError::InvalidResponse)?;

        Ok(ProviderResponse {
            provider_payment_id: parsed.id,
            provider_status: Self::normalize_status(&parsed.status).into(),
            payment_url: parsed.invoice_url,
            raw_response: Some(raw),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::BetaProvider;

    #[test]
    fn maps_xendit_invoice_statuses() {
        assert_eq!(BetaProvider::normalize_status("PAID"), "COMPLETED");
        assert_eq!(BetaProvider::normalize_status("SETTLED"), "COMPLETED");
        assert_eq!(BetaProvider::normalize_status("PENDING"), "PENDING");
        assert_eq!(BetaProvider::normalize_status("EXPIRED"), "FAILED");
        assert_eq!(BetaProvider::normalize_status("NEW_STATUS"), "UNKNOWN");
    }
}
