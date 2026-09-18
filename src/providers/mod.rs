pub mod adapter;
pub mod alpha;
pub mod beta;
pub mod circuit_breaker;
pub mod gamma;
pub mod nicepay;

use crate::config::settings::Settings;
use circuit_breaker::CircuitBreakerProvider;
use std::time::Duration;

/// Build all configured provider adapters, each wrapped in a
/// [`CircuitBreakerProvider`] (§6.3 architecture doc) using
/// `Settings::circuit_breaker_threshold`/`circuit_breaker_timeout_seconds` —
/// config that previously existed but was never read by any runtime code.
///
/// Registration order here no longer determines selection order —
/// `PaymentService::create_payment`/`retry_payment` pick among available
/// providers by `PaymentProvider::priority()` (lower wins), which each
/// adapter is constructed with below from `Settings::*_priority`. The
/// `vec!` order is now just registration order, not implicit priority.
pub fn build_providers(
    settings: &Settings,
) -> anyhow::Result<Vec<Box<dyn adapter::PaymentProvider>>> {
    let open_duration = Duration::from_secs(settings.circuit_breaker_timeout_seconds);
    let with_circuit_breaker = |inner: Box<dyn adapter::PaymentProvider>| {
        Box::new(CircuitBreakerProvider::new(
            inner,
            settings.circuit_breaker_threshold,
            open_duration,
        )) as Box<dyn adapter::PaymentProvider>
    };

    let providers: Vec<Box<dyn adapter::PaymentProvider>> = vec![
        with_circuit_breaker(Box::new(alpha::AlphaProvider::new(
            &settings.midtrans_server_key,
            &settings.midtrans_snap_base_url,
            &settings.midtrans_core_base_url,
            settings.midtrans_timeout_seconds,
            settings.midtrans_priority,
        )?)),
        with_circuit_breaker(Box::new(beta::BetaProvider::new(
            &settings.xendit_secret_key,
            &settings.xendit_base_url,
            settings.xendit_timeout_seconds,
            settings.xendit_priority,
        )?)),
        with_circuit_breaker(Box::new(gamma::GammaProvider::new(
            &settings.doku_client_id,
            &settings.doku_secret_key,
            &settings.doku_base_url,
            settings.doku_timeout_seconds,
            settings.doku_priority,
        )?)),
        with_circuit_breaker(Box::new(nicepay::NicepayProvider::new(
            &settings.nicepay_imid,
            &settings.nicepay_merchant_key,
            &settings.nicepay_base_url,
            &settings.nicepay_pay_method,
            settings.nicepay_timeout_seconds,
            settings.nicepay_priority,
        )?)),
    ];

    Ok(providers)
}
