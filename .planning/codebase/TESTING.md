# Testing Patterns

**Analysis Date:** 2026-02-05

## Test Framework

**Runner:**
- `cargo test` (built-in Rust test harness)
- Uses `#[test]` attribute for unit tests
- Uses `#[tokio::test]` for async integration tests

**Assertion Library:**
- Standard Rust assertions: `assert!`, `assert_eq!`, `assert_ne!`
- Pattern matching: `assert!(matches!(result, ExpectedVariant))`
- No external assertion library (unfancy assertions)

**Snapshot Testing:**
- `insta` crate for query builder snapshots (3 tests in `capitoltrades_api/src/query/`)
- Snapshot files: `snapshots/capitoltrades_api__query__{module}__tests__{test_name}.snap`

**Run Commands:**
```bash
cargo test --workspace              # Run all tests
cargo test --lib                    # Run lib/unit tests only
cargo test --test '*'               # Run integration tests only
cargo test -- --nocapture           # Show println! output
cargo test -- --test-threads=1      # Run sequentially (useful for timing-sensitive tests)
RUST_LOG=debug cargo test -- --nocapture  # Enable debug logging
```

## Test File Organization

**Location:**
- **Unit tests:** Inline in same module via `#[cfg(test)] mod tests { ... }` at end of file
- **Integration tests:** Separate files in `{crate}/tests/` directory
- **Fixtures:** JSON test data in `capitoltrades_api/tests/fixtures/` directory

**Naming:**
- Unit test files: none (inline only)
- Integration test files: descriptive names matching what they test (e.g., `query_builders.rs`, `client_integration.rs`, `deserialization.rs`)
- Test functions: descriptive action or state (e.g., `state_valid_uppercase`, `cache_set_and_get`, `deserialize_trades_full`)

**Structure:**
```
capitoltrades_api/
  src/
    query/
      trade.rs              # 1 unit test: test_trade_query
      politician.rs         # 1 unit test: test_politician_query
      issuer.rs             # 1 unit test: test_issuer_query (inline)
  tests/
    deserialization.rs      # 7 tests (JSON → type deserialization)
    query_builders.rs       # 36 tests (URL parameter encoding)
    client_integration.rs   # 8 tests (wiremock-mocked HTTP client)
    fixtures/
      trades.json
      trades_minimal.json
      politicians.json
      issuers.json

capitoltraders_lib/
  src/
    cache.rs                # 5 unit tests (set/get/expire/clear/overwrite)
    analysis.rs             # 5 unit tests (grouping/aggregation)
    validation.rs           # 70 unit tests (each validator + edge cases)
```

## Test Structure

**Suite Organization:**

Inline unit test module pattern:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        // arrange
        let input = "...";
        // act
        let result = function_under_test(input);
        // assert
        assert_eq!(result, expected);
    }
}
```

Integration test pattern (separate file):
```rust
// tests/query_builders.rs
use capitoltrades_api::{TradeQuery, Query};
use url::Url;

fn base_url() -> Url {
    Url::parse("https://example.com").unwrap()
}

