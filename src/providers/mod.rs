pub mod adapter;
pub mod alpha;
pub mod beta;
pub mod gamma;
pub mod nicepay;

use crate::config::settings::Settings;

/// Build all configured provider adapters.
pub fn build_providers(
    settings: &Settings,
) -> anyhow::Result<Vec<Box<dyn adapter::PaymentProvider>>> {
    let providers: Vec<Box<dyn adapter::PaymentProvider>> = vec![
        Box::new(alpha::AlphaProvider::new(
            &settings.midtrans_server_key,
            &settings.midtrans_snap_base_url,
            &settings.midtrans_core_base_url,
            settings.midtrans_timeout_seconds,
        )?),
        Box::new(beta::BetaProvider::new(
            &settings.xendit_secret_key,
            &settings.xendit_base_url,
            settings.xendit_timeout_seconds,
        )?),
        Box::new(gamma::GammaProvider::new(
            &settings.doku_client_id,
            &settings.doku_secret_key,
            &settings.doku_base_url,
            settings.doku_timeout_seconds,
        )?),
        Box::new(nicepay::NicepayProvider::new(
            &settings.nicepay_imid,
            &settings.nicepay_merchant_key,
            &settings.nicepay_base_url,
            &settings.nicepay_pay_method,
            settings.nicepay_timeout_seconds,
        )?),
    ];

    Ok(providers)
}
