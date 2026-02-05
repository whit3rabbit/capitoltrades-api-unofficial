# Codebase Structure

**Analysis Date:** 2026-02-05

## Directory Layout

```
capitoltraders/
├── Cargo.toml                    # Workspace root, shared dependencies
├── Cargo.lock                    # Locked dependency versions
├── .planning/
│   └── codebase/                 # GSD analysis documents
├── capitoltrades_api/            # Vendored HTTP client (upstream fork)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                # Crate root, re-exports
│   │   ├── client.rs             # Client: HTTP reqwest wrapper
│   │   ├── errors.rs             # Upstream Error enum
│   │   ├── user_agent.rs         # Random user-agent rotation
│   │   ├── types/
│   │   │   ├── mod.rs            # Re-exports all types
│   │   │   ├── trade.rs          # Trade, TradeSize, TxType, AssetType, Label
│   │   │   ├── politician.rs      # Politician, PoliticianDetail, Chamber, Gender, Party
│   │   │   ├── issuer.rs         # IssuerDetail, MarketCap, Sector, Performance, EodPrice
│   │   │   └── meta.rs           # Meta, Paging, PaginatedResponse, Response
│   │   └── query/
│   │       ├── mod.rs            # Re-exports Query trait and types
│   │       ├── common.rs         # Query trait, QueryCommon, SortDirection
│   │       ├── trade.rs          # TradeQuery (20+ fields), TradeSortBy
│   │       ├── politician.rs      # PoliticianQuery, PoliticianSortBy
│   │       └── issuer.rs         # IssuerQuery, IssuerSortBy
│   └── tests/
│       ├── deserialization.rs    # JSON fixture deserialization tests
│       ├── query_builders.rs     # URL parameter encoding tests (36 cases)
│       ├── client_integration.rs # Wiremock integration tests
│       └── fixtures/
│           ├── trades.json       # Sample trade response
│           ├── politicians.json  # Sample politician response
│           └── issuers.json      # Sample issuer response
├── capitoltraders_lib/           # Caching, validation, analysis layer
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                # Crate root, re-exports
│       ├── client.rs             # CachedClient wrapper, cache key generation
│       ├── cache.rs              # MemoryCache: DashMap-backed TTL cache
│       ├── validation.rs         # Input validation (18 validators, COMMITTEE_MAP)
│       ├── analysis.rs           # Trade analysis: by_party, by_sector, top_issuers, by_month, total_volume
│       └── error.rs              # CapitolTradesError enum
├── capitoltraders_cli/           # CLI binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # CLI entry point, Commands enum, tokio runtime
│       ├── output.rs             # Formatting layer: table/JSON/CSV/markdown
│       └── commands/
│           ├── mod.rs            # Module declarations
│           ├── trades.rs         # trades subcommand (TradesArgs + run)
│           ├── politicians.rs    # politicians subcommand (PoliticiansArgs + run)
│           └── issuers.rs        # issuers subcommand (IssuersArgs + run)
└── CLAUDE.md                     # Project instructions (vendoring notes, conventions)
```

## Directory Purposes

