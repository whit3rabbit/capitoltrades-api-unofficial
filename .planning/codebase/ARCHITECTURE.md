# Architecture

**Analysis Date:** 2026-02-07

## Pattern Overview

**Overall:** Three-tier Rust workspace with a vendored HTTP API client layer, a library layer providing caching/validation/scraping, and a CLI presentation layer.

**Key Characteristics:**
- Query builder pattern for constructing typed API requests
- Scrape-first design: CLI uses HTML scraping from `capitoltrades.com`, not the legacy HTTP API
- In-memory TTL caching with concurrent access via DashMap
- SQLite ingestion for offline analysis
- Multi-format output: table, JSON, CSV, Markdown, XML
- Input validation before all filtering operations

## Layers

**Upstream API (`capitoltrades_api`):**
- Purpose: Typed HTTP client for the legacy CapitolTrades BFF API (`https://bff.capitoltrades.com`). Vendored fork with enhancements for filter field builders and custom base URLs (for testing).
- Location: `capitoltrades_api/src/`
- Contains: HTTP client (`client.rs`), error types (`errors.rs`), typed request builders (`query/`), response types (`types/`), user-agent rotation (`user_agent.rs`)
- Depends on: reqwest, serde, url, chrono
- Used by: `capitoltraders_lib` (legacy usage for rate limiting/caching), tests, and potential future API mode

**Library Layer (`capitoltraders_lib`):**
- Purpose: Wraps the vendored API with scraping, caching, validation, and analysis helpers. This is the core business logic layer.
- Location: `capitoltraders_lib/src/`
- Contains:
  - `scrape.rs`: HTML scraper for trades, politicians, issuers (main data source)
  - `client.rs`: CachedClient wrapper (legacy, mostly unused in CLI)
  - `cache.rs`: DashMap-backed TTL cache (TTL: 300s)
  - `db.rs`: SQLite storage and upsert logic
  - `validation.rs`: Input validation for all filter types (48 committees, enum aliases, date parsing)
  - `analysis.rs`: Trade aggregation helpers (by party, by ticker, by month, top traders, volume)
  - `error.rs`: CapitolTradesError enum wrapping API/cache/validation errors
- Depends on: capitoltrades_api, dashmap, reqwest, rusqlite, chrono, regex, serde_json
- Used by: `capitoltraders_cli`

**CLI Layer (`capitoltraders_cli`):**
- Purpose: User-facing command interface with four subcommands. Parses arguments, applies validations, queries the scraper, and formats output.
- Location: `capitoltraders_cli/src/`
- Contains:
  - `main.rs`: CLI entry point, tokio runtime, command routing
  - `commands/`: Subcommand implementations (trades, politicians, issuers, sync)
  - `output.rs`: Formatting (table, JSON, CSV, Markdown)
  - `xml_output.rs`: XML serialization via JSON-to-XML bridge (quick-xml)
- Depends on: capitoltraders_lib, clap, tabled, quick-xml, serde_json, csv, tracing
- Used by: End users via binary `capitoltraders`

## Data Flow

**Query Flow (trades command):**

1. CLI parses arguments: filter flags, sort, pagination, output format
2. Validation layer normalizes input (e.g., "d" -> "democrat", "CA" -> uppercase, committee name -> code)
3. ScrapeClient fetches paginated HTML from capitoltrades.com
4. HTML parser extracts RSC payloads and deserializes to ScrapedTrade structs
5. Client-side filtering applies date ranges, multi-value filters (comma-separated)
6. Row builders flatten nested types (Trade -> TradeRow)
7. Output formatter renders: table (tabled), JSON (serde_json), CSV (csv crate), Markdown (tabled::Style::markdown()), XML (quick-xml)
8. Results written to stdout; stderr logs pagination meta

**Sync Flow (sync command):**

1. ScrapeClient fetches trades page-by-page with optional trade detail pages
2. Politicians and issuers are also scraped
3. Db::upsert_trades() executes transactions to normalize data:
   - Trades -> trades table + committees/labels join tables
   - Issuers -> issuers + issuer_performance + issuer_eod_prices
   - Politicians -> politicians + politician_committees
4. Ingest metadata tracks last_trade_pub_date for incremental runs
5. Supports --full, --since override, --refresh-politicians/issuers, --with-trade-details

**State Management:**

- Runtime state: parsed arguments, OutputFormat enum, scraper instance
- Cache state: MemoryCache (DashMap) with TTL expiration (legacy, not used by CLI scraping)
- Persistent state: SQLite database with schema in `schema/sqlite.sql`
- No global mutable state; all state passed explicitly through function calls

## Key Abstractions

**Query Trait (`capitoltrades_api::query`):**
- Purpose: Unified interface for building URL-encoded query parameters
- Examples: `TradeQuery`, `PoliticianQuery`, `IssuerQuery` in `capitoltrades_api/src/query/`
- Pattern: Builder methods (e.g., `with_party()`, `with_state()`) return Self; `add_to_url()` encodes to URL

**ScrapeClient (`capitoltraders_lib::scrape`):**
- Purpose: Unofficial API via HTML scraping with retry/backoff logic
- Examples: `trades_page()`, `trade_detail()`, `politicians_page()`, `issuer_detail()`
- Pattern: Each method returns ScrapePage<T> with data, total_pages, total_count

