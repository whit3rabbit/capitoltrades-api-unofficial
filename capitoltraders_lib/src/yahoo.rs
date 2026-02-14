//! Yahoo Finance client wrapper for fetching stock prices.
//!
//! Provides YahooClient with methods to fetch historical and current prices,
//! with caching, weekend/holiday fallback, and graceful invalid ticker handling.

use chrono::{Datelike, NaiveDate};
use dashmap::DashMap;
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;

/// Errors from Yahoo Finance operations.
#[derive(Error, Debug)]
pub enum YahooError {
    #[error("Rate limited by Yahoo Finance (HTTP 429)")]
    RateLimited,
    #[error("Invalid date: {0}")]
    InvalidDate(String),
    #[error("Failed to parse response: {0}")]
    ParseFailed(String),
    #[error(transparent)]
    Upstream(#[from] yahoo_finance_api::YahooError),
}

/// Convert chrono::NaiveDate to time::OffsetDateTime at UTC midnight.
pub fn date_to_offset_datetime(date: NaiveDate) -> Result<OffsetDateTime, YahooError> {
    // Convert to NaiveDateTime at midnight
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| YahooError::InvalidDate(date.to_string()))?;

    // Convert to UTC timestamp
    let timestamp = datetime.and_utc().timestamp();

    // Convert to time::OffsetDateTime
    OffsetDateTime::from_unix_timestamp(timestamp)
        .map_err(|_| YahooError::InvalidDate(date.to_string()))
}

/// Convert time::OffsetDateTime to chrono::NaiveDate.
pub fn offset_datetime_to_date(dt: OffsetDateTime) -> NaiveDate {
    let timestamp = dt.unix_timestamp();
    chrono::DateTime::from_timestamp(timestamp, 0)
        .expect("valid timestamp")
        .date_naive()
}

/// Check if a Yahoo API error indicates rate limiting by inspecting Debug output.
/// YErrorMessage is not publicly exported, so we match on the formatted string.
fn is_rate_limit_api_error(err: &yahoo_finance_api::YahooError) -> bool {
    let msg = format!("{:?}", err).to_lowercase();
    msg.contains("too many requests") || msg.contains("rate limit")
}

/// Yahoo Finance client with caching and fallback logic.
pub struct YahooClient {
    connector: yahoo_finance_api::YahooConnector,
    cache: Arc<DashMap<(String, NaiveDate), Option<f64>>>,
}

impl YahooClient {
    /// Create a new YahooClient with default configuration.
    pub fn new() -> Result<Self, YahooError> {
        Ok(Self {
            connector: yahoo_finance_api::YahooConnector::new()?,
            cache: Arc::new(DashMap::new()),
        })
    }

    /// Get the number of cached entries (for testing).
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Get the adjusted close price for a ticker on a specific date.
    ///
    /// Returns Ok(None) if the ticker is invalid or no data exists for that date.
    /// Returns Ok(Some(price)) if data was found.
    /// Uses cache to avoid duplicate API calls.
    pub async fn get_price_on_date(
        &self,
        ticker: &str,
        date: NaiveDate,
    ) -> Result<Option<f64>, YahooError> {
        let key = (ticker.to_string(), date);

        // Check cache first
        if let Some(cached) = self.cache.get(&key) {
            return Ok(*cached);
        }

        // Convert date to time range (midnight to midnight+1 day)
        let start = date_to_offset_datetime(date)?;
        let end = date_to_offset_datetime(
            date.checked_add_days(chrono::Days::new(1))
                .ok_or_else(|| YahooError::InvalidDate(date.to_string()))?,
        )?;

        // Fetch from Yahoo Finance
        match self.connector.get_quote_history(ticker, start, end).await {
            Ok(response) => {
                // Extract quotes - this can also fail with NoQuotes/NoResult
                match response.quotes() {
                    Ok(quotes) => {
                        // Take first quote's adjclose (already f64)
                        let price = quotes.first().map(|q| q.adjclose);

                        // Cache the result
                        self.cache.insert(key, price);

                        Ok(price)
                    }
                    Err(yahoo_finance_api::YahooError::NoQuotes)
                    | Err(yahoo_finance_api::YahooError::NoResult) => {
                        // No data for this date
                        self.cache.insert(key, None);
                        Ok(None)
                    }
                    Err(e) => Err(YahooError::ParseFailed(format!(
                        "Failed to extract quotes: {}",
                        e
                    ))),
                }
            }
            Err(ref e) if is_rate_limit_api_error(e) => {
                // Rate limited -- do NOT cache, let caller retry later
                Err(YahooError::RateLimited)
            }
            Err(yahoo_finance_api::YahooError::NoQuotes)
            | Err(yahoo_finance_api::YahooError::NoResult)
            | Err(yahoo_finance_api::YahooError::ApiError(_)) => {
                // Invalid ticker or no data - cache None and return gracefully
                self.cache.insert(key, None);
                Ok(None)
            }
            Err(e) => Err(YahooError::Upstream(e)),
        }
    }

