//! Circuit breaker decorator for [`PaymentProvider`] — wraps any provider
//! adapter and tracks consecutive call failures to temporarily mark it
//! unavailable, per architecture doc §6.3 ("failure threshold: 5 consecutive
//! failures, open duration: 30s, half-open probe: allow 1 request").
//!
//! `providers::build_providers` wraps every adapter with this, reading the
//! threshold/duration from `Settings::circuit_breaker_threshold`/
//! `circuit_breaker_timeout_seconds` — config that existed since early in
//! this project but was never actually consulted by any runtime code until
//! now.
//!
//! State is tracked as two variants, not three: `Closed` and `Open`. The
//! architecture doc's `HALF_OPEN` is represented as "`Open`, but
//! `open_duration` has elapsed" rather than a separate stored state —
//! `is_available()` computes this from `Instant::elapsed()` on every call
//! without mutating anything. This is a deliberate simplification: a
//! literal third state would need to also track "is a probe currently
//! in-flight" to honor "allow 1 request" under concurrent callers, which
//! this single-process, in-memory breaker does not attempt. In practice
//! this means multiple concurrent requests arriving right after
//! `open_duration` elapses may all be allowed through as probes, not
//! strictly one — accepted as a narrow relaxation of the spec, not a
//! correctness bug for the sequential per-request selection this codebase
//! does today (`PaymentService::select_best_provider`).
//!
//! Also POC-scoped: state lives only in this process's memory, not Redis
//! or the database — a multi-instance deployment would need shared state
//! to coordinate circuit status across processes, which is out of scope.

use async_trait::async_trait;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::adapter::{PaymentProvider, ProviderError, ProviderRequest, ProviderResponse};

enum CircuitState {
    Closed { consecutive_failures: u32 },
    Open { opened_at: Instant },
}

pub struct CircuitBreakerProvider {
    inner: Box<dyn PaymentProvider>,
    state: Mutex<CircuitState>,
    failure_threshold: u32,
    open_duration: Duration,
}

impl CircuitBreakerProvider {
    pub fn new(
        inner: Box<dyn PaymentProvider>,
        failure_threshold: u32,
        open_duration: Duration,
    ) -> Self {
        Self {
            inner,
            state: Mutex::new(CircuitState::Closed {
                consecutive_failures: 0,
            }),
            failure_threshold,
            open_duration,
        }
    }

    fn record_success(&self) {
        let mut state = self.state.lock().unwrap();
        *state = CircuitState::Closed {
            consecutive_failures: 0,
        };
    }

    fn record_failure(&self) {
        let mut state = self.state.lock().unwrap();
        *state = match &*state {
            CircuitState::Closed {
                consecutive_failures,
            } => {
                let failures = consecutive_failures + 1;
                if failures >= self.failure_threshold {
                    CircuitState::Open {
                        opened_at: Instant::now(),
                    }
                } else {
                    CircuitState::Closed {
                        consecutive_failures: failures,
                    }
                }
            }
            // A failed probe (a call made while OPEN's duration had already
            // elapsed) re-opens the circuit for another full duration —
            // matches "HALF_OPEN -> OPEN: probe failure".
            CircuitState::Open { .. } => CircuitState::Open {
                opened_at: Instant::now(),
            },
        };
    }
}

#[async_trait]
impl PaymentProvider for CircuitBreakerProvider {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn priority(&self) -> i32 {
        self.inner.priority()
    }

    fn is_available(&self) -> bool {
        if !self.inner.is_available() {
            return false;
        }

        let state = self.state.lock().unwrap();
        match *state {
            CircuitState::Closed { .. } => true,
            CircuitState::Open { opened_at } => opened_at.elapsed() >= self.open_duration,
        }
    }

