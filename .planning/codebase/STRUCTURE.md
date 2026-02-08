# Codebase Structure

**Analysis Date:** 2026-02-07

## Directory Layout

```
capitoltraders/
├── capitoltrades_api/           # Vendored HTTP API client (upstream fork)
│   ├── src/
│   │   ├── lib.rs               # Crate root, re-exports
│   │   ├── client.rs            # HTTP client with reqwest
│   │   ├── errors.rs            # Error types
│   │   ├── user_agent.rs        # Random user-agent rotation
│   │   ├── types/               # Response type models
│   │   │   ├── mod.rs
│   │   │   ├── trade.rs         # Trade, Asset, TxType, AssetType, Label
│   │   │   ├── politician.rs    # Politician, Chamber, Gender, Party
│   │   │   └── issuer.rs        # IssuerDetail, MarketCap, Sector
│   │   └── query/               # Request builders (Query trait + implementations)
│   │       ├── mod.rs
│   │       ├── common.rs        # Query trait, QueryCommon, SortDirection
│   │       ├── trade.rs         # TradeQuery with 30+ filter fields
│   │       ├── politician.rs    # PoliticianQuery with party/state/committee
│   │       └── issuer.rs        # IssuerQuery with filtering
│   ├── tests/
│   │   ├── deserialization.rs   # JSON fixture parsing tests
│   │   ├── query_builders.rs    # URL parameter encoding tests (36 tests)
│   │   ├── client_integration.rs # wiremock integration tests
│   │   └── fixtures/            # JSON test fixtures (trades.json, etc.)
│   └── Cargo.toml
├── capitoltraders_lib/          # Core library layer
│   ├── src/
│   │   ├── lib.rs               # Re-exports all modules
│   │   ├── scrape.rs            # HTML scraper (ScrapeClient, ScrapedTrade, etc.)
│   │   ├── client.rs            # CachedClient wrapper (legacy rate limiting + cache)
│   │   ├── cache.rs             # DashMap-backed TTL cache (MemoryCache)
│   │   ├── db.rs                # SQLite storage (Db struct, upsert logic)
│   │   ├── error.rs             # CapitolTradesError enum
│   │   ├── validation.rs        # Input validators + committee mapping
│   │   └── analysis.rs          # Trade aggregation helpers
│   ├── tests/                   # (Tests co-located in modules)
│   └── Cargo.toml
├── capitoltraders_cli/          # CLI binary
│   ├── src/
│   │   ├── main.rs              # CLI entry point (clap parser, command router)
│   │   ├── output.rs            # OutputFormat enum + print_* formatting functions
│   │   ├── xml_output.rs        # XML serialization (JSON-to-XML bridge)
│   │   └── commands/
│   │       ├── mod.rs           # Module declarations
│   │       ├── trades.rs        # trades subcommand
│   │       ├── politicians.rs   # politicians subcommand
│   │       ├── issuers.rs       # issuers subcommand
│   │       └── sync.rs          # sync subcommand (SQLite ingestion)
│   ├── tests/
│   │   └── schema_validation.rs # JSON Schema validation against output
│   └── Cargo.toml
├── schema/                      # Data shape documentation
│   ├── sqlite.sql               # SQLite DDL (trades, politicians, issuers, etc.)
│   ├── trade.schema.json        # JSON Schema for trade output
│   ├── politician.schema.json   # JSON Schema for politician output
│   ├── issuer.schema.json       # JSON Schema for issuer output
│   ├── trades.xsd               # XML Schema for trades output
│   ├── politicians.xsd          # XML Schema for politicians output
│   └── issuers.xsd              # XML Schema for issuers output
├── .github/workflows/           # CI configuration
│   └── sqlite-sync.yml          # Daily SQLite sync workflow
├── Cargo.toml                   # Workspace root (members, dependencies)
└── CLAUDE.md                    # Project instructions
```

## Directory Purposes

