# External Integrations

**Analysis Date:** 2026-02-14

## APIs & External Services

**CapitolTrades API:**
- Service: CapitolTrades BFF (Backend for Frontend) API
- Base URL: `https://bff.capitoltrades.com` (production default)
- What it's used for: Fetch paginated lists of congressional trades, politicians, and issuers with extensive filtering
- SDK/Client: Custom Rust client in `capitoltrades_api/src/client.rs`
- Rate Limiting: Randomized 5-10 second delay enforced in `CachedClient`

**HTML Scraping (CapitolTrades Website):**
- Service: https://www.capitoltrades.com (web scraping fallback)
- What it's used for: Fetch detailed trade pages, politician committees, and issuer details via Next.js RSC payloads
- SDK/Client: Custom scraping client in `capitoltraders_lib/src/scrape.rs`

**Yahoo Finance API:**
- Service: Yahoo Finance Public API (via `yahoo_finance_api` crate)
- What it's used for: Fetch historical market prices for trades (on trade date) and current prices for portfolio valuation
- SDK/Client: `YahooClient` wrapper around `yahoo_finance_api::YahooConnector` in `capitoltraders_lib/src/yahoo.rs`
- Rate Limiting: 200-500ms jittered delay, max 5 concurrent requests
- Cache: DashMap-backed `PriceCache` in-memory to minimize redundant calls for the same (ticker, date)

**OpenFEC API:**
- Service: Federal Election Commission (FEC) API (api.data.gov)
- What it's used for: Fetch Schedule A individual contributions and authorized committee mappings for politicians
- SDK/Client: `OpenFecClient` in `capitoltraders_lib/src/openfec/`
- Auth: Requires `OPENFEC_API_KEY` (configured via `.env`)
- Rate Limiting: 200-500ms jittered delay, max 3 concurrent workers; follows standard OpenFEC rate limits (1000/hr)
- Pagination: Keyset-based (last_index + last_date) for Schedule A contributions

**Congress Legislators Dataset:**
- Service: https://theunitedstates.io/congress-legislators/
- What it's used for: Mapping CapitolTrades politicians to FEC candidate IDs (via Bioguide ID or name/state matching)
- Source: `legislators-current.yaml` and `legislators-historical.yaml` downloaded from GitHub

## Data Storage

**Databases:**
- Type: SQLite (embedded)
- Connection: File-based with WAL mode and foreign keys enabled
- Client: Rusqlite 0.31 (bundled)
- Schema: `schema/sqlite.sql`
- Migrations: Handled in `capitoltraders_lib/src/db.rs` using `user_version` (currently v7)

**Tables:**
- `trades`, `politicians`, `issuers` (core data)
- `positions` (materialized portfolio valuation)
- `fec_mappings`, `fec_committees`, `donations` (donor data)
- `employer_mappings`, `employer_lookup` (correlation data)
- `sector_benchmarks` (GICS sector benchmark ETF reference data)

## Caching

**Memory Cache:**
- Type: DashMap-backed TTL cache
- Scope: CapitolTrades API responses (5 min TTL), OpenFEC committee mappings, Yahoo Finance prices
- Eviction: Lazy eviction on access

## Monitoring & Observability

**Logs:**
- Structured logging via `tracing` with `tracing-subscriber` for environment-based filtering
- Progress reporting via `indicatif` for long-running sync/enrichment pipelines

---

*Integration audit: 2026-02-14*
