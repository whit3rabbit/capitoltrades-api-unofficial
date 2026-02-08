# Coding Conventions

**Analysis Date:** 2026-02-07

## Naming Patterns

**Files:**
- Rust source files use snake_case: `cache.rs`, `validation.rs`, `xml_output.rs`
- Test files use module suffix: `cache.rs` contains `#[cfg(test)] mod tests`, inline with source
- Separate integration tests use `_tests.rs` suffix: `validation_tests.rs`, `output_tests.rs`, `xml_output_tests.rs`
- Binary crate uses `src/main.rs`, command modules use `src/commands/{command}.rs`

**Functions:**
- All lowercase with underscores: `validate_search()`, `trades_by_party()`, `format_value()`
- Builder methods use `with_*` prefix for query builders: `with_issuer_id()`, `with_page()`, `with_sort_by()`
- Getter/accessor methods use simple names: `get_user_agent()`, `get_trades()`
- Async functions use standard names without suffix: `run()`, `get_trades()` (suffix handled by `#[tokio::test]`)

**Variables:**
- Lowercase with underscores for local bindings: `base_url`, `mock_server`, `resp`, `json_str`
- Type instances typically use singular abbreviated forms: `trade`, `politician`, `issuer`, `writer`
- Iterators use explicit names: `for trade in trades`, `for party in self.parties`

**Types:**
- PascalCase for all types: `TradeQuery`, `MemoryCache`, `OutputFormat`, `CacheEntry`
- Enum variants use PascalCase: `Party::Democrat`, `OutputFormat::Json`, `Party::Other`
- Type aliases/newtype for IDs: `IssuerID`, `PoliticianID` (used in query builders)

**Constants:**
- SCREAMING_SNAKE_CASE: `MAX_SEARCH_LENGTH`, `MAX_COMMITTEE_LENGTH`, `VALID_STATES`, `COMMITTEE_MAP`

## Code Style

**Formatting:**
- Edition: 2021 (Rust stable)
- Line length: No explicit limit observed, but typically 100-120 chars
- Indentation: 4 spaces (standard Rust)
- No explicit `.rustfmt.toml` or `.prettierrc` in repo

**Linting:**
- Clippy enabled in CI; all warnings resolved
- Specific fixes applied to vendored crate:
  - `#[derive(...)]` used to auto-implement traits (e.g., `Clone`, `Copy`, `Display`)
  - Removed redundant lifetimes
  - Fixed `enum_variant_names` lints
  - Used `#[serde(rename)]` for field mapping instead of custom serialization
  - `Box<TradesArgs>` applied to `Commands::Trades` to avoid `large_enum_variant` warning

**Documentation:**
- Module-level `//!` comments at top of each file
- Doc comments `///` on public types and functions
- Inline comments `//` for complex logic, typically above the logic
- Example from `cache.rs`:
  ```rust
  //! In-memory TTL cache backed by `DashMap` for concurrent access.

  /// Thread-safe in-memory cache with time-to-live expiration.
  ///
  /// Entries are stored as serialized JSON strings. Expired entries are
  /// lazily evicted on the next `get` call for that key.
  pub struct MemoryCache { ... }
  ```

## Import Organization

**Order:**
1. Standard library imports: `use std::...`
2. External crates (alphabetical): `use dashmap::...`, `use serde::...`, `use url::Url`
3. Internal crate imports: `use crate::...`
4. Module declarations: `mod commands;`, `mod cache;`

**Path Aliases:**
- Not used; crate paths explicit throughout
- Fully qualified imports when disambiguating needed: `capitoltrades_api::types::Party`

**Re-exports:**
- Used sparingly in `lib.rs` files to expose public API
- Example from `capitoltrades_api/src/lib.rs`:
  ```rust
  pub use client::Client;
  pub use query::{Query, TradeQuery, PoliticianQuery, IssuerQuery, ...};
  ```

## Error Handling

**Patterns:**
- `Result<T, E>` used for fallible operations
- Custom error types in each crate:
  - `capitoltrades_api::Error` (thiserror derive): `RequestFailed`, `HttpStatus { status, body }`
  - `CapitolTradesError` enum (manual impl): `Api`, `Cache`, `Serialization`, `InvalidInput`
- Error conversion via `From` implementations: `impl From<capitoltrades_api::Error> for CapitolTradesError`
- Validation errors return `Err(CapitolTradesError::InvalidInput(msg))`
- Early returns with `?` operator in functions returning `Result`
- Example from `validation.rs`:
  ```rust
  pub fn validate_state(input: &str) -> Result<String, CapitolTradesError> {
      let upper = input.trim().to_uppercase();
      if VALID_STATES.contains(&upper.as_str()) {
          Ok(upper)
      } else {
          Err(CapitolTradesError::InvalidInput(format!("unknown state code '{}'", input)))
      }
  }
  ```
