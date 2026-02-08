# Testing Patterns

**Analysis Date:** 2026-02-07

## Test Framework

**Runner:**
- Cargo test (`cargo test --workspace`)
- No explicit `Makefile` or test runner config
- Runs all tests via Cargo's built-in test harness

**Assertion Library:**
- Standard Rust `assert!()`, `assert_eq!()`, `assert!()` macros (no external crate)
- Pattern matching for assertions: `assert!(matches!(value, Pattern))`

**Run Commands:**
```bash
cargo test --workspace              # Run all tests (186 unit + 8 async)
cargo test --workspace -- --test-threads=1  # Sequential execution
cargo test -p capitoltrades_api     # Run tests for specific crate
cargo test --test query_builders    # Run specific integration test file
cargo test validation_tests         # Run validation tests
RUST_LOG=debug cargo test           # With logging
```

## Test File Organization

**Location:**
- **Unit tests:** Co-located with source in `#[cfg(test)] mod tests` blocks
- **Integration tests:** Separate files in `tests/` directory at crate root
- **Inline module tests:** In source files: `src/cache.rs`, `src/validation.rs`, `src/analysis.rs`
- **Separate test modules:** `src/output_tests.rs`, `src/xml_output_tests.rs`, `src/validation_tests.rs`

**Naming:**
- Test functions: `test_<what_is_tested>` or `<feature>_<case>` pattern
- Examples: `test_format_value_millions()`, `party_democrat()`, `state_valid_uppercase()`
- Fixtures: `tests/fixtures/{name}.json` (e.g., `trades.json`, `politicians.json`)

**Structure:**
```
capitoltrades_api/
  tests/
    query_builders.rs        # 36 tests for TradeQuery, PoliticianQuery, IssuerQuery builders
    client_integration.rs    # 8 wiremock integration tests
    deserialization.rs       # 7 JSON fixture deserialization tests
    fixtures/                # JSON test data
      trades.json
      politicians.json
      issuers.json
capitoltraders_lib/
  src/
    cache.rs                 # 5 cache unit tests (inline)
    analysis.rs              # 5 analysis unit tests (inline)
    validation.rs            # 83 validation unit tests (inline)
    validation_tests.rs      # 83 validation tests (separate module)
capitoltraders_cli/
  src/
    output.rs                # 20 output unit tests (inline)
    output_tests.rs          # 20 output formatting tests (separate)
    xml_output.rs            # 12 XML output tests (inline)
    xml_output_tests.rs      # 12 XML output tests (separate)
  tests/
    schema_validation.rs     # 9 JSON Schema validation tests
```

## Test Structure

**Suite Organization:**

Tests are organized by feature area with clear naming:

```rust
// From validation_tests.rs: Grouped by function
// -- State validation --
#[test]
fn state_valid_uppercase() { ... }

#[test]
fn state_valid_lowercase() { ... }

#[test]
fn state_invalid() { ... }

// -- Party validation --
#[test]
fn party_democrat() { ... }

#[test]
fn party_shorthand_d() { ... }
```

**Setup Pattern:**

Fixtures loaded via helper functions:

```rust
// From output_tests.rs
fn load_trades_fixture() -> Vec<Trade> {
    let json_str = include_str!("../../capitoltrades_api/tests/fixtures/trades.json");
    let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
    serde_json::from_value(resp["data"].clone()).unwrap()
}

// Used in test
#[test]
fn test_format_value_millions() {
    assert_eq!(format_value(15_000_000), "$15.0M");
}
```

**Teardown Pattern:**
- No explicit teardown; tests are isolated
- `DashMap` cache cleared in tests via `cache.clear()`
- Mock servers (wiremock) destroyed at end of scope
- Fixtures are static (JSON strings embedded or read from disk)

**Assertion Pattern:**

Direct assertions with clear expected values:

```rust
// From query_builders.rs
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
    assert!(query.contains("tradeSize=5"));
}
```

## Mocking

**Framework:** `wiremock` 0.6 (HTTP mocking)

**Patterns:**

Wiremock used to mock HTTP API responses:

```rust
// From client_integration.rs
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
- External HTTP APIs: Use wiremock to mock CapitolTrades BFF API
- Database responses (if tested): Could use wiremock or in-memory substitutes
- Time/clocks: Use `std::thread::sleep()` for cache expiration tests

**What NOT to Mock:**
- Internal business logic: Test directly (validation, analysis, formatting)
- Serialization: Use real `serde_json` or `quick_xml`
- Query builders: Test with real URL construction

## Fixtures and Factories

**Test Data:**

Fixtures are JSON files committed to repo:

```
capitoltrades_api/tests/fixtures/
  trades.json         # Single trade with politician/issuer
  politicians.json    # Two politicians with stats
  issuers.json        # Single issuer with performance data