    async fn create_payment(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse, ProviderError> {
        match self.inner.create_payment(request).await {
            Ok(response) => {
                self.record_success();
                Ok(response)
            }
            Err(err) => {
                self.record_failure();
                Err(err)
            }
        }
    }

    async fn get_payment_status(
        &self,
        provider_payment_id: &str,
    ) -> Result<ProviderResponse, ProviderError> {
        match self.inner.get_payment_status(provider_payment_id).await {
            Ok(response) => {
                self.record_success();
                Ok(response)
            }
            Err(err) => {
                self.record_failure();
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    struct FakeInner {
        available: bool,
        should_fail: Arc<AtomicBool>,
    }

    #[async_trait]
    impl PaymentProvider for FakeInner {
        fn name(&self) -> &str {
            "FAKE"
        }

        fn priority(&self) -> i32 {
            10
        }

        fn is_available(&self) -> bool {
            self.available
        }

        async fn create_payment(
            &self,
            _request: ProviderRequest,
        ) -> Result<ProviderResponse, ProviderError> {
            if self.should_fail.load(Ordering::SeqCst) {
                Err(ProviderError::Unavailable)
            } else {
                Ok(sample_response())
            }
        }

        async fn get_payment_status(
            &self,
            _provider_payment_id: &str,
        ) -> Result<ProviderResponse, ProviderError> {
            unimplemented!("not exercised by these tests")
        }
    }

    fn sample_response() -> ProviderResponse {
        ProviderResponse {
            provider_payment_id: "fake_1".into(),
            provider_status: "PENDING".into(),
            payment_url: None,
            raw_response: None,
        }
    }

    fn sample_request() -> ProviderRequest {
        ProviderRequest {
            amount: 1000,
            currency: "IDR".into(),
            merchant_reference: "REF".into(),
            description: None,
            callback_url: None,
        }
    }

    fn breaker(
        failing: bool,
        failure_threshold: u32,
        open_duration: Duration,
    ) -> (CircuitBreakerProvider, Arc<AtomicBool>) {
        let should_fail = Arc::new(AtomicBool::new(failing));
        let inner = FakeInner {
            available: true,
            should_fail: should_fail.clone(),
        };
        (
            CircuitBreakerProvider::new(Box::new(inner), failure_threshold, open_duration),
            should_fail,
        )
    }

    #[test]
    fn starts_closed_and_available() {
        let (cb, _) = breaker(false, 3, Duration::from_secs(60));
        assert!(cb.is_available());
    }

    #[test]
    fn delegates_name_and_priority_to_inner() {
        let (cb, _) = breaker(false, 3, Duration::from_secs(60));
        assert_eq!(cb.name(), "FAKE");
        assert_eq!(cb.priority(), 10);
    }

    #[test]
    fn unavailable_when_inner_is_unavailable_regardless_of_circuit_state() {
        let should_fail = Arc::new(AtomicBool::new(false));
        let inner = FakeInner {
            available: false,
            should_fail,
        };
        let cb = CircuitBreakerProvider::new(Box::new(inner), 3, Duration::from_secs(60));
        assert!(!cb.is_available());
    }

    #[tokio::test]
    async fn stays_closed_below_failure_threshold() {
        let (cb, _) = breaker(true, 3, Duration::from_secs(60));

        let _ = cb.create_payment(sample_request()).await;
        let _ = cb.create_payment(sample_request()).await;

        assert!(cb.is_available());
    }

    #[tokio::test]
    async fn opens_after_threshold_consecutive_failures() {
        let (cb, _) = breaker(true, 3, Duration::from_secs(60));

        for _ in 0..3 {
            let _ = cb.create_payment(sample_request()).await;
        }

        assert!(!cb.is_available());
    }

    #[tokio::test]
    async fn success_resets_consecutive_failure_count() {
        let (cb, should_fail) = breaker(true, 3, Duration::from_secs(60));

        let _ = cb.create_payment(sample_request()).await; // failure 1
        let _ = cb.create_payment(sample_request()).await; // failure 2

        should_fail.store(false, Ordering::SeqCst);
        let _ = cb.create_payment(sample_request()).await; // success, resets to 0
        assert!(cb.is_available());

        should_fail.store(true, Ordering::SeqCst);
        let _ = cb.create_payment(sample_request()).await; // failure 1 (post-reset)
        let _ = cb.create_payment(sample_request()).await; // failure 2 (post-reset)
                                                           // Threshold is 3 and the counter was reset — two more failures
                                                           // alone must not reopen it.
        assert!(cb.is_available());
    }

    #[tokio::test]
    async fn becomes_available_again_as_a_probe_after_open_duration_elapses() {
        let (cb, _) = breaker(true, 2, Duration::from_millis(20));

        let _ = cb.create_payment(sample_request()).await;
        let _ = cb.create_payment(sample_request()).await;
        assert!(!cb.is_available());

        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(cb.is_available());
    }

    #[tokio::test]
    async fn failed_probe_reopens_the_circuit() {
        let (cb, _) = breaker(true, 2, Duration::from_millis(20));

        let _ = cb.create_payment(sample_request()).await;
        let _ = cb.create_payment(sample_request()).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(cb.is_available()); // probe window open

        let _ = cb.create_payment(sample_request()).await; // probe fails
        assert!(!cb.is_available()); // re-opened
    }

    #[tokio::test]
    async fn successful_probe_closes_the_circuit() {
        let (cb, should_fail) = breaker(true, 2, Duration::from_millis(20));

        let _ = cb.create_payment(sample_request()).await;
        let _ = cb.create_payment(sample_request()).await;
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(cb.is_available()); // probe window open

        should_fail.store(false, Ordering::SeqCst);
        let _ = cb.create_payment(sample_request()).await; // probe succeeds

        // Back to a fresh Closed{0} — a single subsequent failure (below
        // the threshold of 2) must not reopen it immediately.
        should_fail.store(true, Ordering::SeqCst);
        let _ = cb.create_payment(sample_request()).await;
        assert!(cb.is_available());
    }
}
