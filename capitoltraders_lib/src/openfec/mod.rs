//! OpenFEC API client module for fetching FEC candidate, committee, and donation data.

pub mod client;
pub mod error;
pub mod rate_limiter;
pub mod types;

pub use client::OpenFecClient;
pub use error::OpenFecError;
pub use rate_limiter::{RateLimiter, RequestTracker};
