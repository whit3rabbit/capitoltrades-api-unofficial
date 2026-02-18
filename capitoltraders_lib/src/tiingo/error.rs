//! Error types for Tiingo API operations.

use thiserror::Error;

/// Errors from Tiingo API operations.
#[derive(Error, Debug)]
pub enum TiingoError {
    #[error("Rate limited by Tiingo API")]
    RateLimited,
    #[error("Invalid API key (HTTP 401)")]
    InvalidApiKey,
    #[error("Ticker not found on Tiingo")]
    NotFound,
    #[error("Failed to parse response: {0}")]
    ParseFailed(String),
    #[error("Network error")]
    Network(#[from] reqwest::Error),
}
