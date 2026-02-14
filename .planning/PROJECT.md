# Capitol Traders

## What This Is

A Rust CLI tool that tracks US congressional stock trades from CapitolTrades.com. It scrapes trade data, stores it in SQLite, enriches trades with Yahoo Finance market prices, and provides filtered querying with multiple output formats. Includes per-politician portfolio tracking with FIFO cost basis and unrealized/realized P&L. Now integrated with OpenFEC for tracking political contributions and donor-to-issuer correlation.

## Core Value

Users can see what politicians are trading, what those positions are currently worth, whether they are making or losing money, and who is funding their campaigns.

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

### Active

(None -- define next milestone with `/gsd:new-milestone`)

### Out of Scope

- Option valuation (strike price, expiry, Greeks)
- Real-time price streaming
- Brokerage integration
- Price alerts or notifications
- Historical price charts

## Context

- Shipped v1.2 with 503 tests across workspace (all passing)
- Tech stack: Rust, SQLite, reqwest, tokio, clap, yahoo_finance_api, OpenFEC API
- 10 subcommands: trades, politicians, issuers, sync, sync-fec, enrich-prices, portfolio, sync-donations, donations, map-employers
- SQLite schema at v5 with 13 tables
- Enrichment pipeline uses Semaphore + JoinSet + mpsc pattern for concurrent fetching
- Employer mapping uses Jaro-Winkler fuzzy matching (strsim)

## Constraints

- **Database**: SQLite only, schema changes via versioned migrations
- **API dependency (FEC)**: OpenFEC API requires API key; rate limited to 1,000 calls/hour
- **Enrichment pattern**: Must follow existing sentinel CASE upsert pattern

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Keyset Pagination for OpenFEC | OpenFEC Schedule A does not support page-based offset; requires last_index + date | Resumable sync cursor support |
| Employer Normalization | FEC employer data is messy; normalization (uppercase, suffix removal) improves match rate | Higher correlation accuracy |
| Multi-tier Committee Cache | OpenFEC candidate-to-committee mapping is expensive; cache avoids API calls | Reduced API budget consumption |
| Jaro-Winkler Fuzzy Match | Handles common corporate naming variations better than Levenshtein | Accurate donor-to-issuer correlation |
| Separate `sync-donations` | Donation data is large and requires API key; keep separate from core sync | Targeted data ingestion |

---
*Last updated: 2026-02-14 after v1.2 milestone shipped*
