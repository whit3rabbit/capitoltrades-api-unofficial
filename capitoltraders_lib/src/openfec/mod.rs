//! OpenFEC API client module for fetching FEC candidate, committee, and donation data.

pub mod client;
pub mod error;
pub mod types;

pub use client::OpenFecClient;
pub use error::OpenFecError;
