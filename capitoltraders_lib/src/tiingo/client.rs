//! Tiingo API client for fetching historical end-of-day prices.
//!
//! Used as a fallback when Yahoo Finance returns no data for delisted/acquired tickers.
//! Tiingo retains historical data for tickers that have been delisted.

use super::error::TiingoError;
use super::types::TiingoDailyPrice;
use chrono::NaiveDate;
use std::time::Duration;

/// Request timeout for Tiingo API calls.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Tiingo REST API client for end-of-day historical prices.
pub struct TiingoClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl TiingoClient {
    /// Create a new TiingoClient with default base URL.
    pub fn new(api_key: String) -> Result<Self, TiingoError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            client,
            api_key,
            base_url: "https://api.tiingo.com".to_string(),
        })
    }

    /// Create a new TiingoClient with custom base URL (for testing with wiremock).
    pub fn with_base_url(base_url: &str, api_key: String) -> Result<Self, TiingoError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self {
            client,
            api_key,
            base_url: base_url.to_string(),
        })
    }

    /// Get the adjusted close price for a ticker on a specific date.
    ///
    /// Returns `Ok(Some(price))` if data is found.
    /// Returns `Ok(None)` if the ticker is unknown (404) or no data exists for that date (empty array).
    /// Returns `Err(TiingoError::RateLimited)` if Tiingo returns a rate limit response.
    ///
    /// Tiingo quirk: rate limits return HTTP 200 with Content-Type text/plain
    /// instead of a proper 429 status code.
    pub async fn get_price_on_date(
        &self,
        ticker: &str,
        date: NaiveDate,
    ) -> Result<Option<f64>, TiingoError> {
        let date_str = date.format("%Y-%m-%d").to_string();
        let url = format!(
            "{}/tiingo/daily/{}/prices",
            self.base_url, ticker
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.api_key))
            .query(&[("startDate", &date_str), ("endDate", &date_str)])
            .send()
            .await?;

        let status = response.status();

        // 404 = ticker not found on Tiingo
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        // 401 = bad API key
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(TiingoError::InvalidApiKey);
        }

        // Non-success (other than 200/404/401) is an error
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read response body".to_string());
            return Err(TiingoError::ParseFailed(format!(
                "HTTP {}: {}",
                status,
                if body.len() > 200 { &body[..200] } else { &body }
            )));
        }

        // Tiingo quirk: rate-limit responses are HTTP 200 with text/plain content type
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body = response.text().await.map_err(|e| {
            TiingoError::ParseFailed(format!("Failed to read response body: {}", e))
        })?;

        // If content type is text/plain, it is likely a rate limit message
        if content_type.contains("text/plain") || content_type.contains("text/html") {
            let lower = body.to_lowercase();
            if lower.contains("rate limit") || lower.contains("too many requests") || lower.contains("error") {
                return Err(TiingoError::RateLimited);
            }
        }

        // Parse as JSON array of daily prices
        let prices: Vec<TiingoDailyPrice> = serde_json::from_str(&body).map_err(|e| {
            let snippet = if body.len() > 500 { &body[..500] } else { &body };
            TiingoError::ParseFailed(format!(
                "Failed to deserialize response: {} | body: {}",
                e, snippet
            ))
        })?;

        // Empty array = no data for this date range
        Ok(prices.first().map(|p| p.adj_close))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn sample_price_json() -> serde_json::Value {
        serde_json::json!([{
            "adjClose": 142.56,
            "adjHigh": 144.10,
            "adjLow": 141.20,
            "adjOpen": 143.50,
            "adjVolume": 5000000,
            "close": 142.56,
            "date": "2024-06-15T00:00:00+00:00",
            "divCash": 0.0,
            "high": 144.10,
            "low": 141.20,
            "open": 143.50,
            "splitFactor": 1.0,
            "volume": 5000000
        }])
    }

    #[tokio::test]
    async fn success_returns_adj_close() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tiingo/daily/AAPL/prices"))
            .and(query_param("startDate", "2024-06-15"))
            .and(query_param("endDate", "2024-06-15"))
            .and(header("Authorization", "Token test-key"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(sample_price_json())
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let client = TiingoClient::with_base_url(&server.uri(), "test-key".to_string()).unwrap();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let result = client.get_price_on_date("AAPL", date).await.unwrap();

        assert_eq!(result, Some(142.56));
    }

    #[tokio::test]
    async fn empty_array_returns_none() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tiingo/daily/AAPL/prices"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!([]))
                    .insert_header("content-type", "application/json"),
            )
            .mount(&server)
            .await;

        let client = TiingoClient::with_base_url(&server.uri(), "test-key".to_string()).unwrap();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let result = client.get_price_on_date("AAPL", date).await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn not_found_returns_none() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tiingo/daily/FAKETICKER/prices"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let client = TiingoClient::with_base_url(&server.uri(), "test-key".to_string()).unwrap();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let result = client.get_price_on_date("FAKETICKER", date).await.unwrap();

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn rate_limit_plain_text_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tiingo/daily/AAPL/prices"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("Error: Rate limit exceeded. Please wait and try again.")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;

        let client = TiingoClient::with_base_url(&server.uri(), "test-key".to_string()).unwrap();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let result = client.get_price_on_date("AAPL", date).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TiingoError::RateLimited));
    }

    #[tokio::test]
    async fn invalid_api_key_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/tiingo/daily/AAPL/prices"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Authorization error"))
            .mount(&server)
            .await;

        let client = TiingoClient::with_base_url(&server.uri(), "bad-key".to_string()).unwrap();
        let date = NaiveDate::from_ymd_opt(2024, 6, 15).unwrap();
        let result = client.get_price_on_date("AAPL", date).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TiingoError::InvalidApiKey));
    }

    #[test]
    fn tiingo_error_display() {
        let err = TiingoError::RateLimited;
        assert!(err.to_string().contains("Rate limited"));

        let err = TiingoError::InvalidApiKey;
        assert!(err.to_string().contains("Invalid API key"));

        let err = TiingoError::NotFound;
        assert!(err.to_string().contains("not found"));

        let err = TiingoError::ParseFailed("bad json".to_string());
        assert!(err.to_string().contains("parse"));
        assert!(err.to_string().contains("bad json"));
    }

    #[test]
    fn client_creation_with_defaults() {
        let client = TiingoClient::new("test-key".to_string());
        assert!(client.is_ok());
    }

    #[test]
    fn client_creation_with_base_url() {
        let client = TiingoClient::with_base_url("http://localhost:1234", "test-key".to_string());
        assert!(client.is_ok());
    }
}