    /// Get price on date with weekend/holiday fallback.
    ///
    /// If the exact date returns no data and falls on a weekend, tries Friday.
    /// If still no data, fetches a 7-day window ending on the target date and returns
    /// the most recent price found.
    pub async fn get_price_on_date_with_fallback(
        &self,
        ticker: &str,
        date: NaiveDate,
    ) -> Result<Option<f64>, YahooError> {
        // First try exact date
        let result = self.get_price_on_date(ticker, date).await?;
        if result.is_some() {
            return Ok(result);
        }

        // If it's a weekend, try Friday
        use chrono::Weekday;
        let weekday = date.weekday();
        if weekday == Weekday::Sat || weekday == Weekday::Sun {
            let days_back = match weekday {
                Weekday::Sat => 1,
                Weekday::Sun => 2,
                _ => 0,
            };
            if let Some(friday) = date.checked_sub_days(chrono::Days::new(days_back)) {
                let friday_result = self.get_price_on_date(ticker, friday).await?;
                if friday_result.is_some() {
                    // Cache the original date -> Friday's price
                    self.cache
                        .insert((ticker.to_string(), date), friday_result);
                    return Ok(friday_result);
                }
            }
        }

        // Try 7-day window fallback
        let start_date = date
            .checked_sub_days(chrono::Days::new(7))
            .ok_or_else(|| YahooError::InvalidDate(date.to_string()))?;

        let start = date_to_offset_datetime(start_date)?;
        let end = date_to_offset_datetime(
            date.checked_add_days(chrono::Days::new(1))
                .ok_or_else(|| YahooError::InvalidDate(date.to_string()))?,
        )?;

        match self.connector.get_quote_history(ticker, start, end).await {
            Ok(response) => {
                // Extract quotes - this can also fail with NoQuotes/NoResult
                match response.quotes() {
                    Ok(quotes) => {
                        // Take the last (most recent) quote's adjclose (already f64)
                        let price = quotes.last().map(|q| q.adjclose);

                        // Cache the original date -> found price
                        self.cache.insert((ticker.to_string(), date), price);

                        Ok(price)
                    }
                    Err(yahoo_finance_api::YahooError::NoQuotes)
                    | Err(yahoo_finance_api::YahooError::NoResult) => {
                        // No data in this range
                        Ok(None)
                    }
                    Err(e) => Err(YahooError::ParseFailed(format!(
                        "Failed to extract quotes: {}",
                        e
                    ))),
                }
            }
            Err(ref e) if is_rate_limit_api_error(e) => {
                Err(YahooError::RateLimited)
            }
            Err(yahoo_finance_api::YahooError::NoQuotes)
            | Err(yahoo_finance_api::YahooError::NoResult)
            | Err(yahoo_finance_api::YahooError::ApiError(_)) => {
                // Still no data - ticker is genuinely invalid
                Ok(None)
            }
            Err(e) => Err(YahooError::Upstream(e)),
        }
    }

    /// Get the current price for a ticker.
    ///
    /// Uses today's date with fallback logic (handles weekends/market closure).
    pub async fn get_current_price(&self, ticker: &str) -> Result<Option<f64>, YahooError> {
        let today = chrono::Utc::now().date_naive();
        self.get_price_on_date_with_fallback(ticker, today).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Weekday};

    #[test]
    fn test_date_to_offset_datetime_basic() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let result = date_to_offset_datetime(date).unwrap();

        // Verify year, month, day match
        assert_eq!(result.year(), 2024);
        assert_eq!(result.month() as u32, 1);
        assert_eq!(result.day(), 15);

