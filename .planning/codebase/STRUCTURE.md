# Codebase Structure

**Analysis Date:** 2026-02-09

## Directory Layout

```
capitoltraders/
├── Cargo.toml                 # Workspace manifest (3 members)
├── Cargo.lock                 # Dependency versions
├── README.md                  # Usage documentation
├── CLAUDE.md                  # Development guide (agent conventions)
├── schema/
│   └── sqlite.sql            # Database schema DDL (7 tables)
├── .planning/
│   └── codebase/             # This analysis output directory
├── .github/
│   └── workflows/            # CI/CD pipeline configs
├── capitoltrades_api/        # CRATE 1: Vendored upstream API client
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs            # Public API (re-exports)
│   │   ├── client.rs         # HTTP client (reqwest-based)
│   │   ├── errors.rs         # Error types
│   │   ├── user_agent.rs     # Browser user agent handling
│   │   ├── types/            # Request/response models
│   │   │   ├── mod.rs
│   │   │   ├── trade.rs      # Trade, AssetType, Label (22+4 variants)
│   │   │   ├── politician.rs # PoliticianDetail, Chamber, Gender
│   │   │   ├── issuer.rs     # IssuerDetail
│   │   │   └── meta.rs       # Meta, Paging, PaginatedResponse
│   │   └── query/            # Request builders
│   │       ├── mod.rs
│   │       ├── common.rs     # CommonQuery (pagination, sort)
│   │       ├── trade.rs      # TradeQuery (24+ filter fields)
│   │       ├── politician.rs # PoliticianQuery
│   │       └── issuer.rs     # IssuerQuery
│   └── tests/
│       ├── fixtures/         # JSON fixtures for deserialization tests
│       └── ... (7 test files)
├── capitoltraders_lib/       # CRATE 2: Shared library (cache, validation, scrape, db)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs            # Public re-exports
│   │   ├── client.rs         # CachedClient (cache + rate limit wrapper)
│   │   ├── cache.rs          # MemoryCache (DashMap-backed TTL cache)
│   │   ├── scrape.rs         # ScrapeClient (RSC + HTML parsing for enrichment)
│   │   ├── db.rs             # Db (SQLite CRUD, schema init, migrations)
│   │   ├── error.rs          # CapitolTradesError enum
│   │   ├── validation.rs     # 15+ validator functions (validate_state, validate_party, etc.)
│   │   ├── validation_tests.rs # 83 validation test cases
│   │   └── analysis.rs       # Trade aggregation helpers (by party, by ticker, etc.)
│   └── tests/
│       ├── fixtures/         # HTML + JSON fixtures for scrape/integration tests
│       └── ... (20+ test files)
└── capitoltraders_cli/       # CRATE 3: CLI binary
    ├── Cargo.toml
    ├── src/
    │   ├── main.rs           # Entry point, arg parsing, dispatch
    │   ├── output.rs         # Format output (table, JSON, CSV, Markdown, XML)
    │   ├── xml_output.rs     # XML-specific formatting (quick-xml Writer)
    │   ├── output_tests.rs   # 34 output format tests
    │   ├── xml_output_tests.rs # 12 XML roundtrip + wellformedness tests
    │   └── commands/         # Subcommand implementations
    │       ├── mod.rs
    │       ├── trades.rs     # `trades` subcommand (scrape + DB modes)
    │       ├── politicians.rs # `politicians` subcommand
    │       ├── issuers.rs    # `issuers` subcommand
    │       └── sync.rs       # `sync` subcommand (SQLite ingestion + enrichment)
    └── tests/
        └── ... (integration tests)
```

## Directory Purposes

**capitoltrades_api/ (Vendored Upstream):**
- Purpose: Typed HTTP client and data models for CapitolTrades BFF API
- Contains: Client struct, query builders, response types, enum variants (Party, TxType, Chamber, AssetType, Label, MarketCap, Gender, Sector, TradeSize)
- Key files: `src/client.rs` (Client), `src/query/trade.rs` (TradeQuery builder), `src/types/trade.rs` (Trade struct + enums), `src/types/politician.rs` (PoliticianDetail)
- All capitoltrades_api code is vendored with local modifications (documented in lib.rs comments)

