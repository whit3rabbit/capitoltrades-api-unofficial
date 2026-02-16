# Capitol Traders

## What This Is

A Rust CLI tool that tracks US congressional stock trades from CapitolTrades.com. It scrapes trade data, stores it in SQLite, enriches trades with Yahoo Finance market prices and benchmark comparisons, and provides filtered querying with multiple output formats. Includes per-politician portfolio tracking with FIFO cost basis, unrealized/realized P&L, performance analytics with leaderboards, committee-sector conflict detection, donation-trade correlation analysis, and anomaly detection for unusual trading patterns. Integrated with OpenFEC for tracking political contributions.

## Core Value

Users can see what politicians are trading, what those positions are currently worth, whether they are making or losing money, who is funding their campaigns, and whether their trading patterns show signs of unusual activity or conflicts of interest.

## Requirements

### Validated

- [x] Scrape congressional trades from CapitolTrades BFF API -- v1.0
- [x] Store trades, politicians, issuers in SQLite with sync command -- v1.0
- [x] Enrich trades with detail page scraping (filing URL, asset type) -- v1.0
- [x] Enrich issuers with detail page scraping (sector, market cap) -- v1.0
- [x] Enrich politicians with committee data -- v1.0
- [x] Query trades with 24+ filters in scrape and DB modes -- v1.0
- [x] Query politicians with party/state/committee filters -- v1.0
- [x] Query issuers with search/sector/state filters -- v1.0
- [x] Output in table, JSON, CSV, Markdown, XML formats -- v1.0
- [x] Input validation for all filter parameters -- v1.0
- [x] Rate limiting and circuit breaker for upstream requests -- v1.0
- [x] Schema migration system (PRAGMA user_version) -- v1.0
- [x] Yahoo Finance price enrichment for trade tickers -- v1.1
- [x] Per-politician net position tracking via FIFO accounting -- v1.1
- [x] Trade-date historical price lookup with weekend/holiday fallback -- v1.1
- [x] Current price lookup (refreshed on enrichment run) -- v1.1
- [x] Unrealized and realized P&L calculation per position -- v1.1
- [x] `enrich-prices` subcommand for post-sync market data enrichment -- v1.1
- [x] Schema migration v2 (price columns + positions table) -- v1.1
- [x] Option trade classification (excluded from FIFO, noted in output) -- v1.1
- [x] Portfolio query output showing current holdings with values -- v1.1
- [x] Dollar range parsing and share estimation from trade value ranges -- v1.1
- [x] OpenFEC API client with .env API key management -- v1.2
- [x] FEC candidate ID mapping (CapitolTrades politician to FEC candidate) -- v1.2
- [x] Schedule A contribution data ingestion (all available cycles) -- v1.2
- [x] Donation storage in SQLite (new tables, schema v3/v4 migrations) -- v1.2
- [x] Donation analysis: top donors, sector breakdown, employer-to-issuer correlation -- v1.2
- [x] `donations` subcommand with filtering and all 5 output formats -- v1.2
- [x] Donation summary integrated into portfolio/trades output -- v1.2
- [x] Employer mapping CLI (`map-employers`) for manual correlation -- v1.2
- [x] Schema migration v5 (employer mappings + lookup tables) -- v1.2
- [x] GICS sector classification with 200-ticker YAML mapping -- v1.3
- [x] Benchmark price enrichment (S&P 500 + 11 sector ETFs) -- v1.3
- [x] Trade performance scoring (absolute return, annualized return, alpha) -- v1.3
- [x] Politician leaderboards with time period and trade count filters -- v1.3
- [x] Committee-sector conflict detection with jurisdiction mapping -- v1.3
- [x] Donation-trade correlation via employer matching -- v1.3
- [x] Anomaly detection (pre-move trades, unusual volume, sector concentration) -- v1.3
- [x] Analytics-enriched output in existing trades/portfolio/politicians commands -- v1.3

### Active

(No active milestone -- next milestone not yet started)

### Out of Scope

- Option valuation (strike price, expiry, Greeks)
- Real-time price streaming
- Brokerage integration
- Price alerts or notifications
- Historical price charts
- Real-time alerting (30-day disclosure delay)
- Machine learning models (over-engineering for CLI)
- Causal inference claims (correlation only from public data)
- Web dashboard (CLI-first)

## Context

- Shipped v1.3 with 618 tests across workspace (all passing)
- Tech stack: Rust, SQLite, reqwest, tokio, clap, yahoo_finance_api, OpenFEC API
- 13 subcommands: trades, politicians, issuers, sync, sync-fec, enrich-prices, portfolio, sync-donations, donations, map-employers, analytics, conflicts, anomalies
- SQLite schema at v7 with 15+ tables
- Enrichment pipeline: 3-phase (historical prices, current prices, benchmark prices)
- Performance analytics: FIFO closed trade matching, multi-benchmark alpha, politician aggregation
- Conflict detection: committee jurisdiction YAML, committee trading scores, donation-trade correlation
- Anomaly detection: pre-move flags, volume spikes, HHI concentration, composite scoring
- All analytics output uses best-effort enrichment with graceful fallback

## Constraints

- **Database**: SQLite only, schema changes via versioned migrations
- **API dependency (FEC)**: OpenFEC API requires API key; rate limited to 1,000 calls/hour
- **Enrichment pattern**: Must follow existing sentinel CASE upsert pattern
- **Analytics**: All computation is in-memory at CLI runtime (no pre-computed materialized views)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Keyset Pagination for OpenFEC | OpenFEC Schedule A does not support page-based offset | Resumable sync cursor support |
| Employer Normalization | FEC employer data is messy; normalization improves match rate | Higher correlation accuracy |
| Multi-tier Committee Cache | OpenFEC candidate-to-committee mapping is expensive | Reduced API budget consumption |
| Jaro-Winkler Fuzzy Match | Handles corporate naming variations better than Levenshtein | Accurate donor-to-issuer correlation |
| Separate `sync-donations` | Donation data is large and requires API key | Targeted data ingestion |
| SPDR Sector ETFs for GICS Benchmarks | 11 sector SPDRs + SPY; high liquidity, direct GICS mapping | Reliable benchmark comparison |
| Compile-time YAML Inclusion | include_str! for sector and committee mappings | Build-time validation, no runtime I/O |
| Pure Function Design for Analytics | analytics.rs, conflict.rs, anomaly.rs have no DB coupling | Easy testing, deterministic output |
| Best-effort Enrichment | Analytics/conflict data integration doesn't fail commands | Graceful degradation when data unavailable |
| Enriched Types over Base Modification | EnrichedDbTradeRow etc. extend base types | Backward compatibility preserved |
| Percentile Rank Recomputation | Recompute after filtering, not global | Accurate relative positioning |
| FIFO gics_sector Propagation | Sector from buy lot, not sell transaction | Sector classification at purchase time |

---
*Last updated: 2026-02-15 after v1.3 milestone*