        // Verify time is midnight UTC
        assert_eq!(result.hour(), 0);
        assert_eq!(result.minute(), 0);
        assert_eq!(result.second(), 0);
        assert_eq!(result.offset().whole_hours(), 0);
    }

    #[test]
    fn test_date_to_offset_datetime_epoch() {
        let date = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let result = date_to_offset_datetime(date).unwrap();

        // Unix epoch should have timestamp 0
        assert_eq!(result.unix_timestamp(), 0);
    }

    #[test]
    fn test_date_to_offset_datetime_recent() {
        let date = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();
        let result = date_to_offset_datetime(date).unwrap();

        // Verify roundtrip
        let back = offset_datetime_to_date(result);
        assert_eq!(back, date);
    }

    #[test]
    fn test_offset_datetime_to_date_basic() {
        // Create a known OffsetDateTime (2024-06-15 midnight UTC)
        let dt = OffsetDateTime::from_unix_timestamp(1718409600).unwrap();
        let result = offset_datetime_to_date(dt);

        assert_eq!(result.year(), 2024);
        assert_eq!(result.month(), 6);
        assert_eq!(result.day(), 15);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let dates = vec![
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 6, 15).unwrap(),
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap(),
            NaiveDate::from_ymd_opt(2020, 2, 29).unwrap(), // Leap day
        ];

        for date in dates {
            let offset_dt = date_to_offset_datetime(date).unwrap();
            let back = offset_datetime_to_date(offset_dt);
            assert_eq!(back, date, "Roundtrip failed for {}", date);
        }
    }

    #[test]
    fn test_yahoo_error_display() {
        // Test Display implementation for each variant
        let rate_limited = YahooError::RateLimited;
        assert!(rate_limited.to_string().contains("Rate limited"));

        let invalid_date = YahooError::InvalidDate("2024-13-01".to_string());
        assert!(invalid_date.to_string().contains("Invalid date"));
        assert!(invalid_date.to_string().contains("2024-13-01"));

        let parse_failed = YahooError::ParseFailed("malformed JSON".to_string());
        assert!(parse_failed.to_string().contains("parse"));
        assert!(parse_failed.to_string().contains("malformed JSON"));
    }

    #[test]
    fn test_yahoo_client_creation() {
        let result = YahooClient::new();
        assert!(result.is_ok(), "YahooClient::new() should succeed");
        let client = result.unwrap();
        assert_eq!(client.cache_len(), 0, "Cache should start empty");
    }

    #[tokio::test]
    async fn test_cache_deduplication() {
        let client = YahooClient::new().unwrap();
        let ticker = "AAPL";
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        // First call - will fetch from API (or get None)
        let _ = client.get_price_on_date(ticker, date).await;
        let cache_len_after_first = client.cache_len();
        assert_eq!(cache_len_after_first, 1, "Cache should have 1 entry");

        // Second call - should use cache
        let _ = client.get_price_on_date(ticker, date).await;
        let cache_len_after_second = client.cache_len();
        assert_eq!(
            cache_len_after_second, 1,
            "Cache should still have 1 entry (cache hit)"
        );
    }

    #[tokio::test]
    async fn test_cache_stores_none() {
        let client = YahooClient::new().unwrap();
        let ticker = "INVALIDTICKER12345";
        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let result = client.get_price_on_date(ticker, date).await.unwrap();
        assert!(
            result.is_none(),
            "Invalid ticker should return None, not error"
        );

        // Verify None is cached
        assert_eq!(
            client.cache_len(),
            1,
            "Cache should store None for invalid ticker"
        );
    }

    #[test]
    fn test_weekend_detection() {
        // Test that we can detect weekends correctly
        let saturday = NaiveDate::from_ymd_opt(2024, 1, 6).unwrap(); // Jan 6, 2024 is Saturday
        let sunday = NaiveDate::from_ymd_opt(2024, 1, 7).unwrap(); // Jan 7, 2024 is Sunday
        let monday = NaiveDate::from_ymd_opt(2024, 1, 8).unwrap(); // Jan 8, 2024 is Monday

        assert_eq!(saturday.weekday(), Weekday::Sat);
        assert_eq!(sunday.weekday(), Weekday::Sun);
        assert_eq!(monday.weekday(), Weekday::Mon);
    }

    #[tokio::test]
    async fn test_get_current_price_delegates() {
        let client = YahooClient::new().unwrap();
        let _today = chrono::Utc::now().date_naive();

        // This should use get_price_on_date_with_fallback internally
        // We can't verify the exact behavior without mocking, but we can verify it doesn't panic
        let result = client.get_current_price("AAPL").await;

        // Should return Ok (either Some price or None)
        assert!(
            result.is_ok(),
            "get_current_price should return Ok for valid ticker"
        );
    }
}