**capitoltrades_api/src/:**
- Purpose: Typed HTTP client library for the upstream BFF API
- Contains: Client struct (reqwest-based), Query trait for building requests, types for Trade/Politician/Issuer responses
- Key files: `client.rs` (HTTP), `types/mod.rs` (response types), `query/common.rs` (Query trait)

**capitoltrades_api/tests/:**
- Purpose: Verify query parameter encoding and response deserialization
- Contains: 7 deserialization tests, 36 query builder tests, 8 wiremock integration tests
- Fixtures: `trades.json`, `politicians.json`, `issuers.json` (sample API responses)

**capitoltraders_lib/src/:**
- Purpose: Business logic: scraping, validation, caching, persistence, analysis
- Contains: HTML scraper (main data source), SQLite ingestion, validators, error types, analysis helpers
- Key separation: scrape.rs (network/parse), cache.rs (in-memory store), db.rs (SQLite), validation.rs (input normalization)

**capitoltraders_cli/src/:**
- Purpose: User interface: command parsing, filtering, output formatting
- Contains: Four subcommands (trades, politicians, issuers, sync), format renderers
- Key separation: main.rs (entry), commands/ (subcommand logic), output.rs (all non-XML formats), xml_output.rs (XML only)

**capitoltraders_cli/tests/:**
- Purpose: Validate CLI output against JSON/XML schemas
- Contains: 9 schema validation tests

