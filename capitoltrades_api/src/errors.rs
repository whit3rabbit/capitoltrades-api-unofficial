//! Error types for the API client.

/// Errors that can occur when making API requests.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// An HTTP request failed (network error, timeout, or unexpected response).
    #[error("Request failed")]
    RequestFailed,
    /// The API returned a non-success status with a body snippet.
    #[error("Request failed with status {status}")]
    HttpStatus { status: u16, body: String },
}
