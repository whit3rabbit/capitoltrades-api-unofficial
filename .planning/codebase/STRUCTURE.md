# Codebase Structure

**Analysis Date:** 2026-02-15

## Directory Layout

```
capitoltraders/
├── Cargo.toml                 # Workspace manifest
├── schema/
│   └── sqlite.sql            # Database schema DDL (v7, 14 tables)
├── seed_data/
│   ├── gics_sector_mapping.yml   # 200 ticker-to-GICS-sector mappings
│   ├── committee_sectors.yml     # 40+ committee-to-GICS-sector jurisdiction mappings
│   └── employer_issuers.toml     # Curated employer-to-issuer mappings
├── .planning/
│   └── codebase/             # Architecture and analysis docs
├── capitoltrades_api/        # CRATE 1: Vendored upstream API client
├── capitoltraders_lib/       # CRATE 2: Shared library
│   ├── src/
│   │   ├── openfec/          # OpenFEC API client and types
│   │   ├── portfolio/        # FIFO accounting and valuation logic
│   │   ├── yahoo/            # Yahoo Finance market data integration
│   │   ├── employer_mapping/ # FEC employer correlation logic
│   │   ├── client.rs         # Cached CapitolTrades client
│   │   ├── db.rs             # SQLite access and migrations (v1-v7)
│   │   ├── validation.rs     # Input normalization
│   │   ├── analytics.rs      # FIFO trade matching, politician metrics, alpha calculation
│   │   ├── anomaly.rs        # Pre-move signals, volume spikes, HHI concentration scoring
│   │   ├── conflict.rs       # Committee trading scores, donation-trade correlation
│   │   ├── committee_jurisdiction.rs  # Committee-to-GICS-sector jurisdiction mapping
│   │   └── sector_mapping.rs # GICS sector classification and validation
├── capitoltraders_cli/       # CRATE 3: CLI binary
│   ├── src/
│   │   ├── main.rs           # Entry point and command dispatch
│   │   ├── commands/         # Subcommand implementations (13 total)
│   │   │   ├── trades.rs     # Recent trades (scrape/DB)
│   │   │   ├── sync.rs       # SQLite ingestion
│   │   │   ├── sync_fec.rs   # FEC candidate ID mapping
│   │   │   ├── sync_donations.rs # OpenFEC contribution ingestion
│   │   │   ├── donations.rs  # Query synced donations
│   │   │   ├── enrich_prices.rs # Yahoo Finance enrichment
│   │   │   ├── portfolio.rs  # P&L and positions
│   │   │   ├── map_employers.rs # Employer correlation tool
│   │   │   ├── analytics.rs    # Politician performance leaderboard
│   │   │   ├── conflicts.rs    # Committee trading scores and donation correlation
│   │   │   └── anomalies.rs    # Unusual trading pattern detection
│   │   └── output.rs         # Formatters (table, JSON, CSV, MD, XML)
```

## Directory Purposes

**capitoltraders_lib/src/openfec/:**
- Purpose: Typed HTTP client for OpenFEC API
- Contains: `client.rs` (OpenFecClient), `types.rs` (Schedule A, candidate, committee models)

**capitoltraders_lib/src/portfolio/:**
- Purpose: Materialized view and FIFO calculation logic
- Contains: `fifo.rs` (matching engine), `valuation.rs` (P&L calculations)

**capitoltraders_lib/src/yahoo/:**
- Purpose: Yahoo Finance integration for price data
- Contains: `client.rs` (YahooClient wrapper with DashMap caching)

**capitoltraders_lib/src/employer_mapping/:**
- Purpose: Correlating FEC donation employers with stock issuers
- Contains: `normalization.rs`, `fuzzy.rs` (strsim integration), `seed.rs` (curated mappings)

## Key File Locations

**Entry Points:**
- `capitoltraders_cli/src/main.rs` — CLI entry point; handles global flags and subcommand routing
- `capitoltraders_lib/src/lib.rs` — Library exports including `Db`, `CachedClient`, `OpenFecClient`, and `YahooClient`

**Schema & Migrations:**
- `schema/sqlite.sql` — Base DDL for fresh databases
- `capitoltraders_lib/src/db.rs` — Migration logic (v1-v7) for existing databases

---

*Structure analysis: 2026-02-15*
