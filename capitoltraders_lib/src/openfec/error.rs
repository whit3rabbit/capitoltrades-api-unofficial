//! Error types for OpenFEC API operations.

use thiserror::Error;

/// Errors from OpenFEC API operations.
#[derive(Error, Debug)]
pub enum OpenFecError {
    #[error("Rate limited by OpenFEC API (HTTP 429)")]
    RateLimited,
    #[error("Invalid API key (HTTP 403)")]
    InvalidApiKey,
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Failed to parse response: {0}")]
    ParseFailed(String),
    #[error("Network error")]
    Network(#[from] reqwest::Error),
}
