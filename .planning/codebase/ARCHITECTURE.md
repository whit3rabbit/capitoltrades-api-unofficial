# Architecture

**Analysis Date:** 2026-02-14

## Pattern Overview

**Overall:** Multi-layered CLI application with vendored API client, in-memory cache layer, validation/scraping utilities, and dual data access paths (network scrape vs. local SQLite).

**Key Characteristics:**
- Dual-mode operation: real-time scraping (BFF API + HTML scraping) or local SQLite database queries
- Three-crate workspace: `capitoltrades_api` (vendored upstream), `capitoltraders_lib` (shared library), `capitoltraders_cli` (binary)
- Input validation as first barrier before reaching API/database layers
- Cached network requests with exponential backoff retry and rate limiting
- Concurrent enrichment pipeline for trade/issuer detail scraping with circuit breaker
- Yahoo Finance price enrichment pipeline for historical and current market data
- OpenFEC integration for tracking political contributions and donor correlation
- FIFO portfolio accounting for unrealized P&L calculation

## Layers

**API Client (capitoltrades_api):**
- Purpose: HTTP client and typed request/response models for CapitolTrades BFF API
- Location: `capitoltrades_api/src/`
- Contains: `Client` struct wrapping reqwest, query builders (TradeQuery, PoliticianQuery, IssuerQuery), typed response models, enum types
- Used by: `CachedClient` wraps this; `ScrapeClient` supplements with HTML scraping

**Library Layer (capitoltraders_lib):**
- Purpose: Caching, validation, scraping, enrichment, database access, analysis helpers, Yahoo Finance client, OpenFEC client, employer mapping
- Location: `capitoltraders_lib/src/`
- Contains: `CachedClient`, `ScrapeClient`, `Db`, `validation` module, `yahoo` module, `openfec` module, `employer_mapping` module, `portfolio` module
- Depends on: `capitoltrades_api`, dashmap, rusqlite, tokio, regex, yahoo_finance_api, strsim
- Used by: CLI commands exclusively

**CLI Layer (capitoltraders_cli):**
- Purpose: User-facing commands, output formatting, orchestration of lib layer
- Location: `capitoltraders_cli/src/`
- Contains: 10 subcommands (trades, politicians, issuers, sync, sync-fec, enrich-prices, portfolio, sync-donations, donations, map-employers), output formatters
- Depends on: `capitoltraders_lib`, clap, tabled, serde_json, csv, quick-xml, indicatif
- Used by: Entry point only (main.rs)

**Database Layer:**
- Purpose: Persistent storage with enrichment tracking and donor correlation
- Location: `schema/sqlite.sql`
- Schema (v5): 13 tables (trades, politicians, issuers, assets, trade_committees, trade_labels, politician_committees, positions, fec_mappings, fec_committees, donations, donation_sync_meta, employer_mappings, employer_lookup)

## Data Flow

**Scrape Mode (no --db flag):**
1. User provides CLI args → `main.rs` parses into subcommand + output format
2. Create `ScrapeClient` with optional base URL override
3. Command handler validates input parameters using `validation::*()`
4. Build typed query (TradeQuery) from validated inputs
5. `ScrapeClient` checks memory cache (DashMap, 5min TTL)
6. Cache miss → `CachedClient` fetches with rate limiting (5-10s jittered delay)
7. Format results via `output.rs` print functions

**Database Mode (--db flag):**
1. Same validation and arg parsing
2. `Db::open()` → initialize schema and run migrations (v1-v5)
3. Build `DbTradeFilter` or similar from validated inputs
4. `Db::query_trades()` → execute SQL with dynamic WHERE clauses
5. Apply output formatting and donor context if requested (`--show-donor-context`)

**Donation Sync Pipeline (sync-donations):**
1. Resolve politician to FEC candidate ID using `fec_mappings` (populated by `sync-fec`)
2. `CommitteeResolver` fetches authorized committees (memory -> DB -> OpenFEC cache)
3. Spawn concurrent fetch workers (Semaphore-bounded) for Schedule A contributions
4. Keyset pagination (last_index + last_date) used to fetch pages from OpenFEC
5. Atomic DB writes: save donations and update sync cursor in single transaction
6. Resumable: re-running starts from last persisted cursor

**Price Enrichment Pipeline (enrich-prices):**
1. Identify trades with tickers but no `trade_date_price`
2. Fetch historical prices from Yahoo Finance per unique (ticker, date)
3. Estimate shares using midpoint of trade value range and historical price
4. Fetch current prices per unique ticker to update portfolio valuation
5. Materialize positions in `positions` table using FIFO accounting

## Key Abstractions

**CommitteeResolver:**
- Three-tier cache (memory -> SQLite -> OpenFEC API) to minimize API budget consumption
- Classifies committees into campaign, leadership PAC, or joint fundraising

**Employer Mapping:**
- Normalizes raw FEC employer strings (uppercase, trim, remove corporate suffixes)
- Fuzzy matching (strsim Jaro-Winkler) correlates employers to stock issuers
- Curated seed data bootstrap common corporate/political mappings

**FIFO Portfolio:**
- Pure logic engine in `portfolio/fifo.rs` handles buy/sell/receive/exchange matching
- Tracks cost basis and unrealized P&L based on estimated share counts

**Circuit Breaker:**
- Bounded failure tracking in enrichment pipelines to halt on rate limits or network outages
- Thresholds: 5 for OpenFEC (aggressive), 10 for Yahoo Finance (standard)

---

*Architecture analysis: 2026-02-14*