**ScrapedTypes vs UpstreamTypes:**
- `ScrapedTrade`, `ScrapedPolitician`, `ScrapedIssuer*`: Raw deserializable structs from HTML payloads
- `Trade`, `PoliticianDetail`, `IssuerDetail`: Upstream types with full metadata (used in output/analysis)
- Scraped types have nullable/optional fields; missing data populated with safe defaults

**Validators (`capitoltraders_lib::validation`):**
- Purpose: Normalize and validate all user input before filtering
- Examples: `validate_party()`, `validate_committee()`, `validate_date()`, `validate_days()`
- Pattern: Returns typed result or CapitolTradesError::InvalidInput

**OutputFormat Enum + Format Functions:**
- Purpose: Decouple data from presentation
- Examples: `print_trades_table()`, `print_trades_json()`, `print_trades_csv()`, `print_trades_xml()`
- Pattern: Accept upstream type slice, map to row struct, serialize/render

## Entry Points

**CLI Binary (`capitoltraders`):**
- Location: `capitoltraders_cli/src/main.rs`
- Triggers: Invoked by user with subcommand
- Responsibilities:
  1. Parse CLI arguments (clap)
  2. Construct ScrapeClient (with optional base_url override)
  3. Route to command handler
  4. Print output or error

**Trades Subcommand (`capitoltraders trades`):**
- Location: `capitoltraders_cli/src/commands/trades.rs`
- Triggers: `capitoltraders trades --name "Smith" --party d`
- Responsibilities:
  1. Validate 24 filter arguments
  2. Convert --since/--until (absolute dates) to --days (relative)
  3. Paginate through scraper results
  4. Apply client-side filtering
  5. Sort results
  6. Format and print output

**Sync Subcommand (`capitoltraders sync`):**
- Location: `capitoltraders_cli/src/commands/sync.rs`
- Triggers: `capitoltraders sync --db trades.db --since 2026-01-01`
- Responsibilities:
  1. Open/init SQLite database
  2. Determine sync scope (full, incremental, refresh)
  3. Paginate scraper for trades, politicians, issuers
  4. Optionally fetch per-trade detail pages
  5. Upsert into normalized schema
  6. Update metadata (last_trade_pub_date)

**ScrapeClient (`capitoltraders_lib::scrape`):**
- Location: `capitoltraders_lib/src/scrape.rs`
- Entry: `ScrapeClient::new()` or `ScrapeClient::with_base_url()`
- Responsibilities:
  1. HTTP GET with retries, backoff, jitter
  2. Parse RSC payloads from HTML
  3. Deserialize to typed structs
  4. Handle pagination headers
  5. Enforce rate limiting

## Error Handling

**Strategy:** Layered error types with context preservation. All errors converted to anyhow::Result at CLI boundary.

**Patterns:**

1. **Upstream Errors (capitoltrades_api::Error):**
   - HttpStatus, RequestFailed, UnsupportedVersion
   - Wrapped as CapitolTradesError::Api

2. **Scrape Errors (capitoltraders_lib::scrape::ScrapeError):**
   - HttpStatus (with Retry-After header support), MissingPayload, Json, Parse
   - Wrapped as CapitolTradesError via thiserror
   - Retry logic: exponential backoff, max 3 attempts (configurable via env)

3. **Validation Errors (CapitolTradesError::InvalidInput):**
   - Max length exceeded, invalid enum value, out-of-range number
   - Returned from validate_* functions

4. **Database Errors (capitoltraders_lib::db::DbError):**
   - SQLite errors, JSON errors, date parse errors
   - Wrapped via rusqlite::Error From impl

5. **CLI Error Handling (capitoltraders_cli::main):**
   - All Result types converted to anyhow::Result
   - Errors printed to stderr with context
   - Exit code 1 on failure

## Cross-Cutting Concerns

**Logging:**
- Framework: tracing subscriber with env_filter
- Default level: info (set to `capitoltraders=info`)
- Output: stderr (timestamp suppressed, target suppressed)
- Examples: "Starting full sync", "HTTP error for URL", "Invalid URL constructed"

**Validation:**
- All user inputs validated before use
- Normalization: uppercase states, lowercase countries, enum aliases (d -> democrat)
- Comma-separated values split and individually validated
- Max lengths enforced (search: 100 bytes, committee: 80 bytes)

**Rate Limiting:**
- Scraper: RetryConfig with max_retries (3), base_delay_ms (2000), max_delay_ms (30000)
- Exponential backoff with 0.8-1.2x jitter
- Legacy CachedClient: 5-10 second delay between requests (not used in CLI)
- Both configurable via env: CAPITOLTRADES_RETRY_*

**Pagination:**
- CLI enforces page_size 1-100 (default 12 for scrape listing pages)
- ScrapePage struct: data: Vec<T>, total_pages: Option<i64>, total_count: Option<i64>
- Scraper returns fixed 12 results for listing pages (--page-size ignored)
- Sync iterates all pages until exhausted

**Date Handling:**
- chrono::NaiveDate for absolute dates (YYYY-MM-DD)
- Relative days as i64 (e.g., 7 -> "last 7 days")
- Conversion: NaiveDate -> days from today via validate_date -> date_to_relative_days
- Conflicts enforced at clap level (--days conflicts with --since/--until)

---

*Architecture analysis: 2026-02-07*