**schema/:**
- Purpose: Document the structure of CLI output (what --output json/xml actually emit)
- Contains: JSON Schema (draft 2020-12) and XSD files for validation
- Also contains: sqlite.sql (schema for sync command's database)

## Key File Locations

**Entry Points:**
- `capitoltraders_cli/src/main.rs`: CLI binary main()
- `capitoltraders_cli/src/commands/trades.rs`: trades command run()
- `capitoltraders_cli/src/commands/politicians.rs`: politicians command run()
- `capitoltraders_cli/src/commands/issuers.rs`: issuers command run()
- `capitoltraders_cli/src/commands/sync.rs`: sync command run()

**Configuration:**
- `Cargo.toml` (root): Workspace members and shared dependencies
- `capitoltraders_cli/Cargo.toml`: CLI-specific dependencies (clap, tabled, quick-xml)
- `capitoltraders_lib/Cargo.toml`: Library dependencies (dashmap, rusqlite, regex)
- `capitoltrades_api/Cargo.toml`: API client dependencies (reqwest, serde)

**Core Logic:**
- `capitoltraders_lib/src/scrape.rs`: HTML scraper (ScrapeClient, retry logic, RSC payload parsing)
- `capitoltraders_lib/src/validation.rs`: All input validators + COMMITTEE_MAP (48 committees)
- `capitoltraders_lib/src/db.rs`: SQLite upsert operations
- `capitoltraders_lib/src/cache.rs`: DashMap-backed TTL cache
- `capitoltraders_lib/src/error.rs`: Error type hierarchy

**Output Formatting:**
- `capitoltraders_cli/src/output.rs`: Table (tabled), JSON, CSV, Markdown formats
- `capitoltraders_cli/src/xml_output.rs`: XML serialization (JSON-to-XML bridge)

**Type Definitions:**
- `capitoltrades_api/src/types/trade.rs`: Trade, Asset, TxType, AssetType (22 variants), Label (4 variants)
- `capitoltrades_api/src/types/politician.rs`: Politician, PoliticianDetail, Chamber, Gender, Party
- `capitoltrades_api/src/types/issuer.rs`: IssuerDetail, MarketCap (6 variants), Sector (12 variants)

**Testing:**
- `capitoltrades_api/tests/fixtures/`: JSON test data (trades.json, politicians.json, issuers.json)
- `capitoltraders_lib/src/*.rs`: Unit tests in #[cfg(test)] modules
- `capitoltraders_cli/tests/schema_validation.rs`: Schema compliance tests

## Naming Conventions

**Files:**
- Rust modules: `snake_case.rs` (e.g., `scrape.rs`, `trade.rs`, `user_agent.rs`)
- Subcommands: `{command}.rs` in `commands/` (e.g., `trades.rs`, `sync.rs`)
- SQL schema: `sqlite.sql`
- Schema docs: `{entity}.schema.json`, `{entity}.xsd`

**Directories:**
- Crates: `snake_case` (e.g., `capitoltraders_lib`, `capitoltrades_api`)
- Modules: `snake_case` (e.g., `types/`, `query/`, `commands/`)
- Tests: `tests/` or co-located `#[cfg(test)]` modules

**Types & Enums:**
- Type names: `PascalCase` (e.g., `Trade`, `ScrapeClient`, `OutputFormat`)
- Enum variants: `PascalCase` (e.g., `OutputFormat::Json`, `Party::Democrat`)
- Struct fields: `snake_case` (e.g., `tx_date`, `politician_id`, `base_url`)

**Functions & Methods:**
- Function names: `snake_case` (e.g., `validate_party()`, `trades_by_month()`)
- Builder methods: `with_*()` (e.g., `with_base_url()`, `with_party()`)
- Validator functions: `validate_*()` (e.g., `validate_search()`, `validate_state()`)

**Constants:**
- Uppercase with underscores: `MAX_SEARCH_LENGTH`, `VALID_STATES`, `COMMITTEE_MAP`

## Where to Add New Code

**New CLI Subcommand:**
1. Create `capitoltraders_cli/src/commands/{command}.rs` with a clap Args struct and `async fn run(...)`
2. Add the module to `capitoltraders_cli/src/commands/mod.rs`
3. Add a variant to the Commands enum in `capitoltraders_cli/src/main.rs`
4. Wire it into the match block in main()

**New Output Format:**
1. Add OutputFormat variant to the enum in `capitoltraders_cli/src/output.rs`
2. Implement `print_trades_{format}()`, `print_politicians_{format}()`, `print_issuers_{format}()` functions
3. Match on the format in command handlers and call the appropriate print function

**New Query Filter (for upstream API):**
1. Add field to TradeQuery/PoliticianQuery/IssuerQuery in `capitoltrades_api/src/query/{entity}.rs`
2. Add builder method (e.g., `with_new_field()`)
3. Implement URL encoding in add_to_url()
4. Add snapshot test in query_builders.rs

**New Validator:**
1. Create function in `capitoltraders_lib/src/validation.rs` following pattern: `pub fn validate_{field}(input: &str) -> Result<T, CapitolTradesError>`
2. Return typed result (e.g., String, Party enum, i64)
3. Add tests in the #[cfg(test)] module at bottom of validation.rs
4. Call validator from command handler before building query

**New Database Table:**
1. Add DDL to `schema/sqlite.sql`
2. Add Db method in `capitoltraders_lib/src/db.rs` for querying/upserting
3. Export new method from lib.rs if needed by CLI

**New Analysis Function:**
1. Add to `capitoltraders_lib/src/analysis.rs`
2. Signature: `pub fn {operation}(trades: &[Trade]) -> {ResultType}`
3. No network calls; operate on slices only
4. Add unit tests in #[cfg(test)] module

## Special Directories

**target/:**
- Purpose: Cargo build artifacts
- Generated: Yes
- Committed: No (.gitignored)

**.github/workflows/:**
- Purpose: CI/CD automation
- Contains: `sqlite-sync.yml` (daily sync of trades into SQLite)
- Committed: Yes

**.planning/codebase/:**
- Purpose: Architecture/structure documentation (generated by orchestrator)
- Generated: Yes
- Committed: Yes

**schema/:**
- Purpose: Data shape documentation (JSON Schema, XSD, SQL DDL)
- Generated: Partially (DDL is hand-written; schemas are hand-written to avoid vendored crate modifications)
- Committed: Yes

---

*Structure analysis: 2026-02-07*
