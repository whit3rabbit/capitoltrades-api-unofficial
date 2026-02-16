//! Rate limiter and retry logic for OpenFEC API requests.
//!
//! Provides a sliding-window rate limiter that proactively paces requests
//! under the OpenFEC hourly budget (1,000 req/hr free tier, default 900 with
//! safety margin), plus an exponential-backoff retry helper for 429 responses.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use rand::Rng;
use tokio::sync::Mutex;
use tokio::time::{sleep, Instant};

use super::error::OpenFecError;

/// Default hourly budget (10% safety margin below the 1,000 free-tier limit).
const DEFAULT_MAX_REQUESTS: u64 = 900;

/// Default sliding window duration.
const DEFAULT_WINDOW: Duration = Duration::from_secs(3600);

/// Sliding-window rate limiter for the OpenFEC API.
///
/// Tracks timestamps of recent requests in a `VecDeque` behind a tokio Mutex.
/// When the window is full, `acquire()` sleeps until the oldest entry expires.
pub struct RateLimiter {
    timestamps: Mutex<VecDeque<Instant>>,
    max_requests: u64,
    window_duration: Duration,
    tracker: RequestTracker,
}

impl RateLimiter {
    /// Create a rate limiter with explicit budget and window.
    pub fn new(max_requests: u64, window_duration: Duration) -> Self {
        Self {
            timestamps: Mutex::new(VecDeque::with_capacity(max_requests as usize)),
            max_requests,
            window_duration,
            tracker: RequestTracker::new(),
        }
    }

    /// Wait until a request slot is available, then record the timestamp.
    ///
    /// If the sliding window is full, computes how long until the oldest
    /// entry expires, drops the lock, sleeps, then re-acquires and retries.
    pub async fn acquire(&self) {
        loop {
            let now = Instant::now();
            let mut ts = self.timestamps.lock().await;

            // Evict entries older than the window
            let cutoff = now - self.window_duration;
            while ts.front().is_some_and(|&t| t < cutoff) {
                ts.pop_front();
            }

            if (ts.len() as u64) < self.max_requests {
                ts.push_back(now);
                return;
            }

            // Window is full -- compute sleep duration
            let oldest = *ts.front().expect("non-empty after length check");
            let expires_at = oldest + self.window_duration;
            let wait = expires_at.duration_since(now);

            // Drop lock before sleeping
            drop(ts);
            sleep(wait).await;
        }
    }

    /// Non-blocking snapshot of remaining request budget in the current window.
    ///
    /// Returns `None` if the lock is contended (callers should treat as "unknown").
    pub fn remaining_budget(&self) -> Option<u64> {
        match self.timestamps.try_lock() {
            Ok(ts) => {
                let now = Instant::now();
                let cutoff = now - self.window_duration;
                let active = ts.iter().filter(|&&t| t >= cutoff).count() as u64;
                Some(self.max_requests.saturating_sub(active))
            }
            Err(_) => None,
        }
    }

    /// Access the request tracker for recording outcomes.
    pub fn tracker(&self) -> &RequestTracker {
        &self.tracker
    }

    /// The configured max requests per window.
    pub fn max_requests(&self) -> u64 {
        self.max_requests
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_REQUESTS, DEFAULT_WINDOW)
    }
}

/// Atomic counters tracking API request outcomes.
pub struct RequestTracker {
    pub(crate) requests_made: AtomicU64,
    pub(crate) requests_succeeded: AtomicU64,
    pub(crate) requests_rate_limited: AtomicU64,
    pub(crate) requests_failed: AtomicU64,
    /// Cumulative backoff time in milliseconds.
    pub(crate) total_backoff_ms: AtomicU64,
}

impl RequestTracker {
    fn new() -> Self {
        Self {
            requests_made: AtomicU64::new(0),
            requests_succeeded: AtomicU64::new(0),
            requests_rate_limited: AtomicU64::new(0),
            requests_failed: AtomicU64::new(0),
            total_backoff_ms: AtomicU64::new(0),
        }
    }

