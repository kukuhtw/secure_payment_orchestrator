//! Beta Provider Simulator.
//!
//! Simulasi provider dengan behavior yang dapat dikonfigurasi.

use async_trait::async_trait;
use crate::providers::adapter::*;
use uuid::Uuid;

pub struct BetaProvider {
    name: String,
    webhook_secret: String,
}

impl BetaProvider {
    pub fn new(webhook_secret: &str) -> Self {
        Self {
            name: "BETA".into(),
            webhook_secret: webhook_secret.into(),
        }
    }
}

#[async_trait]
impl PaymentProvider for BetaProvider {
    fn name(&self) -> &str { &self.name }
    fn is_available(&self) -> bool { true }

    async fn create_payment(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        Ok(ProviderResponse {
            provider_payment_id: format!("beta_pay_{}", Uuid::new_v4()),
            provider_status: "PENDING".into(),
            payment_url: Some(format!("http://localhost:9092/pay/{}", Uuid::new_v4())),
            raw_response: None,
        })
    }

    async fn get_payment_status(&self, provider_payment_id: &str) -> Result<ProviderResponse, ProviderError> {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Ok(ProviderResponse {
            provider_payment_id: provider_payment_id.into(),
            provider_status: "COMPLETED".into(),
            payment_url: None,
            raw_response: None,
        })
    }
}