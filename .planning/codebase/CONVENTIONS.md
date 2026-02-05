# Coding Conventions

**Analysis Date:** 2026-02-05

## Naming Patterns

**Files:**
- Module files: `lowercase_with_underscores` (e.g., `cache.rs`, `validation.rs`, `client.rs`)
- Test files: `{module}_tests` integrated inline via `#[cfg(test)]` modules in same file, OR separate integration test files in `tests/` directory named `{feature}.rs` (e.g., `query_builders.rs`, `client_integration.rs`)
- Binary: `capitoltraders` (no underscores in binary name, but crate names use underscores like `capitoltraders_cli`)

**Functions:**
- Public functions: `snake_case` (e.g., `validate_search`, `get_trades`, `with_issuer_id`)
- Builder methods: `with_{field}` for single items, `with_{field}s` for collections (e.g., `with_trade_size`, `with_trade_sizes`, `with_state`, `with_states`)
- Validator functions: `validate_{input_type}` (e.g., `validate_state`, `validate_party`, `validate_committee`)
- Test functions: descriptive names prefixed by `test_` or described action (e.g., `test_trades_by_party`, `cache_set_and_get`, `state_valid_uppercase`)

**Variables:**
- Local vars and parameters: `snake_case` (e.g., `cache_key`, `base_url`, `trade_sizes`)
- Constants: `UPPER_CASE` (e.g., `MAX_SEARCH_LENGTH`, `VALID_STATES`, `COMMITTEE_MAP`)
- Type-erased vars: use concrete names (e.g., `trades` not `items`, `issuers` not `data`)

**Types:**
- Structs: `PascalCase` (e.g., `TradeQuery`, `MemoryCache`, `CapitolTradesError`)
- Enums: `PascalCase` variants (e.g., `Party::Democrat`, `Chamber::House`)
- Traits: `PascalCase` (e.g., `Query`)

## Code Style

**Formatting:**
- No explicit formatter configured (uses default rustfmt)
- Line length: practical wrapping (see `sanitize_text`, `validate_committee` for examples of multi-line strings)
- Indentation: 4 spaces (Rust default)

**Linting:**
- Clippy enabled (standard `cargo clippy`)
- Upstream vendored crate warnings are intentionally left unfixed per Chesterton's Fence principle (documented in CLAUDE.md)
- Warnings about large enum variants addressed via boxing (see `Commands::Trades(Box<TradesArgs>)` in `capitoltraders_cli/src/main.rs:28`)

## Import Organization

**Order:**
1. Standard library imports (`use std::...`)
2. External crates (`use anyhow::`, `use chrono::`, `use serde::`, `use url::`)
3. Internal workspace crates (`use capitoltrades_api::`, `use capitoltraders_lib::`)
4. Relative imports (`use crate::...`)

Example from `capitoltraders_lib/src/validation.rs`:
```rust
use chrono::{NaiveDate, Utc};
use capitoltrades_api::types::{AssetType, Chamber, ...};
use crate::error::CapitolTradesError;
```

**Path Aliases:**
- No path aliases configured (uses full paths)
- Imports flatten type hierarchies: `use capitoltrades_api::types::{AssetType, Chamber, ...}` rather than `use capitoltrades_api::types` followed by `types::AssetType`

**Module Re-exports:**
- Core public API re-exported in `lib.rs` files. Example: `capitoltraders_lib/src/lib.rs` re-exports `CachedClient`, `Cache`, validation functions, analysis functions

## Error Handling

**Patterns:**
- Result-based errors for validation: all validators return `Result<T, CapitolTradesError>` (never panic on invalid input)
- Custom error enum `CapitolTradesError` wraps three categories: `Api`, `Cache`, `Serialization`, `InvalidInput` (see `capitoltraders_lib/src/error.rs:4-9`)
- Validation errors use descriptive messages with examples (e.g., "unknown state code 'XX'. Valid codes: AL, AK, ... DC, PR, VI (50 states + DC + territories)")
- Error propagation via `?` operator and `anyhow::Result<T>` in CLI layer
- Upstream API errors wrapped transparently via `From` impl: `CapitolTradesError::Api(capitoltrades_api::Error)`

**Error Display:**
```rust
impl fmt::Display for CapitolTradesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api(e) => write!(f, "API error: {}", e),
            Self::Cache(msg) => write!(f, "Cache error: {}", msg),
            Self::Serialization(e) => write!(f, "Serialization error: {}", e),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}
```

## Logging

