# Architecture

**Analysis Date:** 2026-02-05

## Pattern Overview

**Overall:** Three-layer Rust workspace with vendored HTTP client, caching library layer, and CLI application.

**Key Characteristics:**
- Workspace design with shared dependencies across three crates
- Trait-based query builder pattern (Query trait in `capitoltrades_api`)
- Layered composition: raw API client wrapped by cached wrapper, consumed by CLI
- TTL-based in-memory caching with concurrent access via DashMap
- Async/await throughout using tokio runtime
- Validation layer that normalizes user input before API calls

## Layers

**API Layer (`capitoltrades_api`):**
- Purpose: HTTP client for CapitolTrades BFF API (upstream vendored fork)
- Location: `capitoltrades_api/src/`
- Contains: Client, Query traits, Request/response types, User-Agent rotation
- Depends on: reqwest (HTTP), url (URL parsing), serde (JSON serialization)
- Used by: CachedClient in `capitoltraders_lib`

**Library Layer (`capitoltraders_lib`):**
- Purpose: Caching, validation, error handling, and analysis helpers
- Location: `capitoltraders_lib/src/`
- Contains: CachedClient wrapper, MemoryCache, validation functions, error types, analysis helpers
- Depends on: capitoltrades_api, dashmap (concurrent cache), chrono (date handling)
- Used by: CLI binary

**CLI Layer (`capitoltraders_cli`):**
- Purpose: User-facing command-line interface with subcommands
- Location: `capitoltraders_cli/src/`
- Contains: Commands (trades, politicians, issuers), argument parsing, output formatting
- Depends on: capitoltraders_lib, clap (CLI parsing), tabled (table formatting)
- Used by: End users via `capitoltraders` binary

## Data Flow

**Query Execution (typical flow):**

1. CLI parses command-line arguments via clap into typed Args structs (`TradesArgs`, `PoliticiansArgs`, `IssuersArgs`)
2. Each command validates user input via `capitoltraders_lib::validation::*` functions
3. Validated values are assembled into a Query object (TradeQuery, PoliticianQuery, IssuerQuery)
4. CachedClient checks MemoryCache for matching query key
5. Cache miss: Client.get_*() calls API via reqwest, deserializes JSON response
6. Response cached as JSON string with TTL
7. Response formatted (table, JSON, CSV, or markdown) and output to stdout

**Example: `capitoltraders trades --party democrat --days 7`**

1. Clap parses to TradesArgs { party: Some("democrat"), days: Some(7), ... }
2. validate_party("democrat") returns Party::Democrat
3. TradeQuery built with .with_party(&Party::Democrat).with_pub_date_relative(7)
4. query_to_cache_key(query) generates "trades:p1:s20:...pa[democrat]:pdr7:..."
5. Cache lookup in MemoryCache.store
6. If miss, Client.get_trades(query) builds URL with query.add_to_url() and fetches
7. Response deserialized to PaginatedResponse<Trade>
8. Cached as JSON
9. Formatted and printed as table (default) or other format

**State Management:**
- Immutable: Query objects, validated CLI arguments, API responses
- Mutable: CachedClient internally holds shared MemoryCache (via Rc/Arc pattern)
- No request/response mutation after deserialization
- Cache TTL is 300 seconds (5 minutes), per-instance lifecycle

## Key Abstractions

**Query Trait (capitoltrades_api::query::common):**
- Purpose: Standardize how different query types encode themselves as URL parameters
- Examples: `TradeQuery`, `PoliticianQuery`, `IssuerQuery` all implement Query
- Pattern: Each query type implements add_to_url() to append its specific parameters, calls common.add_to_url() for pagination/dates

**CachedClient Wrapper:**
- Purpose: Transparent caching layer over raw Client
- Location: `capitoltraders_lib/src/client.rs`
- Pattern: Implements same public interface as Client (get_trades, get_politicians, get_issuers, get_issuer)
- Cache key generated from full query state via query_to_cache_key() functions

**MemoryCache:**
- Purpose: Thread-safe, TTL-based in-memory cache
- Location: `capitoltraders_lib/src/cache.rs`
- Pattern: DashMap<String, CacheEntry> where CacheEntry holds JSON string + expiration time
- Access: get(key) checks expiration and auto-removes stale entries, set(key, value) stores with TTL

