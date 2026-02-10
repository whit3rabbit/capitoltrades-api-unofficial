# External Integrations

**Analysis Date:** 2026-02-09

## APIs & External Services

**CapitolTrades API:**
- Service: CapitolTrades BFF (Backend for Frontend) API
- Base URL: `https://bff.capitoltrades.com` (production default)
- What it's used for: Fetch paginated lists of congressional trades, politicians, and issuers with extensive filtering
- SDK/Client: Custom Rust client in `capitoltrades_api/src/client.rs` via `capitoltrades_api::Client`
- Auth: None (public API, no credentials required)
- Rate Limiting: Randomized 5-10 second delay between consecutive requests enforced in `CachedClient` (`capitoltraders_lib/src/client.rs`)
- Retry Strategy: Exponential backoff with jitter, configurable via environment variables

**HTML Scraping (CapitolTrades Website):**
- Service: https://www.capitoltrades.com (web scraping fallback)
- What it's used for: Fetch detailed trade pages, politician committees, and issuer details via Next.js RSC payloads
- SDK/Client: Custom scraping client in `capitoltraders_lib/src/scrape.rs` via `ScrapeClient` struct
- Auth: None (public website)
- Rate Limiting: Randomized 5-10 second delay between consecutive scrape requests
- Retry Strategy: Same exponential backoff as API (3 retries, 2-30 second delays, with 0.8-1.2x jitter)
- Headers Included: Browser-like user agent rotation, origin/referer, accept, CORS fetch headers

## Data Storage

**Databases:**
- Type: SQLite (embedded)
- Connection: File-based at user-specified path (e.g., `capitoltraders.db`)
- Client: Rusqlite 0.31 (with bundled SQLite)
- Pragmas: Foreign keys ON, journal mode WAL, synchronous NORMAL
- Schema: Located in `schema/sqlite.sql`
- Tables: trades, politicians, issuers, ingest_meta
- Migrations: Versioned via user_version pragma (currently v1 adds enriched_at columns)

**File Storage:**
- Type: Local filesystem only
- Usage: SQLite database file storage at user-specified path

**Caching:**
- Type: In-memory TTL cache (DashMap-backed)
- Implementation: `MemoryCache` in `capitoltraders_lib/src/cache.rs`
- TTL: 300 seconds (5 minutes) by default
- Thread-safe: Yes (concurrent access via DashMap)
- Where used: `CachedClient` wraps all API/scrape requests to deduplicate network calls
- Eviction: Lazy eviction on key access (checked on get, removed if expired)

## Authentication & Identity

**Auth Provider:**
- Type: None required
- Both CapitolTrades APIs and scraping are public-facing, no authentication needed
- User agents rotated randomly to avoid IP blocking

## Monitoring & Observability

**Error Tracking:**
- Type: None (no external service integration)
- Local error handling via `thiserror` and `anyhow`

**Logs:**
- Type: Structured logging via Tracing
- Output: stderr via `tracing-subscriber`
- Filtering: Environment variable `RUST_LOG` (default `capitoltraders=info`)
- Format: Includes level, span, message (target disabled in output)

## CI/CD & Deployment

**Hosting:**
- Type: Standalone binary
- Deployment: No special hosting required; binary runs anywhere Rust is supported

**CI Pipeline:**
- Type: GitHub Actions (workflows in `.github/workflows/`)
- Services: Testing and build automation

## Environment Configuration

**Required env vars:**
- None (all have sensible defaults)

**Optional env vars:**
- `CAPITOLTRADES_RETRY_MAX`: Max retries (default 3)
- `CAPITOLTRADES_RETRY_BASE_MS`: Base backoff delay (default 2000)
- `CAPITOLTRADES_RETRY_MAX_MS`: Max backoff delay (default 30000)
- `CAPITOLTRADES_BASE_URL`: Override scrape/API base URL
- `RUST_LOG`: Tracing filter (e.g., `capitoltraders=debug`)

**Secrets location:**
- None used in codebase (all public APIs)
- `.env` files: Not required; all config via CLI flags or environment variables

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- None

## API Details