#[test]
fn trade_query_with_issuer_ids_and_sizes() {
    let url = TradeQuery::default()
        .with_issuer_id(100)
        .with_issuer_id(200)
        .with_trade_size(TradeSize::From100Kto250K)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("issuer=100"));
    assert!(query.contains("issuer=200"));
}
```

**Patterns:**

- **Setup:** Create test inputs, parse fixtures, instantiate mocks
- **Teardown:** None typically (Rust cleans up via drop semantics)
- **Assertion:** Direct equality checks with `assert_eq!`, or pattern matching with `assert!(matches!(...))`
- **Fixtures:** Load from JSON files in `tests/fixtures/` directory via `std::fs::read_to_string`

## Mocking

**Framework:** `wiremock` 0.6 for HTTP mocking

**Patterns:**
```rust
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn get_trades_success() {
    let mock_server = MockServer::start().await;
    let body = load_fixture("trades.json");

    Mock::given(method("GET"))
        .and(path("/trades"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_ok());
}
```

**What to Mock:**
- HTTP client responses (via wiremock for integration tests)
- Timestamps in cache expiry tests (use `std::thread::sleep` for timing)

**What NOT to Mock:**
- Validation logic (test directly, no mocking needed)
- Cache internals (test via public API)
- Query builders (test URL output directly)
- JSON deserialization (test with real fixtures)
- Error types (test via pattern matching on Result)

## Fixtures and Factories

**Test Data:**

JSON fixtures stored in `capitoltrades_api/tests/fixtures/`:
- `trades.json` - Full trade response with pagination metadata
- `trades_minimal.json` - Empty trade response
- `politicians.json` - Multiple politician records with stats
- `issuers.json` - Issuer detail with performance data

Loading pattern:
```rust
fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

#[test]
fn deserialize_trades_full() {
    let json = load_fixture("trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.data.len(), 1);
}
```

**Builders for Complex Types:**

For analysis tests, trades loaded via fixture and reused:
```rust
fn load_fixture_trades() -> Vec<Trade> {
    let json = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../capitoltrades_api/tests/fixtures/trades.json")
    ).unwrap();
    let resp: capitoltrades_api::types::PaginatedResponse<Trade> =
        serde_json::from_str(&json).unwrap();
    resp.data
}

#[test]
fn test_trades_by_party() {
    let trades = load_fixture_trades();
    let by_party = trades_by_party(&trades);
    assert!(by_party.contains_key("democrat"));
}
```

**Location:**
- Fixtures: `capitoltrades_api/tests/fixtures/` (shared across crates via absolute path)
- No factory functions (builders preferred, see `TradeQuery::default().with_issuer_id(100)`)

## Coverage

**Requirements:** None explicitly enforced

**View Coverage:**
```bash
# Requires tarpaulin or llvm-cov
cargo tarpaulin --workspace
# or
cargo llvm-cov
```

**Current Status:** Comprehensive test coverage for:
- All validation functions (70 tests covering success, invalid, edge cases)
- Query builders (36 tests covering filter combinations, URL encoding, pagination)
- Cache behavior (5 tests covering set/get/expiry/clear)
- Analysis functions (5 tests with fixture data)
- Deserialization (7 tests covering success and malformed JSON)
- HTTP client (8 tests covering success/error/malformed responses)

## Test Types

**Unit Tests:**
- Scope: Single function or small module
- Location: Inline `#[cfg(test)] mod tests` at end of module
- Examples:
  - `validate_state()` tests (15 tests covering uppercase/lowercase/territories/invalid)
  - `cache.set() / cache.get()` tests (5 tests)
  - Query builder tests (36 tests verifying URL parameters)
- Run: `cargo test --lib`

**Integration Tests:**
- Scope: Multiple components working together
- Location: Separate files in `tests/` directory
- Examples:
  - `client_integration.rs` - HTTP client with wiremock server (8 tests)
  - `query_builders.rs` - Query structs encoding to URLs (36 tests)
  - `deserialization.rs` - Upstream types deserializing from JSON (7 tests)
- Run: `cargo test --test '*'`

**E2E Tests:**
- Status: Not implemented
- CLI binary testable via `cargo run -p capitoltraders_cli -- ...` but no automated end-to-end tests

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn get_trades_success() {
    let mock_server = MockServer::start().await;
    // ... setup mocks ...
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_ok());
}
```

**Error Testing:**
```rust
#[test]
fn state_invalid() {
    assert!(validate_state("XX").is_err());
}

#[test]
fn deserialize_malformed_json_returns_error() {
    let bad_json = r#"{"data": not valid json}"#;
    let result = serde_json::from_str::<PaginatedResponse<Trade>>(bad_json);
    assert!(result.is_err());
}
```

**Boundary Testing:**
```rust
#[test]
fn page_size_valid() {
    assert_eq!(validate_page_size(1).unwrap(), 1);
    assert_eq!(validate_page_size(100).unwrap(), 100);
}

#[test]
fn page_size_zero_rejected() {
    assert!(validate_page_size(0).is_err());
}

#[test]
fn page_size_over_100_rejected() {
    assert!(validate_page_size(101).is_err());
}
```

**Fixture-Based Assertion:**
```rust
#[test]
fn deserialize_trades_full() {
    let json = load_fixture("trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();

    let trade = &resp.data[0];
    assert_eq!(trade.tx_id, 12345);
    assert_eq!(trade.politician_id, "P000197");
    assert_eq!(trade.issuer_id, 5678);
    assert_eq!(trade.value, 50000);
}
```

## Test Statistics

**Total Tests:** 128 (run `cargo test -- --list` for full inventory)

| Crate | Location | Count | Type |
|-------|----------|-------|------|
| capitoltrades_api | src/query/*.rs | 3 | Snapshot/inline |
| capitoltrades_api | tests/deserialization.rs | 7 | Integration |
| capitoltrades_api | tests/query_builders.rs | 36 | Integration |
| capitoltrades_api | tests/client_integration.rs | 8 | Integration (wiremock) |
| capitoltraders_lib | src/cache.rs | 5 | Unit |
| capitoltraders_lib | src/analysis.rs | 5 | Unit |
| capitoltraders_lib | src/validation.rs | 70 | Unit |

## Debugging Tests

**Enable Logging:**
```bash
RUST_LOG=debug cargo test -- --nocapture
```

**Run Single Test:**
```bash
cargo test state_valid_uppercase -- --nocapture
```

**Run Specific Test Module:**
```bash
cargo test capitoltraders_lib::validation::tests --lib
```

**Run Tests with Output:**
```bash
cargo test -- --nocapture --test-threads=1
```

---

*Testing analysis: 2026-02-05*
