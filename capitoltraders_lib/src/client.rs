//! Caching and rate-limiting wrapper around the API client.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use capitoltrades_api::types::{
    IssuerDetail, PaginatedResponse, PoliticianDetail, Response, Trade,
};
use capitoltrades_api::{Client, IssuerQuery, PoliticianQuery, TradeQuery};
use rand::Rng;

use crate::cache::MemoryCache;
use crate::error::CapitolTradesError;

/// API client wrapper that adds in-memory caching and rate limiting.
///
/// Cache hits bypass the network entirely. On cache misses, a randomized
/// 5-10 second delay is enforced between consecutive HTTP requests to
/// avoid overwhelming the API. The first request has no delay.
pub struct CachedClient {
    inner: Client,
    cache: MemoryCache,
    /// Tracks when the last HTTP request was sent, for rate limiting.
    last_request: Mutex<Option<Instant>>,
}

struct RetryConfig {
    max_retries: usize,
    base_delay_ms: u64,
    max_delay_ms: u64,
}

impl RetryConfig {
    fn from_env() -> Self {
        Self {
            max_retries: env_usize("CAPITOLTRADES_RETRY_MAX", 3),
            base_delay_ms: env_u64("CAPITOLTRADES_RETRY_BASE_MS", 2000),
            max_delay_ms: env_u64("CAPITOLTRADES_RETRY_MAX_MS", 30000),
        }
    }

    fn delay_for_attempt(&self, attempt: usize) -> Duration {
        let shift = (attempt.saturating_sub(1)).min(30) as u32;
        let exp = 1u64 << shift;
        let base = self
            .base_delay_ms
            .saturating_mul(exp)
            .min(self.max_delay_ms);
        let jitter = rand::thread_rng().gen_range(0.8..1.2);
        Duration::from_millis((base as f64 * jitter) as u64)
    }
}

impl CachedClient {
    /// Creates a new cached client using the production API URL.
    pub fn new(cache: MemoryCache) -> Self {
        Self {
            inner: Client::new(),
            cache,
            last_request: Mutex::new(None),
        }
    }

    /// Creates a new cached client with a custom base URL. Used for testing.
    pub fn with_base_url(base_url: &str, cache: MemoryCache) -> Self {
        Self {
            inner: Client::with_base_url(base_url),
            cache,
            last_request: Mutex::new(None),
        }
    }