**Validation Functions:**
- Purpose: Normalize user input to API-compatible types
- Location: `capitoltraders_lib/src/validation.rs`
- Pattern: Each validate_*() function maps CLI string input to typed enum or normalized string
- Examples: validate_party("d") -> Party::Democrat, validate_committee("hsag") -> String("hsag")

**Error Type (CapitolTradesError):**
- Purpose: Unified error handling across layers
- Location: `capitoltraders_lib/src/error.rs`
- Variants: Api (wrapped capitoltrades_api::Error), Cache, Serialization, InvalidInput
- Implements From<> for propagation with ?

## Entry Points

**CLI Binary (`capitoltraders`):**
- Location: `capitoltraders_cli/src/main.rs`
- Triggers: `capitoltraders trades|politicians|issuers [OPTIONS]`
- Responsibilities:
  1. Parse command-line arguments
  2. Create MemoryCache (TTL 300s) and CachedClient
  3. Dispatch to appropriate command handler
  4. Return Result<(), anyhow::Error> to tokio runtime

**Trades Subcommand:**
- Location: `capitoltraders_cli/src/commands/trades.rs`
- Triggers: `capitoltraders trades [--party, --days, --state, ...]`
- Responsibilities:
  1. Validate all input via validation module
  2. Build TradeQuery from validated inputs
  3. Execute cached query via client.get_trades()
  4. Format results and print

**Politicians Subcommand:**
- Location: `capitoltraders_cli/src/commands/politicians.rs`
- Triggers: `capitoltraders politicians [--party, --state, --committee, ...]`
- Responsibilities:
  1. Validate inputs (prioritize --name over hidden --search alias)
  2. Build PoliticianQuery
  3. Execute and format results

**Issuers Subcommand:**
- Location: `capitoltraders_cli/src/commands/issuers.rs`
- Triggers: `capitoltraders issuers [--search, --state, --market-cap, ...]`
- Responsibilities:
  1. Validate inputs
  2. Build IssuerQuery
  3. Execute and format results

## Error Handling

**Strategy:** Early validation + transparent propagation with context

**Patterns:**
- CLI-level: All user input validated before query construction (fail fast)
- Client-level: API errors wrapped in CapitolTradesError::Api, logged via tracing
- Serialization errors: JSON parse failures wrapped in CapitolTradesError::Serialization
- Cache errors: JSON cache storage failures logged but don't block response (response used directly)
- Command-level: anyhow::Result<()> for main() exit code propagation

**Example error flow:**
```
validate_party("invalid") -> Err(CapitolTradesError::InvalidInput("..."))
                          -> anyhow::bail!() in command handler
                          -> main() returns Err(anyhow::Error)
                          -> process exit code 1
```

## Cross-Cutting Concerns

**Logging:**
- Framework: tracing (structured logging)
- Initialization: `tracing_subscriber::fmt()` in main.rs with env filter
- Levels: error (HTTP client build failures, API parse failures), trace (query execution)
- Prefix: "capitoltraders=info" default level

**Validation:**
- All user input validated in capitoltraders_lib::validation module before query construction
- State codes normalized to uppercase, committees resolved from full names to API codes
- Search strings sanitized (control chars stripped, length limited to 100 bytes)
- Enum-based inputs (party, gender, market_cap, etc.) validate against known variants

**Authentication:**
- None required (API is public)
- User-Agent rotation via capitoltrades_api/src/user_agent.rs to avoid blocking

**Caching:**
- Query-based keying: full query state generates unique cache key
- TTL: 300 seconds (5 minutes)
- Storage: In-process DashMap, not persistent
- Invalidation: Automatic on TTL expiry, manual via client.clear_cache()
- Concurrency: DashMap provides lock-free concurrent access

**Output Formatting:**
- Four formats: table (tabled crate), JSON (serde_json), CSV (csv crate), markdown (tabled::Style::markdown)
- Structs: TradeRow, PoliticianRow, IssuerRow each derive Tabled + Serialize for dual-mode output
- Location: capitoltraders_cli/src/output.rs, separate print_*_table/json/csv/markdown functions

---

*Architecture analysis: 2026-02-05*