```

Loaded via:
- `include_str!()` macro for compile-time embedding (output tests)
- `std::fs::read_to_string()` for runtime loading (integration tests)

**Example from deserialization tests:**

```rust
#[test]
fn test_trade_deserialization() {
    let json_str = include_str!("fixtures/trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(json_str).unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].tx_id, 12345);
}
```

**Factories:**
- Not used; fixtures directly deserialized
- Builder pattern used where needed: `TradeQuery::default().with_issuer_id(100)`

**Location:**
- Fixtures: `capitoltrades_api/tests/fixtures/` (shared across crates via `include_str!()`)
- Test helpers: Inline in test files as `fn load_fixture()`, `fn base_url()`, etc.

## Coverage

**Requirements:**
- No explicit coverage target enforced in CI
- Coverage reported locally via `tarpaulin` (not configured in repo)

**View Coverage:**
```bash
# Install
cargo install cargo-tarpaulin

# Generate HTML report
cargo tarpaulin --workspace --html --output-dir coverage

# Terminal report
cargo tarpaulin --workspace
```

**Test Count:** 194 total tests across all crates
- 3 insta snapshot tests (upstream)
- 7 deserialization tests
- 36 query builder tests
- 8 wiremock integration tests
- 5 cache unit tests
- 5 analysis unit tests
- 83 validation unit tests
- 12 XML output tests
- 20 output unit tests
- 9 JSON Schema validation tests
- 6 other (insta, edge cases)

## Test Types

**Unit Tests:**

Test individual functions in isolation. Examples:

```rust
// validation.rs: 83 tests
#[test]
fn state_valid_uppercase() { assert_eq!(validate_state("CA").unwrap(), "CA"); }

// cache.rs: 5 tests
#[test]
fn cache_expiration() {
    let cache = MemoryCache::new(Duration::from_millis(1));
    cache.set("key1".to_string(), "value1".to_string());
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(cache.get("key1"), None);
}

// analysis.rs: 5 tests
#[test]
fn test_trades_by_party() {
    let trades = load_fixture_trades();
    let by_party = trades_by_party(&trades);
    assert!(by_party.contains_key("democrat"));
}
```

**Integration Tests:**

Test component interaction via API mocking:

```rust
// client_integration.rs: 8 tests
#[tokio::test]
async fn get_trades_server_error() {
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/trades"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_err());
}
```

**E2E/Schema Tests:**

Validate entire output structures against schemas:

```rust
// schema_validation.rs: 9 tests
#[test]
fn test_trades_fixture_conforms_to_schema() {
    let fixture = load_fixture("trades.json");
    let schema = load_schema("trade.schema.json");
    let data = extract_data_array(&fixture);

    let validator = jsonschema::draft202012::new(&schema).expect("trade schema compiles");
    let result = validator.validate(&data);
    if let Err(e) = &result {
        panic!("trades fixture failed validation: {e}");
    }
}
```

**Not used:**
- Benchmark tests (`#[bench]`)
- Documentation tests

## Common Patterns

**Async Testing:**

Used for I/O-bound operations (HTTP requests):

```rust
#[tokio::test]
async fn get_trades_success() {
    let mock_server = MockServer::start().await;
    // ... setup ...
    let result = client.get_trades(&query).await;
    assert!(result.is_ok());
}
```

**Error Testing:**

Validate error cases explicitly:

```rust
#[test]
fn state_invalid() {
    assert!(validate_state("XX").is_err());
}

#[tokio::test]
async fn get_trades_server_error() {
    // ... mock 500 response ...
    let result = client.get_trades(&query).await;
    assert!(result.is_err());
}
```

**Snapshot Testing:**

Used minimally (3 insta tests in upstream):

```rust
// capitoltrades_api/src/query/trade.rs
#[test]
fn snapshot_trade_query_url() {
    let url = TradeQuery::default().with_search("apple").add_to_url(&base_url());
    insta::assert_yaml_snapshot!(url.query().unwrap());
}
```

**Parametric Testing:**

Not explicitly used; multiple assertions in single test:

```rust
#[test]
fn party_shorthand_variants() {
    assert!(matches!(validate_party("d").unwrap(), Party::Democrat));
    assert!(matches!(validate_party("r").unwrap(), Party::Republican));
}
```

**Deserialization Testing:**

Verify JSON fixtures parse correctly:

```rust
#[test]
fn test_trade_deserialization() {
    let json = include_str!("fixtures/trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(json).unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].tx_id, 12345);
}
```

## Test-Specific Dependencies

**dev-dependencies:**

| Crate | Version | Purpose |
|-------|---------|---------|
| `insta` | 1 | Snapshot testing for query builders |
| `wiremock` | 0.6 | HTTP mocking for integration tests |
| `tokio` | 1 (features: rt-multi-thread, macros) | Async test runtime |
| `jsonschema` | 0.29 | JSON Schema validation in tests |

All production dependencies also available to tests (serde_json, chrono, etc.).

---

*Testing analysis: 2026-02-07*
