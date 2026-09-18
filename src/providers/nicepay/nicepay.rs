//! NICEPAY Professional/Checkout v1 example adapter.
//!
//! NICEPAY products can have different required fields and payment flows. This adapter
//! demonstrates registration for a merchant/payment method configured in the environment.
//! Inquiry is intentionally guarded until the canonical status contract carries the extra
//! reference and amount required by NICEPAY. Confirm the activated product contract before
//! using it outside Sandbox.

use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::providers::adapter::*;

const REGISTRATION_TARGET: &str = "/nicepay/api/v1.0/registration";

pub struct NicepayProvider {
    name: String,
    imid: String,
    merchant_key: String,
    base_url: String,
    pay_method: String,
    client: Client,
    timeout_seconds: u64,
    priority: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationRequest<'a> {
    time_stamp: String,
    i_mid: &'a str,
    pay_method: &'a str,
    currency: &'a str,
    amt: String,
    reference_no: &'a str,
    goods_nm: &'a str,
    billing_nm: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    call_back_url: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    db_process_url: Option<&'a str>,
    merchant_token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistrationResponse {
    result_cd: String,
    #[serde(default)]
    result_msg: Option<String>,
    #[serde(default)]
    t_xid: Option<String>,
    #[serde(default)]
    payment_url: Option<String>,
}

impl NicepayProvider {
    pub fn new(
        imid: &str,
        merchant_key: &str,
        base_url: &str,
        pay_method: &str,
        timeout_seconds: u64,
        priority: i32,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_seconds))
            .build()?;

        Ok(Self {
            name: "NICEPAY".into(),
            imid: imid.into(),
            merchant_key: merchant_key.into(),
            base_url: base_url.trim_end_matches('/').into(),
            pay_method: pay_method.into(),
            client,
            timeout_seconds,
            priority,
        })
    }

    fn ensure_configured(&self) -> Result<(), ProviderError> {
        if self.imid.trim().is_empty() || self.merchant_key.trim().is_empty() {
            return Err(ProviderError::Unavailable);
        }
        Ok(())
    }

    fn endpoint(&self, target: &str) -> Result<Url, ProviderError> {
        Url::parse(&format!("{}{}", self.base_url, target))
            .map_err(|error| ProviderError::Network(format!("Invalid NICEPAY API URL: {error}")))
    }

    fn timestamp() -> String {
        Utc::now().format("%Y%m%d%H%M%S").to_string()
    }

    fn merchant_token(&self, timestamp: &str, reference_no: &str, amount: &str) -> String {
        let component = format!(
            "{}{}{}{}{}",
            timestamp, self.imid, reference_no, amount, self.merchant_key
        );
        hex::encode(Sha256::digest(component.as_bytes()))
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
                .get("resultCd")
                .or_else(|| parsed.get("resultCode"))
                .and_then(Value::as_str)
                .unwrap_or(status.as_str())
                .to_owned(),
            message: parsed
                .get("resultMsg")
                .or_else(|| parsed.get("resultMessage"))
                .and_then(Value::as_str)
                .unwrap_or("NICEPAY request failed")
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
            "0" | "SUCCESS" | "PAID" => "COMPLETED",
            "9" | "PENDING" | "PROCESSING" => "PENDING",
            "1" | "2" | "3" | "FAILED" | "EXPIRED" | "CANCELLED" => "FAILED",
            "REFUNDED" | "PARTIAL_REFUND" => "REFUNDED",
            _ => "UNKNOWN",
        }
    }
}

#[async_trait]
impl PaymentProvider for NicepayProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        !self.imid.trim().is_empty() && !self.merchant_key.trim().is_empty()
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
                message: "NICEPAY example adapter currently supports IDR only".into(),
                http_status: StatusCode::UNPROCESSABLE_ENTITY.as_u16(),
            });
        }

        let timestamp = Self::timestamp();
        let amount = request.amount.to_string();
        let description = request
            .description
            .as_deref()
            .unwrap_or(&request.merchant_reference);
        let payload = RegistrationRequest {
            time_stamp: timestamp.clone(),
            i_mid: &self.imid,
            pay_method: &self.pay_method,
            currency: &request.currency,
            amt: amount.clone(),
            reference_no: &request.merchant_reference,
            goods_nm: description,
            billing_nm: &request.merchant_reference,
            call_back_url: request.callback_url.as_deref(),
            db_process_url: request.callback_url.as_deref(),
            merchant_token: self.merchant_token(&timestamp, &request.merchant_reference, &amount),
        };

        let response = self
            .client
            .post(self.endpoint(REGISTRATION_TARGET)?)
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
        let parsed: RegistrationResponse =
            serde_json::from_value(raw).map_err(|_| ProviderError::InvalidResponse)?;
        if parsed.result_cd != "0000" {
            return Err(ProviderError::Provider {
                code: parsed.result_cd,
                message: parsed
                    .result_msg
                    .unwrap_or_else(|| "NICEPAY registration failed".into()),
                http_status: StatusCode::BAD_GATEWAY.as_u16(),
            });
        }

        let transaction_id = parsed.t_xid.ok_or(ProviderError::InvalidResponse)?;
        let payment_url = parsed.payment_url.ok_or(ProviderError::InvalidResponse)?;
        Ok(ProviderResponse {
            provider_payment_id: transaction_id,
            provider_status: "PENDING".into(),
            payment_url: Some(payment_url.clone()),
            raw_response: Some(json!({ "payment_url": payment_url })),
        })
    }

    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError> {
        self.ensure_configured()?;

        // The current canonical provider contract only supplies provider_payment_id.
        // NICEPAY inquiry signatures also require referenceNo and amt, so a production
        // implementation must load those values from the PaymentAttempt/Payment records.
        Err(ProviderError::Provider {
            code: "INQUIRY_CONTEXT_REQUIRED".into(),
            message: format!(
                "NICEPAY inquiry for {provider_payment_id} requires referenceNo and amt"
            ),
            http_status: StatusCode::NOT_IMPLEMENTED.as_u16(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::NicepayProvider;

    #[test]
    fn maps_nicepay_statuses() {
        assert_eq!(NicepayProvider::normalize_status("0"), "COMPLETED");
        assert_eq!(NicepayProvider::normalize_status("PENDING"), "PENDING");
        assert_eq!(NicepayProvider::normalize_status("EXPIRED"), "FAILED");
        assert_eq!(NicepayProvider::normalize_status("REFUNDED"), "REFUNDED");
        assert_eq!(NicepayProvider::normalize_status("NEW_STATUS"), "UNKNOWN");
    }
}
