//! Library layer for Capitol Traders: cached API client, validation, and analysis helpers.
//!
//! Wraps the vendored `capitoltrades_api` crate with an in-memory TTL cache,
//! rate limiting, input validation, and trade analysis functions.

pub mod analysis;
pub mod cache;
pub mod client;
pub mod committee;
pub mod db;
pub mod error;
pub mod fec_mapping;
pub mod openfec;
pub mod portfolio;
pub mod pricing;
pub mod scrape;
pub mod validation;
pub mod yahoo;

pub use capitoltrades_api;
pub use capitoltrades_api::types;
pub use capitoltrades_api::{
    IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy, Query, SortDirection, TradeQuery,
    TradeSortBy,
};

pub use client::CachedClient;
pub use committee::{CommitteeClass, CommitteeError, CommitteeResolver, ResolvedCommittee};
pub use db::{
    Db, DbError, DbIssuerFilter, DbIssuerRow, DbPoliticianFilter, DbPoliticianRow, DbTradeFilter,
    DbTradeRow, IssuerStatsRow, PoliticianStatsRow, PortfolioFilter, PortfolioPosition,
    PriceEnrichmentRow,
};
pub use error::CapitolTradesError;
pub use fec_mapping::{FecMapping, FecMappingError, Legislator, download_legislators, match_legislators_to_politicians};
pub use openfec::{OpenFecClient, OpenFecError};
pub use portfolio::{calculate_positions, Lot, Position, TradeFIFO};
pub use pricing::{estimate_shares, parse_trade_range, ShareEstimate, TradeRange};
pub use scrape::{
    ScrapeClient, ScrapeError, ScrapePage, ScrapedIssuerDetail, ScrapedIssuerList,
    ScrapedPoliticianCard, ScrapedTrade, ScrapedTradeDetail,
};
pub use yahoo::{YahooClient, YahooError};
