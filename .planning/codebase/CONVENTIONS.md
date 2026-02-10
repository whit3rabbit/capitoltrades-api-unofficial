# Coding Conventions

**Analysis Date:** 2026-02-09

## Naming Patterns

**Files:**
- Module files: `lowercase_with_underscores.rs` (e.g., `validation.rs`, `cache.rs`, `error.rs`)
- Tests: Inline tests in module files using `#[cfg(test)] mod tests {}` or separate test files with `_tests.rs` suffix (e.g., `validation_tests.rs`)
- Test directories: `tests/` at crate root for integration tests

**Functions:**
- Lowercase with underscores: `validate_state()`, `trades_by_party()`, `build_trade_rows()`
- Verb-first for operations: `validate_*`, `build_*`, `load_*`, `format_*`, `count_*`, `update_*`, `upsert_*`
- Getters: simple name or `get_*` (e.g., `get_meta()`, `max_trade_pub_date()`)
- Builder patterns: `with_*` for fluent builders (e.g., `with_base_url()`, `with_party()`)

**Variables:**
- Lowercase with underscores: `last_request`, `max_retries`, `tx_date`, `issuer_name`
- Single-letter for loop indices only: `for trade in trades` (prefer named iterators)
- Field names match snake_case: `pub tx_id`, `pub politician_id`, `pub issuer_name`

**Types:**
- PascalCase for structs and enums: `Trade`, `TradeQuery`, `CapitolTradesError`, `OutputFormat`, `MemoryCache`
- Acronym-heavy names acceptable: `Db`, `DbTradeRow`, `DbTradeFilter`, `IssuerID`, `PoliticianID`

## Code Style

**Formatting:**
- Standard Rust formatting (no custom rustfmt.toml)
- Line width: Default (100 chars)
- No trailing commas in multi-line function signatures

**Linting:**
- Runs with `cargo clippy --workspace`
- All clippy warnings resolved (no warnings in codebase)
- No custom clippy configuration required

**Derives:**
- Common pattern: `#[derive(Copy, Clone)]` for enums/small types
- Serializable types: `#[derive(Serialize, Deserialize, Debug, Clone)]`
- Display trait frequently implemented manually for formatting (e.g., in `Party`, `TxType`, `Gender`)

## Import Organization

**Order:**
1. Standard library: `use std::...`
2. External crates: `use chrono::...`, `use serde::...`, etc.
3. Upstream crate types: `use capitoltrades_api::...`
4. Internal modules: `use crate::error::...`, `use crate::validation::...`
5. Type imports grouped by category at end

**Path Aliases:**
- No glob imports
- Specific imports preferred: `use chrono::{NaiveDate, Utc};`
- Grouped related types on one line: `use crate::types::{Trade, Politician};`

**Examples from codebase:**
```rust
// capitoltraders_lib/src/client.rs
use std::sync::Mutex;
use std::time::{Duration, Instant};

use capitoltrades_api::types::{
    IssuerDetail, PaginatedResponse, PoliticianDetail, Response, Trade,
};
use capitoltrades_api::{Client, IssuerQuery, PoliticianQuery, TradeQuery};
use rand::Rng;

use crate::cache::MemoryCache;
use crate::error::CapitolTradesError;

// capitoltraders_lib/src/validation.rs
use capitoltrades_api::types::{
    AssetType, Chamber, Gender, Label, MarketCap, Party, Sector, TradeSize, TxType,
};
use chrono::{NaiveDate, Utc};

use crate::error::CapitolTradesError;
```

## Error Handling

**Patterns:**
- Custom error enum `CapitolTradesError` with variants for each error type:
  - `Api(capitoltrades_api::Error)` - upstream API errors
  - `Cache(String)` - cache operation failures
  - `Serialization(serde_json::Error)` - JSON errors
  - `InvalidInput(String)` - validation failures
- Implement `Display`, `std::error::Error` traits with `source()` method
- Use `From<T>` impl for automatic error conversion (e.g., `From<serde_json::Error>`)
- DbError for database operations with `#[derive(thiserror::Error, Debug)]` and variants for each failure mode

**Result types:**
- Use `Result<T, CapitolTradesError>` alias in library
- Use `anyhow::Result<T>` in CLI for ergonomic error propagation
- Early returns with `?` operator preferred over `match` on Option/Result

**Examples:**
```rust
// capitoltraders_lib/src/error.rs - Display implementation
impl fmt::Display for CapitolTradesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(e) => write!(f, "API error: {}", e),
            Self::Cache(msg) => write!(f, "Cache error: {}", msg),
            // ...
        }
    }
}

// capitoltraders_lib/src/validation.rs - Early return pattern
pub fn validate_state(input: &str) -> Result<String, CapitolTradesError> {
    let upper = input.trim().to_uppercase();
    if VALID_STATES.contains(&upper.as_str()) {
        Ok(upper)
    } else {
        Err(CapitolTradesError::InvalidInput(format!("unknown state code '{}' ...", input)))
    }
}
```

## Logging

**Framework:** `tracing` crate (0.1)