**capitoltrades_api/ (vendored upstream):**
- Purpose: HTTP client for CapitolTrades BFF API (https://bff.capitoltrades.com)
- Contains: reqwest-based HTTP client, type definitions, Query trait + implementations
- Key files: `src/client.rs` (Client struct), `src/types/` (domain types), `src/query/` (query builders)
- Scope: Read-only fork from https://github.com/TommasoAmici/capitoltrades crates/ subdirectory
- Modifications documented in CLAUDE.md (base_url flexibility, pub fields, new filters)

**capitoltraders_lib/ (library layer):**
- Purpose: Caching, validation, error handling, analysis helpers
- Contains: CachedClient, MemoryCache, validation module, error types, analysis functions
- Key files: `src/client.rs` (cache wrapper), `src/cache.rs` (TTL cache), `src/validation.rs` (input normalization)
- Scope: User input is validated here before reaching API layer

**capitoltraders_cli/ (CLI binary):**
- Purpose: Command-line interface for querying and analyzing congressional trades
- Contains: Subcommands (trades, politicians, issuers), output formatting
- Key files: `src/main.rs` (entry point), `src/commands/` (subcommands), `src/output.rs` (formatters)
- Scope: User-facing CLI binary named `capitoltraders`

## Key File Locations

**Entry Points:**
- `capitoltraders_cli/src/main.rs`: CLI entry point, parses global flags, dispatches to subcommands
- `capitoltraders_lib/src/client.rs`: CachedClient.new(), public interface for all queries
- `capitoltrades_api/src/client.rs`: Raw HTTP Client, get_trades/get_politicians/get_issuers/get_issuer

**Configuration:**
- `Cargo.toml`: Workspace configuration, shared dependency versions
- `capitoltraders_cli/src/main.rs`: Hardcoded TTL (300s), default output format ("table")

**Core Logic:**
- `capitoltrades_api/src/query/`: Query trait and implementations (URL encoding)
- `capitoltraders_lib/src/validation.rs`: Input validation (18 validators, COMMITTEE_MAP)
- `capitoltraders_lib/src/cache.rs`: TTL cache implementation
- `capitoltraders_lib/src/client.rs`: CachedClient wrapper, cache key generation

**Testing:**
- `capitoltrades_api/tests/`: Deserialization, query builders, integration tests
- `capitoltrades_api/src/query/*.rs`: Inline snapshot tests (insta)
- `capitoltraders_lib/src/cache.rs`: Inline cache unit tests
- `capitoltraders_lib/src/analysis.rs`: Inline analysis unit tests
- `capitoltraders_lib/src/validation.rs`: Inline validation unit tests (83 cases)

## Naming Conventions

**Files:**
- `*.rs`: Rust source files
- `lib.rs`: Crate root, typically re-exports public types
- `main.rs`: Binary entry point
- `mod.rs`: Module declarations (empty re-exports)
- `test file naming`: Matches source file (trades_test.rs tests trades.rs) or use #[cfg(test)] mod tests

**Directories:**
- `src/`: Rust source
- `tests/`: Integration tests (Cargo test harness)
- `commands/`: CLI subcommand implementations
- `types/`: Domain types (Trade, Politician, Issuer)
- `query/`: Query builder types (TradeQuery, PoliticianQuery, IssuerQuery)
- `.planning/codebase/`: GSD analysis documents

**Modules & Types:**
- Snake_case: files, module names, functions, variables
- PascalCase: structs, enums, traits, type aliases
- UPPER_SNAKE_CASE: constants (COMMITTEE_MAP, VALID_STATES, MAX_SEARCH_LENGTH)
- Example: `validate_party()` function in validation.rs returns Party enum

**Identifiers:**
- Abbreviations: `tx_` prefix for transaction-related fields (tx_type, tx_date, tx_days)
- CLI flags: kebab-case (--market-cap, --asset-type, --tx-type)
- API params: camelCase per upstream BFF API (tradeSize, assetType, txType, mcap)

## Where to Add New Code

**New Filter for Trades Command:**
1. Add field to `capitoltrades_api/src/query/trade.rs` TradeQuery struct
2. Add builder method to TradeQuery impl (e.g., `with_new_filter()`)
3. Add URL encoding in TradeQuery::add_to_url()
4. Add validation function in `capitoltraders_lib/src/validation.rs`
5. Add CLI arg to `capitoltraders_cli/src/commands/trades.rs` TradesArgs struct
6. Add parsing logic in trades::run() that validates and calls builder
7. Update cache key generation in `capitoltraders_lib/src/client.rs` query_to_cache_key()
8. Add tests in `capitoltrades_api/tests/query_builders.rs`

**New Subcommand (e.g., `transactions`):**
1. Create `capitoltraders_cli/src/commands/transactions.rs` with TransactionArgs struct and async run() function
2. Add module declaration to `capitoltraders_cli/src/commands/mod.rs`: `pub mod transactions;`
3. Add Commands::Transactions variant to enum in `capitoltraders_cli/src/main.rs`
4. Add match arm in main() to dispatch to commands::transactions::run()
5. If querying new API endpoints, add methods to `capitoltrades_api/src/client.rs` Client
6. Add output struct (TransactionRow) and print functions to `capitoltraders_cli/src/output.rs`

**New Analysis Function:**
- Location: `capitoltraders_lib/src/analysis.rs`
- Pattern: Function takes &[Trade] slice, returns collection (Vec, HashMap, BTreeMap)
- Example: `pub fn trades_by_party(trades: &[Trade]) -> HashMap<String, Vec<&Trade>>`
- Add tests inline in module with #[cfg(test)] mod tests

**Shared Utilities:**
- Location: `capitoltraders_lib/src/` for logic, `capitoltrades_api/src/` for types
- Validation: `capitoltraders_lib/src/validation.rs`
- Error handling: `capitoltraders_lib/src/error.rs`
- Caching: `capitoltraders_lib/src/cache.rs`

## Special Directories

**capitoltrades_api/tests/:**
- Purpose: Integration tests with wiremock HTTP mocking
- Generated: JSON snapshots via insta snapshot testing (committed to git)
- Committed: Yes, snapshots committed for regression testing
- Example: `capitoltrades_api/tests/query_builders.rs` tests URL encoding for all 36 query parameter combinations

**capitoltrades_api/tests/fixtures/:**
- Purpose: Hardcoded JSON responses for deserialization and analysis tests
- Generated: No (manually created)
- Committed: Yes (test data)
- Files: trades.json, politicians.json, issuers.json (sample CapitolTrades API responses)

**.planning/codebase/:**
- Purpose: GSD codebase mapping documents
- Generated: Yes (by GSD mapping commands)
- Committed: Yes (planning artifacts)
- Contents: ARCHITECTURE.md, STRUCTURE.md, and other analysis documents

---

*Structure analysis: 2026-02-05*