**Framework:** `tracing` crate with `tracing-subscriber`

**Patterns:**
- Configured in CLI main via `tracing_subscriber::fmt()` with custom env filter
- Default level: `info` for `capitoltraders` modules, configurable via `RUST_LOG` env var (see `capitoltraders_cli/src/main.rs:37-43`)
- No explicit log statements in library code (library is log-agnostic)
- Async-aware: used with `#[tokio::main]` runtime

## Comments

**When to Comment:**
- Module-level doc comments for public APIs (sparse in codebase)
- Field-level comments for non-obvious types (e.g., COMMITTEE_MAP docstring explains code vs. full name)
- Complex validation logic documented with intent (e.g., `sanitize_text` explains filter behavior)

**JSDoc/TSDoc:**
- Not used (Rust codebase)
- Some doc comments on constants: `pub const COMMITTEE_MAP: &[(&str, &str)] = &[...]` with header explaining purpose

**Example:**
```rust
/// Committee code-to-name mapping. The API uses short abbreviation codes
/// (e.g., `hsag` for "House - Agriculture"). Users can pass either the code
/// or the full name; we always send the code to the API.
pub const COMMITTEE_MAP: &[(&str, &str)] = &[...]
```

## Function Design

**Size:**
- Most validators 5-20 lines (simple validation logic per function)
- Builder methods 1-3 lines (single field mutation + return self)
- Query encoding methods 30-50 lines (loop over collections, append URL params)
- Cache methods 10-15 lines (check TTL, deserialize/serialize)

**Parameters:**
- Owned types for inputs that need mutation (e.g., `mut self` in builders)
- References for read-only queries (e.g., `&str` in validators, `&[T]` for collections)
- No variadic arguments (uses `Vec` or array slices instead)
- Default values via `Option<T>` (see `QueryCommon.page_size`, `pub_date_relative`)

**Return Values:**
- `Result<T, E>` for fallible operations (validation, I/O)
- `Option<T>` for nullable values (cache hits, computed values)
- `Self` for builder chains
- Direct values for infallible operations (e.g., formatting, simple lookups)

**Example - Builder:**
```rust
pub fn with_issuer_id(mut self, issuer_id: IssuerID) -> Self {
    self.issuer_ids.push(issuer_id);
    self
}
```

**Example - Validator:**
```rust
pub fn validate_state(input: &str) -> Result<String, CapitolTradesError> {
    let upper = input.trim().to_uppercase();
    if VALID_STATES.contains(&upper.as_str()) {
        Ok(upper)
    } else {
        Err(CapitolTradesError::InvalidInput(format!(
            "unknown state code '{}'. Valid codes: ...",
            input
        )))
    }
}
```

## Module Design

**Exports:**
- Public functions and types exported at crate root via `pub fn`, `pub struct`, `pub enum`
- No wildcard exports (all re-exports explicit)
- Crate-level re-exports in `lib.rs` to create single public API surface (see `capitoltraders_lib/src/lib.rs`)

**Barrel Files:**
- Module-level re-exports in `mod.rs` (e.g., `capitoltrades_api/src/types/mod.rs` re-exports all type variants)
- Used to flatten type hierarchies for cleaner imports

**Example - Re-export Pattern:**
```rust
// capitoltraders_lib/src/lib.rs
pub use cache::MemoryCache;
pub use client::CachedClient;
pub use validation::*;
pub use analysis::*;
```

## Common Idioms

**Fluent Builder Pattern:**
```rust
TradeQuery::default()
    .with_issuer_id(100)
    .with_trade_size(TradeSize::From100Kto250K)
    .with_page(3)
    .add_to_url(&base_url)
```

**Iteration with URL Encoding:**
```rust
for issuer_id in self.issuer_ids.iter() {
    url.query_pairs_mut()
        .append_pair("issuer", &issuer_id.to_string());
}
```

**Fixture-Based Testing:**
- Test data loaded from `tests/fixtures/` JSON files (e.g., `trades.json`, `politicians.json`)
- Deserialization tests verify schema compatibility, integration tests verify client behavior with mocked server

**Validation Composability:**
- Validators accept `&str`, return typed results (Party, Gender, MarketCap, etc.)
- Multi-value CLI filters split on comma, validate individually, then add to query builder one by one

**Cache Key Generation:**
- Uses `format!("prefix:{:?}", struct_instance)` to leverage Debug impl for struct state
- Cache hit returns deserialized object, miss triggers API call and caches JSON response

---

*Convention analysis: 2026-02-05*