**Patterns:**
- Initialized in CLI main: `tracing_subscriber::fmt()` with `EnvFilter`
- Default directive for crate: `"capitoltraders=info"`
- Environment variable control: `RUST_LOG`
- Used minimally; not found in library code paths

**Setup (from capitoltraders_cli/src/main.rs):**
```rust
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::from_default_env()
            .add_directive("capitoltraders=info".parse().unwrap()),
    )
    .with_target(false)
    .init();
```

## Comments

**When to Comment:**
- Module documentation: All modules start with `//!` doc comments explaining purpose
- Function documentation: Doc comments with `///` for public API functions
- Complex logic: Inline comments explain non-obvious algorithms or domain-specific rules
- Constants: Always documented (e.g., committee map, validation limits)

**Doc Comments:**
- Used extensively for public functions and types
- Examples from codebase:
```rust
/// Unique transaction identifier.
#[serde(rename = "_txId")]
pub tx_id: i64,

/// Trade size bracket used by the API for filtering by dollar amount range.
///
/// Each variant's discriminant (1-10) is the value sent to the API.
#[derive(Copy, Clone)]
pub enum TradeSize { ... }
```

## Function Design

**Size:**
- Prefer small, focused functions (50-150 lines typical)
- Large functions broken into helper functions (e.g., database query builders)

**Parameters:**
- Slices preferred over Vec: `fn build_trade_rows(trades: &[Trade])` not `&Vec<Trade>`
- Borrowed references for non-owned types: `&str` for strings, `&[T]` for collections
- Owned values for types that need to move: `String`, `Vec<T>` when returned from collection

**Return Values:**
- Explicit error types: `Result<T, ErrorType>` not `Option<Result<T>>`
- Unit return for side effects: `pub fn set(&self, key: String, value: String)` not `-> Result<()>`
- Iterators vs collections: Collectors preferred for simplicity, iterators for lazy evaluation rare

**Examples:**
```rust
// capitoltraders_lib/src/cache.rs
pub fn get(&self, key: &str) -> Option<String>
pub fn set(&self, key: String, value: String)

// capitoltraders_lib/src/analysis.rs - slices not Vec
pub fn trades_by_party(trades: &[Trade]) -> HashMap<String, Vec<&Trade>>
pub fn top_traded_issuers(trades: &[Trade], limit: usize) -> Vec<(String, usize)>
```

## Module Design

**Exports:**
- Public types and functions in `lib.rs` use `pub use` declarations
- Internal modules use `mod` without pub; only curated API exposed
- Example from `capitoltraders_lib/src/lib.rs`:
```rust
pub mod analysis;
pub mod cache;
pub mod client;

pub use client::CachedClient;
pub use db::{Db, DbError, DbIssuerFilter, DbIssuerRow, ...};
pub use error::CapitolTradesError;
```

**Barrel Files:**
- Used in multi-file modules: `capitoltraders_cli/src/commands/mod.rs` imports submodules

**File Layout:**
- Inline tests at bottom of module files in `#[cfg(test)] mod tests {}` block
- Fixture loading functions at top of test block (helper functions first)
- Test functions organized by feature with comment sections

## Type Conversions

**serde patterns:**
- `#[serde(rename_all = "camelCase")]` for API types (source is camelCase)
- `#[serde(rename = "_txId")]` for underscore-prefixed API fields
- Manual `Display` impls for CLI output (e.g., `Party::to_string()` returns "democrat" or "republican")
- Deserialization validates field presence, not custom validation on enum variants

**Builder Patterns:**
- Fluent builders with `mut self` and return `Self`:
```rust
impl TradeQuery {
    pub fn with_party(mut self, party: Party) -> Self {
        self.parties.push(party);
        self
    }
}

let query = TradeQuery::default()
    .with_party(Party::Democrat)
    .with_state("CA");
```

## Database & SQL

**Patterns:**
- Parameterized queries with `?1`, `?2` placeholders (rusqlite style)
- Transactions for multi-statement operations: `conn.transaction()?`
- `optional()` for nullable single-row queries: `.optional().map_err(DbError::from)`
- PRAGMA setup for durability: `PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;`

**Sentinel Pattern:**
- CASE expressions preserve enriched data during upserts:
```sql
asset_type = CASE WHEN excluded.asset_type != 'unknown' THEN excluded.asset_type ELSE assets.asset_type END
```

## Async Patterns

**Async Functions:**
- `async fn` directly in handlers, not `fn() -> impl Future`
- Tokio runtime: `#[tokio::main]` macro in CLI entry point
- Concurrent operations use `JoinSet` and `Semaphore` for backpressure control
- Channel-based producer-consumer for I/O-bound work (mpsc for data enrichment pipeline)

## Test Organization in Source Files

**Pattern:**
- Tests live at module bottom in `#[cfg(test)] mod tests {}`
- Fixture loaders before test functions
- Tests organized in logical groups with doc comments
- Integration tests in separate `tests/` directory with fixtures

---

*Convention analysis: 2026-02-09*