    pub fn record_success(&self) {
        self.requests_made.fetch_add(1, Ordering::Relaxed);
        self.requests_succeeded.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_rate_limited(&self) {
        self.requests_made.fetch_add(1, Ordering::Relaxed);
        self.requests_rate_limited.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_failure(&self) {
        self.requests_made.fetch_add(1, Ordering::Relaxed);
        self.requests_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_backoff(&self, duration: Duration) {
        self.total_backoff_ms
            .fetch_add(duration.as_millis() as u64, Ordering::Relaxed);
    }

    /// Snapshot the current counters.
    pub fn summary(&self) -> TrackerSummary {
        TrackerSummary {
            requests_made: self.requests_made.load(Ordering::Relaxed),
            requests_succeeded: self.requests_succeeded.load(Ordering::Relaxed),
            requests_rate_limited: self.requests_rate_limited.load(Ordering::Relaxed),
            requests_failed: self.requests_failed.load(Ordering::Relaxed),
            total_backoff_secs: self.total_backoff_ms.load(Ordering::Relaxed) as f64 / 1000.0,
        }
    }
}

/// Immutable snapshot of tracker counters for display.
#[derive(Debug, Clone)]
pub struct TrackerSummary {
    pub requests_made: u64,
    pub requests_succeeded: u64,
    pub requests_rate_limited: u64,
    pub requests_failed: u64,
    pub total_backoff_secs: f64,
}

/// Execute an async operation with rate limiting and exponential backoff on 429s.
///
/// - Calls `rate_limiter.acquire()` before each attempt.
/// - On `OpenFecError::RateLimited`: waits `base_backoff * 2^(attempt-1)` plus
///   0-10s jitter, then retries up to `max_retries` times.
/// - On `OpenFecError::InvalidApiKey` or other errors: returns immediately.
/// - Records all outcomes on the tracker.
pub async fn with_retry<F, Fut, T>(
    rate_limiter: &RateLimiter,
    max_retries: u32,
    base_backoff: Duration,
    operation: F,
) -> Result<T, OpenFecError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, OpenFecError>>,
{
    let tracker = rate_limiter.tracker();

    for attempt in 0..=max_retries {
        rate_limiter.acquire().await;

        match operation().await {
            Ok(val) => {
                tracker.record_success();
                return Ok(val);
            }
            Err(OpenFecError::RateLimited) => {
                tracker.record_rate_limited();

                if attempt == max_retries {
                    return Err(OpenFecError::RateLimited);
                }

                // Exponential backoff: base * 2^attempt, plus 0-10s jitter
                let backoff_multiplier = 1u64 << attempt;
                let backoff = base_backoff * backoff_multiplier as u32;
                let jitter = Duration::from_millis(rand::thread_rng().gen_range(0..10_000));
                let total_wait = backoff + jitter;

                tracker.record_backoff(total_wait);
                sleep(total_wait).await;
            }
            Err(e) => {
                tracker.record_failure();
                return Err(e);
            }
        }
    }

    // Unreachable, but satisfies the compiler
    Err(OpenFecError::RateLimited)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn acquire_under_limit() {
        let limiter = RateLimiter::new(10, Duration::from_secs(60));

        // Should all return immediately when under budget
        for _ in 0..10 {
            limiter.acquire().await;
        }

        assert_eq!(limiter.remaining_budget(), Some(0));
    }

    #[tokio::test]
    async fn acquire_at_limit_blocks() {
        tokio::time::pause();

        let limiter = Arc::new(RateLimiter::new(3, Duration::from_secs(10)));

        // Fill the window
        for _ in 0..3 {
            limiter.acquire().await;
        }

        // Next acquire should block -- spawn it and verify it doesn't complete immediately
        let limiter_clone = Arc::clone(&limiter);
        let handle = tokio::spawn(async move {
            limiter_clone.acquire().await;
        });

        // Advance time just short of expiry -- task should still be pending
        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(!handle.is_finished());

        // Advance past the window -- task should complete
        tokio::time::advance(Duration::from_secs(2)).await;
        tokio::task::yield_now().await;
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn window_expiry() {
        tokio::time::pause();

        let limiter = RateLimiter::new(2, Duration::from_secs(5));

        limiter.acquire().await;
        limiter.acquire().await;
        assert_eq!(limiter.remaining_budget(), Some(0));

        // Advance past the window
        tokio::time::advance(Duration::from_secs(6)).await;

        // Should have full budget again
        assert_eq!(limiter.remaining_budget(), Some(2));

        // And acquire should work immediately
        limiter.acquire().await;
        assert_eq!(limiter.remaining_budget(), Some(1));
    }

    #[tokio::test]
    async fn remaining_budget_decrements() {
        let limiter = RateLimiter::new(5, Duration::from_secs(60));

        assert_eq!(limiter.remaining_budget(), Some(5));
        limiter.acquire().await;
        assert_eq!(limiter.remaining_budget(), Some(4));
        limiter.acquire().await;
        assert_eq!(limiter.remaining_budget(), Some(3));
    }

    #[tokio::test]
    async fn tracker_counters() {
        let tracker = RequestTracker::new();

        tracker.record_success();
        tracker.record_success();
        tracker.record_rate_limited();
        tracker.record_failure();
        tracker.record_backoff(Duration::from_secs(60));

        let summary = tracker.summary();
        assert_eq!(summary.requests_made, 4);
        assert_eq!(summary.requests_succeeded, 2);
        assert_eq!(summary.requests_rate_limited, 1);
        assert_eq!(summary.requests_failed, 1);
        assert!((summary.total_backoff_secs - 60.0).abs() < 0.01);
    }

    #[tokio::test]
    async fn safety_margin_default() {
        let limiter = RateLimiter::default();
        assert_eq!(limiter.max_requests(), 900);
    }

    #[tokio::test]
    async fn with_retry_succeeds_first_attempt() {
        let limiter = RateLimiter::new(100, Duration::from_secs(60));
        let result = with_retry(&limiter, 3, Duration::from_secs(1), || async {
            Ok::<_, OpenFecError>(42)
        })
        .await;
        assert_eq!(result.unwrap(), 42);

        let summary = limiter.tracker().summary();
        assert_eq!(summary.requests_made, 1);
        assert_eq!(summary.requests_succeeded, 1);
    }

    #[tokio::test]
    async fn with_retry_retries_on_rate_limit() {
        tokio::time::pause();

        let limiter = RateLimiter::new(100, Duration::from_secs(3600));
        let attempt = Arc::new(AtomicU64::new(0));
        let attempt_clone = Arc::clone(&attempt);

        let result = with_retry(
            &limiter,
            3,
            Duration::from_millis(100), // Short backoff for test speed
            move || {
                let attempt = Arc::clone(&attempt_clone);
                async move {
                    let n = attempt.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        Err(OpenFecError::RateLimited)
                    } else {
                        Ok(99)
                    }
                }
            },
        )
        .await;

        assert_eq!(result.unwrap(), 99);

        let summary = limiter.tracker().summary();
        assert_eq!(summary.requests_succeeded, 1);
        assert_eq!(summary.requests_rate_limited, 2);
    }

    #[tokio::test]
    async fn with_retry_propagates_non_rate_limit_errors() {
        let limiter = RateLimiter::new(100, Duration::from_secs(60));
        let result = with_retry(&limiter, 3, Duration::from_secs(1), || async {
            Err::<i32, _>(OpenFecError::InvalidApiKey)
        })
        .await;

        assert!(matches!(result, Err(OpenFecError::InvalidApiKey)));

        let summary = limiter.tracker().summary();
        assert_eq!(summary.requests_made, 1);
        assert_eq!(summary.requests_failed, 1);
    }

    #[tokio::test]
    async fn with_retry_exhausts_retries() {
        tokio::time::pause();

        let limiter = RateLimiter::new(100, Duration::from_secs(3600));
        let result = with_retry(
            &limiter,
            2,
            Duration::from_millis(100),
            || async { Err::<i32, _>(OpenFecError::RateLimited) },
        )
        .await;

        assert!(matches!(result, Err(OpenFecError::RateLimited)));

        let summary = limiter.tracker().summary();
        // 1 initial + 2 retries = 3 rate_limited
        assert_eq!(summary.requests_rate_limited, 3);
    }
}
