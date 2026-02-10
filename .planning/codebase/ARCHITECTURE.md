# Architecture

**Analysis Date:** 2026-02-09

## Pattern Overview

**Overall:** Multi-layered CLI application with vendored API client, in-memory cache layer, validation/scraping utilities, and dual data access paths (network scrape vs. local SQLite).

**Key Characteristics:**
- Dual-mode operation: real-time scraping (BFF API + HTML scraping) or local SQLite database queries
- Three-crate workspace: `capitoltrades_api` (vendored upstream), `capitoltraders_lib` (shared library), `capitoltraders_cli` (binary)
- Input validation as first barrier before reaching API/database layers
- Cached network requests with exponential backoff retry and rate limiting
- Concurrent enrichment pipeline for trade/issuer detail scraping with circuit breaker
- Lazy expiration of TTL cache entries on access

## Layers

**API Client (capitoltrades_api):**
- Purpose: HTTP client and typed request/response models for CapitolTrades BFF API
- Location: `capitoltrades_api/src/`
- Contains: `Client` struct wrapping reqwest, query builders (TradeQuery, PoliticianQuery, IssuerQuery), typed response models (Trade, Politician, IssuerDetail), enum types (Party, TxType, Chamber, AssetType, Label, etc.)
- Depends on: reqwest, serde, chrono
- Used by: `CachedClient` wraps this; `ScrapeClient` supplements with HTML scraping

**Library Layer (capitoltraders_lib):**
- Purpose: Caching, validation, scraping, enrichment, database access, analysis helpers
- Location: `capitoltraders_lib/src/`
- Contains: `CachedClient` (rate-limited + TTL cache wrapper), `ScrapeClient` (RSC payload + HTML parsing for enrichment), `Db` (SQLite access), `validation` module (input normalization), `analysis` module (trade aggregations), `error` module (typed errors)
- Depends on: `capitoltrades_api`, dashmap (concurrent cache), rusqlite (SQLite), tokio (async), regex (HTML parsing)
- Used by: CLI commands exclusively

**CLI Layer (capitoltraders_cli):**
- Purpose: User-facing commands, output formatting, orchestration of lib layer
- Location: `capitoltraders_cli/src/`
- Contains: Four subcommands in `commands/` (trades, politicians, issuers, sync), output formatters (table/JSON/CSV/Markdown/XML), argument parsing via clap
- Depends on: `capitoltraders_lib`, clap, tabled, serde_json, csv, quick-xml
- Used by: Entry point only (main.rs)

**Database Layer:**
- Purpose: Persistent storage for trades, politicians, issuers with enrichment tracking
- Location: `schema/sqlite.sql`
- Schema: 7 tables (trades, politicians, issuers, assets, trade_committees, trade_labels, politician_committees) with foreign keys and enrichment tracking (`enriched_at` columns)

## Data Flow

**Scrape Mode (no --db flag):**

1. User provides CLI args → `main.rs` parses into subcommand + output format
2. Create `ScrapeClient` with optional base URL override (CAPITOLTRADES_BASE_URL env var)
3. Command handler (e.g., `trades.rs::run()`) validates all input parameters using `validation::*()` functions
4. Build typed query (TradeQuery with filters) from validated inputs
5. `ScrapeClient.get_trades_paginated()` → checks cache (MemoryCache via DashMap, 5min TTL)
6. Cache miss → `CachedClient.get_trades()` (with rate limiting: 5-10s delay between requests, exponential backoff retry)
7. HTTP response → deserialize to typed Trade/Politician/Issuer objects
8. Store in cache for 5 minutes
9. Apply client-side filtering (date ranges, search text) if needed
10. Format results via `output.rs` print functions (table, JSON, CSV, Markdown, XML)
11. Write to stdout

**Database Mode (--db flag):**

1. Same validation and arg parsing
2. `Db::open()` → initialize schema (migrate_v1 adds enriched_at columns)
3. Build `DbTradeFilter` or similar from validated inputs
4. `Db::query_trades()` → prepare SQL with dynamic WHERE clauses + params
5. Execute and deserialize rows to `DbTradeRow` structs
6. Apply output formatting
7. Write to stdout

**Sync Mode (sync subcommand):**

1. Validate inputs (page_size, concurrency bounds)
2. Create/open SQLite database
3. Fetch paginated trade list from API (page 1 with `page_size=100`)
4. Insert trades + politicians + issuers into tables (upsert logic)
5. For each subsequent page: fetch and insert
6. If `--enrich` flag: identify trades/issuers missing enrichment data
7. Spawn concurrent fetch tasks (Semaphore-bounded JoinSet) for detail pages
8. Use mpsc channel to serialize SQLite writes (one writer thread)
9. Circuit breaker stops on consecutive failures (default 5)
10. Track progress with indicatif progress bar

**Enrichment Pipeline:**