**capitoltraders_lib/ (Shared Library):**
- Purpose: Caching, validation, scraping, database, analysis helpers
- Contains: CachedClient (wraps capitoltrades_api::Client with MemoryCache + rate limiting), ScrapeClient (RSC payload + HTML parsing), Db (SQLite), validation (input normalization), analysis (trade aggregations)
- Key files: `src/client.rs` (CachedClient), `src/scrape.rs` (ScrapeClient), `src/db.rs` (Db), `src/validation.rs` (validators), `src/cache.rs` (MemoryCache)
- Test count: 288 tests total (cache, validation, scrape, db, analysis, enrichment pipeline)

**capitoltraders_cli/ (Binary):**
- Purpose: User-facing CLI commands, output formatting, orchestration
- Contains: Four subcommands (trades, politicians, issuers, sync), output formatters (table/JSON/CSV/Markdown/XML), clap-based arg parsing
- Key files: `src/main.rs` (entry point), `src/commands/trades.rs` (trades subcommand), `src/commands/sync.rs` (sync with enrichment), `src/output.rs` (formatters), `src/xml_output.rs` (XML writer)
- Output formats: Table (tabled crate), JSON (serde_json), CSV (csv crate), Markdown (tabled Style::markdown()), XML (quick-xml Writer)

**schema/ (Database):**
- Purpose: SQLite schema DDL and migrations
- Contains: CREATE TABLE statements for trades, politicians, issuers, assets, trade_committees, trade_labels, politician_committees
- Key file: `schema/sqlite.sql` (7 tables with foreign keys and enrichment_at tracking)

## Key File Locations

**Entry Points:**
- `capitoltraders_cli/src/main.rs` — Binary entry point; parses CLI args, initializes ScrapeClient, dispatches to subcommand
- `capitoltraders_lib/src/lib.rs` — Library re-exports (CachedClient, Db, ScrapeClient, validation, analysis)
- `capitoltrades_api/src/lib.rs` — Vendored API client re-exports (Client, query builders, types)

**Configuration:**
- `Cargo.toml` (root) — Workspace manifest with 3 members
- `capitoltraders_cli/Cargo.toml` — Defines clap, tabled, csv, quick-xml dependencies
- `capitoltraders_lib/Cargo.toml` — Defines cache (dashmap), scrape (reqwest, regex), db (rusqlite) dependencies

**Core Logic:**
- `capitoltraders_lib/src/client.rs` — CachedClient with rate limiting and retry logic
- `capitoltraders_lib/src/scrape.rs` — ScrapeClient for HTML RSC payload extraction
- `capitoltraders_lib/src/db.rs` — Db struct with query methods, migrations, enrichment logic
- `capitoltraders_lib/src/validation.rs` — Input validation (15+ functions for states, parties, committees, etc.)
- `capitoltrades_api/src/query/trade.rs` — TradeQuery builder with 24+ filter fields
- `capitoltraders_cli/src/output.rs` — Output formatting (table, JSON, CSV, Markdown, XML)

**Testing:**
- `capitoltraders_lib/src/validation_tests.rs` — 83 validation unit tests (state, party, committee, dates, bounds, etc.)
- `capitoltraders_lib/tests/fixtures/` — HTML + JSON fixtures for scrape/integration tests (trade_detail.html, politician_page.html, issuer_detail.html, etc.)
- `capitoltraders_cli/src/output_tests.rs` — 34 output format tests (table, JSON, CSV, markdown, DB output)
- `capitoltraders_cli/src/xml_output_tests.rs` — 12 XML wellformedness and roundtrip tests
- `capitoltrades_api/tests/fixtures/` — JSON response fixtures for deserialization tests

## Naming Conventions

**Files:**
- Command implementations: `src/commands/{subcommand}.rs` (trades.rs, politicians.rs, issuers.rs, sync.rs)
- Test files: `{module}_tests.rs` or inline `#[cfg(test)] mod tests` blocks; fixtures in `tests/fixtures/`
- Output modules: `output.rs` (general), `xml_output.rs` (XML-specific)

**Directories:**
- `src/` — Source code for Rust modules
- `tests/` — Integration test files
- `tests/fixtures/` — Test data (HTML, JSON)
- `schema/` — Database DDL and migrations
- `src/commands/` — CLI subcommand implementations
- `src/query/` — Request builders (capitoltrades_api only)
- `src/types/` — Response data models (capitoltrades_api only)

**Modules:**
- `lib.rs` — Crate root with public re-exports
- `mod.rs` — Submodule aggregator (e.g., `commands/mod.rs` re-exports all subcommands)
- Private modules follow functionality (cache.rs, client.rs, db.rs, scrape.rs, validation.rs, analysis.rs)

