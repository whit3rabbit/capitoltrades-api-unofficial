//! Error types for the library layer.

use std::fmt;

/// Errors produced by the library layer, wrapping upstream API errors
/// and adding cache, serialization, and input validation failures.
#[derive(Debug)]
pub enum CapitolTradesError {
    /// An error from the underlying API client.
    Api(capitoltrades_api::Error),
    /// A cache operation failed (e.g. deserialization of cached data).
    Cache(String),
    /// JSON serialization or deserialization failed.
    Serialization(serde_json::Error),
    /// User-provided input failed validation.
    InvalidInput(String),
}

impl fmt::Display for CapitolTradesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(e) => write!(f, "API error: {}", e),
            Self::Cache(msg) => write!(f, "Cache error: {}", msg),
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for CapitolTradesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Api(e) => Some(e),
            Self::Serialization(e) => Some(e),
            _ => None,
        }
    }
}

impl From<capitoltrades_api::Error> for CapitolTradesError {
    fn from(e: capitoltrades_api::Error) -> Self {
        Self::Api(e)
    }
}

impl From<serde_json::Error> for CapitolTradesError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e)
    }
}