**CapitolTrades BFF Endpoints (via `Client` in `capitoltrades_api/src/client.rs`):**
- `GET /trades` - Paginated list of trades with query filters
- `GET /politicians` - Paginated list of politicians with query filters
- `GET /issuers/{issuer_id}` - Single issuer detail by numeric ID
- `GET /issuers` - Paginated list of issuers with query filters

**Query Parameters:**
- Trades: page, pageSize, parties, states, committees, search, pubDate, txDate, sortBy, sortDirection, genders, marketCaps, assetTypes, labels, sectors, txTypes, chambers, politicianIds, issuerStates, countries
- Politicians: page, pageSize, search, parties, states, committees, issuerIds, sortBy, sortDirection
- Issuers: page, pageSize, search, states, politicianIds, marketCaps, sectors, countries, sortBy, sortDirection

**Pagination:**
- Default page size: 12 (trades), configurable via `page_size` parameter
- Pagination: Zero-based offset via `page` parameter
- Response includes `meta` (total_count, total_pages) and `paging` info

## Data Flow

**Scrape Mode (Primary):**
1. User invokes CLI command (e.g., `trades --politician pelosi`)
2. Filters validated via `capitoltraders_lib/src/validation.rs`
3. Cache checked in `MemoryCache` (5-minute TTL)
4. If cache miss: Rate limiter waits 5-10 seconds
5. Request sent to `bff.capitoltrades.com` via `reqwest::Client`
6. Response deserialized into typed structs
7. Optional enrichment: Scrape detail pages from `www.capitoltrades.com` for additional data
8. Data formatted and output (table, JSON, CSV, markdown, XML)

**DB Mode:**
1. User invokes `--db path/to/db.sqlite` flag
2. SQLite database opened with Rusqlite
3. Filters applied via SQL WHERE clauses
4. Results fetched from local tables
5. Data formatted and output

**Sync Mode:**
1. User invokes `sync --db path/to/db.sqlite [--full] [--enrich]`
2. Database initialized if missing
3. Pagination loop fetches from API
4. Data inserted into trades, politicians, issuers tables
5. Optional enrichment: Concurrent scraping of detail pages (semaphore-controlled)
6. Enriched data upserted with sentinel CASE to preserve previous enrichment

## Rate Limiting Strategy

**Implementation:**
- Location: `CachedClient::rate_limit()` in `capitoltraders_lib/src/client.rs`
- Pattern: Tracks `last_request` timestamp, enforces minimum delay between calls
- Delay: Random 5-10 seconds (float range 5.0..10.0)
- First request: No delay (optimizes interactive use)
- Subsequent: Respects previous request time, sleeps if needed

**Retry Logic:**
- Location: `CachedClient::with_retry()` in `capitoltraders_lib/src/client.rs`
- Strategy: Exponential backoff with jitter
- Max retries: 3 (configurable via `CAPITOLTRADES_RETRY_MAX`)
- Base delay: 2000ms (configurable via `CAPITOLTRADES_RETRY_BASE_MS`)
- Max delay: 30000ms (configurable via `CAPITOLTRADES_RETRY_MAX_MS`)
- Retryable: HTTP 429 (Too Many Requests), 5xx errors, network failures
- Non-retryable: HTTP 4xx (except 429), parsing errors, cache misses

## Cache Key Structure

**Trade cache key format:**
```
trades:p{page}:s{pageSize}:i{issuerIds}:ts{tradeSizes}:pa[{parties}]:st{states}:co{committees}:q{search}:
pdr{pubDateRelative}:tdr{txDateRelative}:sb{sortBy}:sd{sortDirection}:
ge{genders}:mc{marketCaps}:at{assetTypes}:la{labels}:se{sectors}:tt{txTypes}:ch{chambers}:
pi{politicianIds}:is{issuerStates}:cn{countries}
```

**Politician cache key format:**
```
politicians:p{page}:s{pageSize}:search{search}:pa[{parties}]:st{states}:co{committees}:is{issuerIds}:sb{sortBy}:sd{sortDirection}
```

**Issuer cache key format:**
```
issuers:p{page}:s{pageSize}:search{search}:st{states}:pi{politicianIds}:mc{marketCaps}:se{sectors}:cn{countries}:sb{sortBy}:sd{sortDirection}
```

---

*Integration audit: 2026-02-09*
