//! DOKU Checkout provider adapter.
//!
//! The module and Rust type retain the historical `gamma`/`GammaProvider` names to
//! minimize internal churn. Externally this provider identifies itself as `DOKU`.

use std::time::Duration;

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use reqwest::{Client, Method, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::providers::adapter::*;

type HmacSha256 = Hmac<Sha256>;

const CREATE_PAYMENT_TARGET: &str = "/checkout/v1/payment";
const STATUS_TARGET_PREFIX: &str = "/orders/v1/status";

pub struct GammaProvider {
    name: String,
    client_id: String,
    secret_key: String,
    base_url: String,
    client: Client,
    timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
struct CheckoutRequest<'a> {
    order: CheckoutOrder<'a>,
    payment: CheckoutPayment,
}

#[derive(Debug, Serialize)]
struct CheckoutOrder<'a> {
    amount: i64,
    invoice_number: &'a str,
    currency: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callback_url_result: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct CheckoutPayment {
    payment_due_date: u32,
}

#[derive(Debug, Deserialize)]
struct CheckoutResponse {
    order: CheckoutOrderResponse,
    payment: CheckoutPaymentResponse,
}

#[derive(Debug, Deserialize)]
struct CheckoutOrderResponse {
    invoice_number: String,
}

#[derive(Debug, Deserialize)]
struct CheckoutPaymentResponse {
    payment_url: String,
}

impl GammaProvider {
    pub fn new(
        client_id: &str,
        secret_key: &str,
        base_url: &str,
        timeout_seconds: u64,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()?;

        Ok(Self {
            name: "DOKU".into(),
            client_id: client_id.into(),
            secret_key: secret_key.into(),
            base_url: base_url.trim_end_matches('/').into(),
            client,
            timeout_seconds,
        })
    }

    fn ensure_configured(&self) -> Result<(), ProviderError> {
        if self.client_id.trim().is_empty() || self.secret_key.trim().is_empty() {
            return Err(ProviderError::Unavailable);
        }
        Ok(())
    }

    fn digest(body: &[u8]) -> String {
        BASE64.encode(Sha256::digest(body))
    }

    fn signature(
        &self,
        request_id: &str,
        timestamp: &str,
        request_target: &str,
        digest: Option<&str>,
    ) -> Result<String, ProviderError> {
        let mut component = format!(
            "Client-Id:{}\nRequest-Id:{}\nRequest-Timestamp:{}\nRequest-Target:{}",
            self.client_id, request_id, timestamp, request_target
        );
        if let Some(digest) = digest {
            component.push_str(&format!("\nDigest:{digest}"));
        }

        let mut mac = HmacSha256::new_from_slice(self.secret_key.as_bytes())
            .map_err(|_| ProviderError::InvalidResponse)?;
        mac.update(component.as_bytes());
        Ok(format!(
            "HMACSHA256={}",
            BASE64.encode(mac.finalize().into_bytes())
        ))
    }

    fn request_url(&self, request_target: &str) -> Result<Url, ProviderError> {
        Url::parse(&format!("{}{}", self.base_url, request_target)).map_err(|error| {
            ProviderError::Network(format!("Invalid DOKU API URL: {error}"))
        })
    }

    fn signed_request(
        &self,
        method: Method,
        request_target: &str,
        body: Option<&[u8]>,
    ) -> Result<reqwest::RequestBuilder, ProviderError> {
        let request_id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let digest = body.map(Self::digest);
        let signature = self.signature(
            &request_id,
            &timestamp,
            request_target,
            digest.as_deref(),
        )?;

        let mut request = self
            .client
            .request(method, self.request_url(request_target)?)
            .header("Client-Id", &self.client_id)
            .header("Request-Id", request_id)
            .header("Request-Timestamp", timestamp)
            .header("Signature", signature);

        if let Some(digest) = digest {
            request = request.header("Digest", digest);
        }
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.to_vec());
        }
        Ok(request)
    }

    async fn error_from_response(response: reqwest::Response) -> ProviderError {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let parsed: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({
            "raw_body": body
        }));
        let error = parsed.get("error").unwrap_or(&parsed);

        ProviderError::Provider {
            code: error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or(status.as_str())
                .to_owned(),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("DOKU request failed")
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
            "SUCCESS" | "PAID" | "SETTLED" => "COMPLETED",
            "PENDING" | "PROCESSING" => "PENDING",
            "FAILED" | "EXPIRED" | "CANCELLED" | "CANCELED" => "FAILED",
            "REFUNDED" | "PARTIAL_REFUND" => "REFUNDED",
            _ => "UNKNOWN",
        }
    }

    fn status_from_response(raw: &Value) -> Option<&str> {
        raw.pointer("/transaction/status")
            .or_else(|| raw.pointer("/order/status"))
            .or_else(|| raw.get("status"))
            .and_then(Value::as_str)
    }
}

#[async_trait]
impl PaymentProvider for GammaProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        !self.client_id.trim().is_empty() && !self.secret_key.trim().is_empty()
    }

    async fn create_payment(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        if request.currency != "IDR" {
            return Err(ProviderError::Provider {
                code: "UNSUPPORTED_CURRENCY".into(),
                message: "DOKU adapter currently supports IDR only".into(),
                http_status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            });
        }

        let payload = CheckoutRequest {
            order: CheckoutOrder {
                amount: request.amount,
                invoice_number: &request.merchant_reference,
                currency: &request.currency,
                callback_url: request.callback_url.as_deref(),
                callback_url_result: request.callback_url.as_deref(),
            },
            payment: CheckoutPayment {
                payment_due_date: 60,
            },
        };
        let body = serde_json::to_vec(&payload).map_err(|_| ProviderError::InvalidResponse)?;

        let response = self
            .signed_request(Method::POST, CREATE_PAYMENT_TARGET, Some(&body))?
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
        let parsed: CheckoutResponse = serde_json::from_value(raw)
            .map_err(|_| ProviderError::InvalidResponse)?;

        Ok(ProviderResponse {
            provider_payment_id: parsed.order.invoice_number,
            provider_status: "PENDING".into(),
            payment_url: Some(parsed.payment.payment_url.clone()),
            raw_response: Some(json!({
                "payment_url": parsed.payment.payment_url
            })),
        })
    }

    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        let encoded_id: String = url::form_urlencoded::byte_serialize(provider_payment_id.as_bytes())
            .collect();
        let request_target = format!("{STATUS_TARGET_PREFIX}/{encoded_id}");
        let response = self
            .signed_request(Method::GET, &request_target, None)?
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
        let status = Self::status_from_response(&raw).ok_or(ProviderError::InvalidResponse)?;

        Ok(ProviderResponse {
            provider_payment_id: provider_payment_id.into(),
            provider_status: Self::normalize_status(status).into(),
            payment_url: None,
            raw_response: Some(raw),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::GammaProvider;

    #[test]
    fn maps_doku_transaction_statuses() {
        assert_eq!(GammaProvider::normalize_status("SUCCESS"), "COMPLETED");
        assert_eq!(GammaProvider::normalize_status("PENDING"), "PENDING");
        assert_eq!(GammaProvider::normalize_status("EXPIRED"), "FAILED");
        assert_eq!(GammaProvider::normalize_status("REFUNDED"), "REFUNDED");
        assert_eq!(GammaProvider::normalize_status("NEW_STATUS"), "UNKNOWN");
    }

    #[test]
    fn creates_stable_digest() {
        assert_eq!(
            GammaProvider::digest(br#"{"order":{"amount":10000}}"#),
            "mlDlQXdbvRL3g9/LBXZPKCPcdm5awXDuBB0Cvh9P8XQ="
        );
    }
}