    async fn rate_limit(&self) {
        let sleep_dur = {
            let last = self.last_request.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(last_time) = *last {
                let elapsed = last_time.elapsed();
                let delay = Duration::from_secs_f64(rand::thread_rng().gen_range(5.0..10.0));
                if elapsed < delay {
                    Some(delay - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(dur) = sleep_dur {
            tokio::time::sleep(dur).await;
        }
        *self.last_request.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    async fn with_retry<T, F, Fut>(&self, label: &str, mut f: F) -> Result<T, CapitolTradesError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, CapitolTradesError>>,
    {
        let cfg = RetryConfig::from_env();
        let mut attempt = 0usize;
        loop {
            match f().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    attempt += 1;
                    if attempt > cfg.max_retries || !is_retryable(&err) {
                        return Err(err);
                    }
                    let delay = cfg.delay_for_attempt(attempt);
                    tracing::warn!(
                        "{} request failed (attempt {}/{}), retrying in {:.1}s",
                        label,
                        attempt,
                        cfg.max_retries,
                        delay.as_secs_f64()
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Fetches trades, returning cached results when available.
    pub async fn get_trades(
        &self,
        query: &TradeQuery,
    ) -> Result<PaginatedResponse<Trade>, CapitolTradesError> {
        let cache_key = format!("trades:{:?}", query_to_cache_key(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<Trade> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self
            .with_retry("trades", || async {
                self.rate_limit().await;
                Ok(self.inner.get_trades(query).await?)
            })
            .await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    /// Fetches politicians, returning cached results when available.
    pub async fn get_politicians(
        &self,
        query: &PoliticianQuery,
    ) -> Result<PaginatedResponse<PoliticianDetail>, CapitolTradesError> {
        let cache_key = format!("politicians:{:?}", query_to_cache_key_politician(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<PoliticianDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self
            .with_retry("politicians", || async {
                self.rate_limit().await;
                Ok(self.inner.get_politicians(query).await?)
            })
            .await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    /// Fetches a single issuer by ID, returning cached results when available.
    pub async fn get_issuer(
        &self,
        issuer_id: i64,
    ) -> Result<Response<IssuerDetail>, CapitolTradesError> {
        let cache_key = format!("issuer:{}", issuer_id);

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: Response<IssuerDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self
            .with_retry("issuer", || async {
                self.rate_limit().await;
                Ok(self.inner.get_issuer(issuer_id).await?)
            })
            .await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    /// Fetches issuers, returning cached results when available.
    pub async fn get_issuers(
        &self,
        query: &IssuerQuery,
    ) -> Result<PaginatedResponse<IssuerDetail>, CapitolTradesError> {
        let cache_key = format!("issuers:{:?}", query_to_cache_key_issuer(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<IssuerDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self
            .with_retry("issuers", || async {
                self.rate_limit().await;
                Ok(self.inner.get_issuers(query).await?)
            })
            .await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    /// Removes all entries from the cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

fn is_retryable(err: &CapitolTradesError) -> bool {
    match err {
        CapitolTradesError::Api(api_err) => match api_err {
            capitoltrades_api::Error::RequestFailed => true,
            capitoltrades_api::Error::HttpStatus { status, .. } => *status == 429 || *status >= 500,
        },
        _ => false,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parties_cache_key(parties: &[capitoltrades_api::types::Party]) -> String {
    parties
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn query_to_cache_key(query: &TradeQuery) -> String {
    format!(
        "p{}:s{:?}:i{:?}:ts{:?}:pa[{}]:st{:?}:co{:?}:q{:?}:\
         pdr{:?}:tdr{:?}:sb{}:sd{}:\
         ge{:?}:mc{:?}:at{:?}:la{:?}:se{:?}:tt{:?}:ch{:?}:\
         pi{:?}:is{:?}:cn{:?}",
        query.common.page,
        query.common.page_size,
        query.issuer_ids,
        query
            .trade_sizes
            .iter()
            .map(|t| *t as u8)
            .collect::<Vec<_>>(),
        parties_cache_key(&query.parties),
        query.states,
        query.committees,
        query.search,
        query.common.pub_date_relative,
        query.common.tx_date_relative,
        query.sort_by,
        query.common.sort_direction as u8,
        query
            .genders
            .iter()
            .map(|g| g.to_string())
            .collect::<Vec<_>>(),
        query
            .market_caps
            .iter()
            .map(|m| *m as u8)
            .collect::<Vec<_>>(),
        query
            .asset_types
            .iter()
            .map(|a| a.to_string())
            .collect::<Vec<_>>(),
        query
            .labels
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>(),
        query
            .sectors
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        query
            .tx_types
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>(),
        query
            .chambers
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>(),
        query.politician_ids,
        query.issuer_states,
        query.countries,
    )
}

fn query_to_cache_key_politician(query: &PoliticianQuery) -> String {
    format!(
        "p{}:s{:?}:search{:?}:pa[{}]:st{:?}:co{:?}:is{:?}:sb{}:sd{}",
        query.common.page,
        query.common.page_size,
        query.search,
        parties_cache_key(&query.parties),
        query.states,
        query.committees,
        query.issuer_ids,
        query.sort_by,
        query.common.sort_direction as u8,
    )
}

fn query_to_cache_key_issuer(query: &IssuerQuery) -> String {
    format!(
        "p{}:s{:?}:search{:?}:st{:?}:pi{:?}:mc{:?}:se{:?}:cn{:?}:sb{}:sd{}",
        query.common.page,
        query.common.page_size,
        query.search,
        query.states,
        query.politician_ids,
        query
            .market_caps
            .iter()
            .map(|m| *m as u8)
            .collect::<Vec<_>>(),
        query
            .sectors
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        query.countries,
        query.sort_by,
        query.common.sort_direction as u8,
    )
}