1. `count_unenriched_trades()` identifies trades where `enriched_at IS NULL`
2. For each trade ID: spawn async task with permit from Semaphore (concurrency limit)
3. Task fetches trade detail page via `ScrapeClient.trade_detail()`
4. Extract filing_url, asset_type, and other metadata from HTML
5. Send (id, result) tuple through mpsc channel
6. Main thread receives and calls `Db::update_trade_detail()` with sentinel CASE protection (don't overwrite non-null fields)
7. On HTTP failure: record in circuit breaker, stop if threshold exceeded

**State Management:**

- **Scrape mode:** Stateless except for in-flight cache (MemoryCache cleared on app exit)
- **Database mode:** Stateful persistence in SQLite; enrichment state tracked via `enriched_at` timestamp
- **Rate limiting:** Last request timestamp stored in `Mutex<Option<Instant>>` within CachedClient
- **Concurrent writes:** Single writer via mpsc receiver prevents SQLite lock contention during enrichment

## Key Abstractions

**Query Builders (TradeQuery, PoliticianQuery, IssuerQuery):**
- Purpose: Fluent API for constructing API requests with multiple filters
- Examples: `capitoltrades_api/src/query/trade.rs`, `capitoltrades_api/src/query/politician.rs`
- Pattern: Builder methods return `Self`, chain-able; `add_to_url()` encodes all fields into query params
- Multi-value filters stored as Vec, comma-separated in URL param (e.g., `?party=d&party=r`)

**Cached Client:**
- Purpose: Transparent caching + rate limiting wrapper around API client
- Examples: `capitoltraders_lib/src/client.rs` → `CachedClient::get_trades()`, `get_politicians()`, `get_issuers()`
- Pattern: Check cache key (derived from entire query), return cached JSON string if present + not expired; on miss, rate limit, fetch, cache result as JSON string, deserialize

**Scrape Client (RSC + HTML):**
- Purpose: Fetch and parse trade/politician/issuer detail pages (not available via official API)
- Examples: `capitoltraders_lib/src/scrape.rs` → `ScrapeClient::trade_detail()`, `issuer_detail()`, `politician_page()`
- Pattern: Next.js RSC payload extraction from HTML script tags, JSON parsing, regex-based field extraction for fallback
- Retry with exponential backoff on HTTP errors; respects Retry-After headers

**Database (Db):**
- Purpose: SQLite CRUD and aggregation queries with enrichment tracking
- Examples: `capitoltraders_lib/src/db.rs` → `Db::query_trades()`, `replace_trade()`, `mark_enriched()`
- Pattern: Prepared statements, dynamic WHERE clause building via vectors, sentinel CASE in upserts (don't overwrite enriched fields)
- Foreign keys enabled; WAL mode for concurrent reads; unchecked_transaction for bulk inserts

**Validation Module:**
- Purpose: Input normalization and validation before API/DB calls
- Examples: `capitoltraders_lib/src/validation.rs` → `validate_state()`, `validate_party()`, `validate_committee()`, etc.
- Pattern: Each validator returns typed enum or error; shortcuts like "d"→"democrat", "ca"→"CA", committee code resolution
- Committed to memory: COMMITTEE_MAP (48 entries), VALID_STATES (56 entries)

**Output Formatters:**
- Purpose: Convert API/DB rows to table, JSON, CSV, Markdown, or XML
- Examples: `capitoltraders_cli/src/output.rs`, `capitoltraders_cli/src/xml_output.rs`
- Pattern: Intermediate row structs (TradeRow, PoliticianRow, IssuerRow) implement Tabled + Serialize; format-specific print functions
- XML special handling: custom Writer for streaming (no generic serialization), singularization for array elements

## Entry Points

**Main Binary (capitoltraders):**
- Location: `capitoltraders_cli/src/main.rs::main()`
- Triggers: Invoked from CLI with args and subcommand
- Responsibilities: Parse CLI args (clap), instantiate ScrapeClient (with optional base_url override), dispatch to subcommand, handle output format selection, initialize tracing (info level)

**Subcommands:**
- `trades`: `capitoltraders_cli/src/commands/trades.rs::run()` and `run_db()` — fetch trades with 24+ filter options
- `politicians`: `capitoltraders_cli/src/commands/politicians.rs::run()` and `run_db()` — fetch politicians with filtering/sorting
- `issuers`: `capitoltraders_cli/src/commands/issuers.rs::run()` and `run_db()` — fetch issuers with filtering
- `sync`: `capitoltraders_cli/src/commands/sync.rs::run()` — ingest into SQLite with optional enrichment

## Error Handling

**Strategy:** Typed errors propagated via `Result<T>` with thiserror for library, anyhow for application.

**Patterns:**

- **Input Validation:** `CapitolTradesError::InvalidInput(String)` returned from validation functions; commands convert to `anyhow::bail!()`
- **API Errors:** `capitoltrades_api::Error` (network, deserialization, HTTP status) wrapped in `CapitolTradesError::Api()`
- **Scrape Errors:** `ScrapeError` (HTTP, missing payload, parse failure) → converted to `CapitolTradesError` or surfaced as `anyhow::Error`
- **Database Errors:** `DbError` (SQLite, JSON, date parse) → `anyhow::bail!()` in command handlers
- **Retry Logic:** Exponential backoff on transient errors (429, 500, 502, 503); max 3 retries with 2-30s delay
- **Circuit Breaker:** Consecutive failure counter in `sync` enrichment; stops after threshold (default 5) consecutive HTTP failures

## Cross-Cutting Concerns

**Logging:** tracing crate initialized in main.rs with env_filter; default level is info; module-specific "capitoltraders=info"

**Validation:** Every user input (filter value, date, page number) validated by `validation::*()` before reaching API/DB layer; normalizes casing, resolves shortcuts, bounds checks

**Authentication:** None required; no API key or OAuth (scrapes public website)

**Rate Limiting:** 5-10s random delay between consecutive HTTP requests in CachedClient to avoid overwhelming upstream; configurable via CAPITOLTRADES_RETRY_* env vars

**Caching:** 5-minute TTL in-memory cache for API responses (MemoryCache backed by DashMap); lazy eviction on access; no persistent cache

**Enrichment:** Asynchronous detail page scraping with Semaphore-bounded concurrency (default 3), mpsc channel for serializing SQLite writes, circuit breaker on consecutive failures

---

*Architecture analysis: 2026-02-09*
