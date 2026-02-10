//! Yahoo Finance client wrapper for fetching stock prices.
//!
//! Provides YahooClient with methods to fetch historical and current prices,
//! with caching, weekend/holiday fallback, and graceful invalid ticker handling.

use chrono::NaiveDate;
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

/// Yahoo Finance client with caching and fallback logic.
pub struct YahooClient {
    #[allow(dead_code)]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

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
}
