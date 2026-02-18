//! Library layer for Capitol Traders: cached API client, validation, and analysis helpers.
//!
//! Wraps the vendored `capitoltrades_api` crate with an in-memory TTL cache,
//! rate limiting, input validation, and trade analysis functions.

pub mod analysis;
pub mod analytics;
pub mod anomaly;
pub mod cache;
pub mod client;
pub mod committee;
pub mod committee_jurisdiction;
pub mod conflict;
pub mod db;
pub mod employer_mapping;
pub mod error;
pub mod fec_mapping;
pub mod openfec;
pub mod portfolio;
pub mod pricing;
pub mod scrape;
pub mod sector_mapping;
pub mod ticker_alias;
pub mod tiingo;
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
    AnalyticsTradeRow, ContributorAggRow, Db, DbError, DbIssuerFilter, DbIssuerRow,
    DbPoliticianFilter, DbPoliticianRow, DbTradeFilter, DbTradeRow, DonationFilter, DonationRow,
    DonationSummary, DonorContext, EmployerAggRow, EnrichmentDiagnostics, HHIPositionRow,
    IssuerStatsRow, PoliticianStatsRow, PortfolioFilter, PortfolioPosition, PreMoveCandidateRow,
    PriceEnrichmentRow, SectorTotal, StateAggRow, TradeVolumeRow,
};
pub use employer_mapping::{
    is_blacklisted, load_seed_data, match_employer, normalize_employer, EmployerMappingError,
    MatchResult, MatchType, SeedMapping,
};
pub use error::CapitolTradesError;
pub use fec_mapping::{FecMapping, FecMappingError, Legislator, download_legislators, match_legislators_to_politicians};
pub use openfec::{OpenFecClient, OpenFecError};
pub use portfolio::{calculate_positions, Lot, Position, TradeFIFO};
pub use pricing::{estimate_shares, parse_trade_range, resolve_yahoo_ticker, ShareEstimate, TradeRange};
pub use ticker_alias::{load_ticker_aliases, parse_ticker_aliases, TickerAlias, TickerAliasError};
pub use scrape::{
    ScrapeClient, ScrapeError, ScrapePage, ScrapedIssuerDetail, ScrapedIssuerList,
    ScrapedPoliticianCard, ScrapedTrade, ScrapedTradeDetail,
};
pub use sector_mapping::{
    load_sector_mappings, parse_sector_mappings, validate_sector, SectorMapping,
    SectorMappingError, GICS_SECTORS,
};
pub use tiingo::{TiingoClient, TiingoError};
pub use yahoo::{YahooClient, YahooError};
pub use analytics::{
    AnalyticsTrade, ClosedTrade, TradeMetrics, PoliticianMetrics, calculate_closed_trades,
    compute_trade_metrics, aggregate_politician_metrics, absolute_return, annualized_return,
    holding_period_days, simple_alpha,
};
pub use committee_jurisdiction::{
    CommitteeJurisdiction, load_committee_jurisdictions, get_committee_sectors,
};
pub use conflict::{
    CommitteeTradingScore, DonationTradeCorrelation, ConflictSummary,
    calculate_committee_trading_score,
};
pub use anomaly::{
    PreMoveSignal, VolumeSignal, ConcentrationScore, AnomalyScore,
    TradeWithFuturePrice, TradeVolumeRecord, PortfolioPositionForHHI,
    detect_pre_move_trades, detect_unusual_volume, calculate_sector_concentration,
    calculate_composite_anomaly_score,
};
