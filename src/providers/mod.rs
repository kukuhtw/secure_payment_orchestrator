pub mod adapter;
pub mod alpha;
pub mod beta;
pub mod gamma;

use crate::config::settings::Settings;

/// Build all configured provider adapters.
pub fn build_providers(settings: &Settings) -> anyhow::Result<Vec<Box<dyn adapter::PaymentProvider>>> {
    let providers: Vec<Box<dyn adapter::PaymentProvider>> = vec![
        Box::new(alpha::AlphaProvider::new(
            &settings.webhook_secret_alpha,
        )),
        Box::new(beta::BetaProvider::new(
            &settings.webhook_secret_beta,
        )),
        Box::new(gamma::GammaProvider::new(
            &settings.webhook_secret_gamma,
        )),
    ];

    Ok(providers)
}