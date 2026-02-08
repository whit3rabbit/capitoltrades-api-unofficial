//! Library layer for Capitol Traders: cached API client, validation, and analysis helpers.
//!
//! Wraps the vendored `capitoltrades_api` crate with an in-memory TTL cache,
//! rate limiting, input validation, and trade analysis functions.

pub mod analysis;
pub mod cache;
pub mod client;
pub mod db;
pub mod error;
pub mod scrape;
pub mod validation;

pub use capitoltrades_api;
pub use capitoltrades_api::types;
pub use capitoltrades_api::{
    IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy, Query, SortDirection, TradeQuery,
    TradeSortBy,
};

pub use client::CachedClient;
pub use db::{Db, DbError, IssuerStatsRow, PoliticianStatsRow};
pub use error::CapitolTradesError;
pub use scrape::{
    ScrapeClient, ScrapeError, ScrapePage, ScrapedIssuerDetail, ScrapedIssuerList,
    ScrapedPoliticianCard, ScrapedTrade,
};
