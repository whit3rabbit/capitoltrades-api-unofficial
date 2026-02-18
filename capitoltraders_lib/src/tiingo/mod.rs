//! Tiingo API client module for fetching historical end-of-day prices.
//!
//! Used as a fallback price source when Yahoo Finance cannot resolve
//! delisted or acquired tickers. Tiingo retains historical data for
//! tickers that have been removed from exchanges.

pub mod client;
pub mod error;
pub mod types;

pub use client::TiingoClient;
pub use error::TiingoError;
