//! Vendored fork of the [`capitoltrades`](https://github.com/TommasoAmici/capitoltrades) API client.
//!
//! Provides an HTTP client for the CapitolTrades BFF API, along with typed request
//! builders and response models for trades, politicians, and issuers.
//!
//! # Modifications from upstream
//!
//! - `Client.base_api_url` changed from `&'static str` to `String`; added `with_base_url()`.
//! - `Meta`, `Paging` fields and `PaginatedResponse.meta` made `pub`.
//! - Added `Default` impl for `Client`.
//! - `TradeQuery`: added filter fields (parties, states, committees, search, genders,
//!   market caps, asset types, labels, sectors, tx types, chambers, politician IDs,
//!   issuer states, countries) with builder methods and URL encoding.
//! - `PoliticianQuery`: added states, committees fields with builders.
//! - `TxType`, `Chamber`, `Gender`: added `Clone`, `Copy`, `Display` derives.
//! - `MarketCap`: added `Display` impl (outputs numeric value for API).
//! - New enums: `AssetType` (22 variants), `Label` (4 variants).
//! - All clippy warnings resolved.

mod client;
mod errors;
mod query;
pub mod types;
pub mod user_agent;
pub use self::client::Client;
pub use self::errors::Error;
pub use self::query::{
    IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy, Query, SortDirection, TradeQuery,
    TradeSortBy,
};