**Functions:**
- Public entry points: `run()`, `run_db()` in command modules; `new()`, `with_base_url()` for clients
- Validators: `validate_{input_type}()` (validate_state, validate_party, validate_committee, etc.)
- Formatters: `print_{type}_{format}()` (print_trades_table, print_json, print_db_trades_csv, etc.)
- Database: `query_{entity}()`, `replace_{entity}()`, `mark_enriched()`, `update_{entity}_{field}()`

**Types:**
- Capitalized: Trade, Politician, Issuer, TradeQuery, CachedClient, ScrapeClient, Db
- Enums: Party, TxType, Chamber, AssetType, Label, Gender, MarketCap, Sector (in capitoltrades_api); OutputFormat (in capitoltraders_cli)
- Row models (scrape): ScrapedTrade, ScrapedPoliticianCard, ScrapedTradeDetail, ScrapedIssuerDetail
- Row models (DB): DbTradeRow, DbPoliticianRow, DbIssuerRow, DbTradeFilter, DbPoliticianFilter, DbIssuerFilter
- Errors: CapitolTradesError, ScrapeError, DbError

## Where to Add New Code

**New Feature (e.g., new filter or subcommand):**
- Primary code: `capitoltraders_cli/src/commands/{new_feature}.rs` — Implement `run()` and `run_db()` functions
- Validation: Add validator to `capitoltraders_lib/src/validation.rs` (if new input type)
- Tests: Add inline `#[cfg(test)] mod tests` in command file or create `capitoltraders_cli/src/{new_feature}_tests.rs`
- If requires new API field: Add to `capitoltrades_api/src/query/trade.rs` (TradeQuery builder)

**New Output Format:**
- Formatter implementation: `capitoltraders_cli/src/output.rs` — Add `print_{type}_{format}()` function
- Enum variant: Add to `OutputFormat` enum in `output.rs`
- XML only: Use `capitoltraders_cli/src/xml_output.rs` instead (quick-xml Writer)
- Tests: Add cases to `capitoltraders_cli/src/output_tests.rs` or `xml_output_tests.rs`

**New Database Query:**
- Implementation: `capitoltraders_lib/src/db.rs` — Add method to `Db` struct
- Row type: Define or reuse in `src/db.rs` (DbTradeRow, DbPoliticianRow, etc.)
- Filters: Add to `DbTradeFilter` or create new `Db{Entity}Filter` enum
- Tests: Add to inline `#[cfg(test)] mod tests` in `db.rs`

**New Validation Rule:**
- Function: `capitoltraders_lib/src/validation.rs` — Add `validate_{thing}()` function returning Result<T>
- Tests: Add to `capitoltraders_lib/src/validation_tests.rs` (83 existing examples)
- Integration: Call validator in command handler before building query/filter

**New Scrape Feature (e.g., detail page parsing):**
- Implementation: `capitoltraders_lib/src/scrape.rs` — Add method to `ScrapeClient`
- Data type: Define result struct (e.g., `ScrapedTradeDetail`)
- Fixtures: Add HTML file to `capitoltraders_lib/tests/fixtures/` for testing
- Tests: Add to inline `#[cfg(test)] mod tests` in `scrape.rs`

**Utilities (shared helpers):**
- Shared across commands: `capitoltraders_lib/src/analysis.rs` (trade aggregations, statistics)
- Only for CLI output: `capitoltraders_cli/src/output.rs` or `capitoltraders_cli/src/xml_output.rs`

## Special Directories

**schema/:**
- Purpose: Database schema DDL (CREATE TABLE, indexes)
- Generated: No (committed to repo)
- Committed: Yes
- Migrations: Versioned via `Db::migrate_v1()` (checks `PRAGMA user_version`)

**tests/fixtures/:**
- Purpose: Test data (HTML, JSON)
- Generated: No (committed to repo)
- Committed: Yes
- Used by: Scrape tests (HTML), deserialization tests (JSON)

**target/:**
- Purpose: Compiled artifacts
- Generated: Yes (by cargo build)
- Committed: No (.gitignore)

**.planning/codebase/:**
- Purpose: Analysis documents (this file)
- Generated: Yes (by gsd mapper)
- Committed: Yes (for future phases)

---

*Structure analysis: 2026-02-09*