- In CLI layer, errors bubbled to `main()` via `anyhow::Result` (wraps any `Error` impl)
- `anyhow::bail!()` macro used for error early exit in CLI commands

## Logging

**Framework:** `tracing` crate (0.1)

**Patterns:**
- Log errors with context:
  ```rust
  tracing::error!("Invalid URL constructed: {}", e);
  tracing::error!("Failed to build HTTP client: {}", e);
  tracing::error!("Request failed with status {}: {}", status, snippet);
  ```
- Initialized in `main()` via `tracing_subscriber::fmt()`:
  ```rust
  tracing_subscriber::fmt()
      .with_env_filter(
          tracing_subscriber::EnvFilter::from_default_env()
              .add_directive("capitoltraders=info".parse().unwrap())
      )
      .with_target(false)
      .init();
  ```
- Logs to stderr; stdout reserved for data output (table/JSON/CSV/XML/Markdown)
- `RUST_LOG` environment variable controls verbosity

## Comments

**When to Comment:**
- Complex business logic (e.g., date-to-relative-days conversion)
- Non-obvious regex patterns or parsing logic
- Workarounds or temporary solutions
- API quirks (e.g., "API expects lowercase issuerState but uppercase politician state")

**JSDoc/TSDoc:**
- Not applicable (Rust uses doc comments)
- Doc comments use markdown: `/// This function does [thing](link)` and **bold** formatting rare
- Example from `cache.rs`:
  ```rust
  /// Returns the cached value for `key`, or `None` if missing or expired.
  pub fn get(&self, key: &str) -> Option<String> { ... }
  ```

## Function Design

**Size:**
- Typical functions 10-40 lines
- Large functions broken into helpers (e.g., `write_value()` in `xml_output.rs` recursively handles JSON values)
- Builder methods chain multiple `with_*` calls on immutable self

**Parameters:**
- Immutable references `&self` for methods on immutable structs
- Owned values for builders: `self` consumed, returns modified copy via `Default` and chaining
- Lifetime elision used where possible
- Slices for read-only collections: `&[Trade]` in analysis functions

**Return Values:**
- `Result<T, E>` for fallible ops; unwrap only in tests
- `Option<T>` for nullable values
- Direct values for simple getters: `pub fn total_volume(trades: &[Trade]) -> i64`
- Owned `Vec`, `HashMap`, `String` for aggregate results

**Async Functions:**
- Used for I/O: `async fn get_trades()`, `async fn run()` in command handlers
- Marked with `#[tokio::main]` on `main()`, `#[tokio::test]` on async tests
- Spawned with `.await` at call sites

## Module Design

**Exports:**
- Re-exported in `lib.rs` for public API: `pub use client::Client;`
- Lib crate root defines what's public to consumers
- Private modules (`capitoltraders_lib/src/cache.rs`) have internal tests via `#[cfg(test)] mod tests`

**Barrel Files:**
- `types/mod.rs` re-exports all type modules: `pub use trade::*;`, `pub use politician::*;`
- `query/mod.rs` re-exports all query builders
- Simplifies imports: `use capitoltrades_api::types::{Trade, Party, ...};`

**Test Module Organization:**
- Unit tests co-located with source: `#[cfg(test)] mod tests { ... }`
- Integration tests in separate `tests/` directory for cross-crate testing
- Test fixtures in `tests/fixtures/` (JSON payloads)
- Schema tests in `capitoltraders_cli/tests/schema_validation.rs` (separate integration suite)

## Struct Field Access

**Visibility:**
- Private by default; pub fields on public structs when representing data
- Getters used sparingly (none in vendored crate query builders)
- Internal fields in `CacheEntry`, `MemoryCache`, etc. private with accessor methods

**Derive Macros:**
- Standard derives: `Default`, `Clone`, `Debug`, `Copy` where applicable
- Serde derives: `#[derive(Serialize, Deserialize)]` on data types
- Tabled derives: `#[derive(Tabled)]` on row types for table output
- With field-level `#[serde(rename)]`, `#[tabled(rename)]` for API/display mapping

## Pattern Matching

- Used in error handling: `match result { Ok(x) => ..., Err(e) => ... }`
- Used in query building for `Option` fields: `if let Some(x) = &self.field { ... }`
- Destructuring in for-each patterns: `for (key, val) in map { ... }`

---

*Convention analysis: 2026-02-07*
