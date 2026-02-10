# Testing Patterns

**Analysis Date:** 2026-02-09

## Test Framework

**Runner:**
- Native Rust test harness (no external test runner)
- No explicit test framework config file

**Run Commands:**
```bash
cargo test --workspace              # Run all 294 tests
cargo test -p capitoltraders_lib    # Run lib tests only
cargo test -p capitoltrades_api     # Run API tests only
cargo test validation               # Filter by test name pattern
cargo test --lib                    # Run unit tests only (exclude integration tests)
```

**Test Count:**
- 294 total tests across three crates
- capitoltraders_cli: 57 tests (output, XML, circuit breaker)
- capitoltraders_lib: 174 tests (validation, cache, DB, scraping, analysis)
- capitoltrades_api: 3 snapshot tests (query builders)
- integration tests: 51 tests (client integration, deserialization, query builders, schema validation)

## Test File Organization

**Location:**
- Inline unit tests: Bottom of each module file in `#[cfg(test)] mod tests {}`
- Integration tests: `capitoltrades_api/tests/` directory
- Schema validation: `capitoltraders_cli/tests/`

**Naming:**
- Test functions: `test_<feature>_<scenario>()` or `<scenario>()` for simple cases
- Test modules: `#[cfg(test)] mod tests { ... }`
- Fixture files: `tests/fixtures/<data-type>.json`

**Structure:**
```
capitoltrades_api/
├── src/
│   ├── lib.rs
│   ├── client.rs
│   └── query/
│       ├── trade.rs
│       │   └── #[cfg(test)] mod tests {}  (query builder tests)
│       ├── politician.rs
│       └── issuer.rs
└── tests/
    ├── client_integration.rs        (8 tests with wiremock)
    ├── deserialization.rs           (7 tests with JSON fixtures)
    ├── query_builders.rs            (36 tests with snapshot assertions)
    └── fixtures/
        ├── trades.json
        ├── politicians.json
        └── issuers.json

capitoltraders_lib/
├── src/
│   ├── validation.rs
│   │   └── validation_tests.rs      (83 validation tests)
│   ├── cache.rs
│   │   └── #[cfg(test)] mod tests {} (5 cache tests)
│   ├── db.rs
│   │   └── #[cfg(test)] mod tests {} (65 DB tests)
│   └── analysis.rs
│       └── #[cfg(test)] mod tests {} (5 analysis tests)
└── tests/
    (none - all tests inline)

capitoltraders_cli/
├── src/
│   ├── output.rs
│   │   └── output_tests.rs          (43 output formatting tests)
│   └── xml_output.rs
│       └── xml_output_tests.rs      (14 XML tests)
└── tests/
    └── schema_validation.rs         (9 schema validation tests)
```

## Test Structure

**Suite Organization:**
All tests follow this pattern:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Fixture loaders first
    fn load_fixture(name: &str) -> String {
        std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
    }

    // Test functions organized by feature with comment sections
    // -- Feature section --

    #[test]
    fn test_specific_case() {
        // Arrange
        let input = ...;

        // Act
        let result = function_under_test(input);

        // Assert
        assert_eq!(result, expected);
    }
}
```

**Patterns:**
- `use super::*;` to access module internals
- Fixture loading at module level for reuse
- Simple names for assertions: `assert!(result.is_ok())`, `assert_eq!(x, y)`, `assert!(matches!(x, Pattern))`
- No test setup/teardown; in-memory databases for isolation

**Example Test Organization (from capitoltraders_lib/src/validation_tests.rs):**
```rust
// -- State validation --
#[test]
fn state_valid_uppercase() { ... }

#[test]
fn state_valid_lowercase() { ... }

// ... more state tests ...

// -- Party validation --
#[test]
fn party_democrat() { ... }
```

## Mocking

**Framework:** `wiremock` 0.6 (used for HTTP mocking in integration tests)

**Patterns:**
- Mock HTTP server for client integration tests
- Fixture JSON files for response bodies
- Matchers for HTTP method, path, query parameters

**Example (from capitoltrades_api/tests/client_integration.rs):**
```rust
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
- HTTP responses for client testing (wiremock)
- Fixture JSON for deserialization tests

**What NOT to Mock:**
- Database operations (use in-memory SQLite)
- Validation logic (call directly)
- Type conversions (test via serde)

## Fixtures and Factories

**Test Data:**
Fixture JSON files at `capitoltrades_api/tests/fixtures/`:
- `trades.json` - Trade response with metadata
- `politicians.json` - Politician detail response
- `issuers.json` - Issuer detail response
- `trades_minimal.json` - Empty trades response

**Location:**
- Fixtures: `tests/fixtures/*.json`
- Loaded via `std::fs::read_to_string()` in fixture loaders
- Can be loaded with `include_str!()` for embedded fixtures

**Database Factories:**
- In-memory SQLite: `Db::open_in_memory()?` for test isolation
- No fixtures needed; tests build data programmatically

**Example (from capitoltraders_lib/src/db.rs):**
```rust
#[cfg(test)]
pub fn open_in_memory() -> Result<Self, DbError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON; ...")?;
    Ok(Self { conn })
}

#[test]
fn query_trades_with_party_filter() {
    let db = test_db();  // Helper function
    // Insert test data, then query
    let rows = db.query_trades(&filter)?;
    assert!(!rows.is_empty());
}
```

## Coverage

**Requirements:** No explicit target enforced

**Current Coverage:**
- Unit tests: ~294 tests across all modules
- Integration tests: 51 tests (wiremock, deserialization, schema validation)
- High coverage areas: validation (83 tests), database operations (65 tests), output formatting (43 tests)
- Test distribution:
  - API layer: Deserialization, query builders, client integration
  - Library layer: Validation, caching, database, analysis
  - CLI layer: Output formatting, XML serialization, schema validation

## Test Types

**Unit Tests:**
- Scope: Individual functions and types in isolation
- Approach: Direct function calls with assertions
- Tools: Standard `assert_*` macros
- Examples:
  - `validate_state("CA")` - input validation
  - `format_value(15_000_000)` - formatting
  - `cache.set() / cache.get()` - cache operations

**Integration Tests:**
- Scope: HTTP client with mocked server responses
- Approach: Mock server + client + query builder
- Tools: wiremock for mocking, serde_json for fixtures
- Examples:
  - `get_trades_with_filters_sends_query_params()` - query param encoding
  - `get_trades_server_error()` - error handling
  - Schema validation against JSON fixtures

**Schema Validation Tests (from capitoltraders_cli/tests/schema_validation.rs):**
- Scope: Fixture JSON conforms to published schemas
- Approach: jsonschema library (draft202012 module)
- Examples:
  - `test_trades_fixture_conforms_to_schema()`
  - `test_trade_schema_rejects_missing_required_field()`

**Database Tests:**
- Scope: SQLite operations in isolation
- Approach: In-memory database, direct SQL verification
- Examples:
  - `test_upsert_trades()` - insert/update logic
  - `test_enrichment_queue_batch_size_limiting()` - query limits
  - `test_update_trade_detail_labels()` - partial updates

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn test_async_operation() {
    let client = ScrapeClient::new();
    let result = client.trades(1).await;
    assert!(result.is_ok());
}
```

**Error Testing (Validation):**
```rust
#[test]
fn state_invalid() {
    assert!(validate_state("XX").is_err());
}

// Or with pattern matching
#[test]
fn party_invalid() {
    assert!(matches!(
        validate_party("libertarian"),
        Err(CapitolTradesError::InvalidInput(_))
    ));
}
```

**Option/Result Assertions:**
```rust
#[test]
fn test_optional_value() {
    assert_eq!(cache.get("key"), Some("value".to_string()));
    assert_eq!(cache.get("missing"), None);
}

#[test]
fn test_result_type() {
    let result = validate_search("text");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "text");
}
```

**Fixture-Based Scraping Tests:**
```rust
const TRADE_DETAIL_FIXTURE: &str = include_str!("../tests/fixtures/trade_detail.html");

#[test]
fn extract_trade_detail_from_fixture() {
    let rsc = extract_rsc_payload(TRADE_DETAIL_FIXTURE).unwrap();
    let detail = extract_trade_detail(&rsc).unwrap();
    assert_eq!(detail.asset_type.as_deref(), Some("stock"));
}
```

**Deserialization Tests:**
```rust
#[test]
fn deserialize_trades_full() {
    let json = load_fixture("trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].tx_id, 12345);
    assert_eq!(resp.meta.paging.page, 1);
}

#[test]
fn deserialize_malformed_json_returns_error() {
    let bad_json = r#"{"data": not valid json}"#;
    let result = serde_json::from_str::<PaginatedResponse<Trade>>(bad_json);
    assert!(result.is_err());
}
```

**Output Formatting Tests:**
```rust
#[test]
fn test_format_value_millions() {
    assert_eq!(format_value(15_000_000), "$15.0M");
}

#[test]
fn test_build_trade_rows_mapping() {
    let trades = load_trades_fixture();
    let rows = build_trade_rows(&trades);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].tx_date, "2024-03-01");
    assert_eq!(rows[0].politician, "Jane Smith");
}
```

**Database Filter Tests (Dynamic Query Building):**
```rust
#[test]
fn query_trades_with_party_filter() {
    let db = test_db();
    let filter = DbTradeFilter { party: Some("Democrat".into()), ..Default::default() };
    let rows = db.query_trades(&filter).unwrap();
    assert!(rows.iter().all(|r| r.party == "Democrat"));
}
```

**XML Output Tests:**
```rust
#[test]
fn test_trade_xml_wellformed() {
    let xml_string = generate_trade_xml(&trades);
    let result = quick_xml::de::from_str::<Root>(&xml_string);
    assert!(result.is_ok());
}

#[test]
fn test_xml_special_chars_escaped() {
    let trades = vec![Trade { ... }];
    let xml = generate_trade_xml(&trades);
    assert!(xml.contains("&amp;"));  // & escaped as &amp;
}
```

## Unwrap Usage

**Acceptable in Tests:**
- Fixture loading: `std::fs::read_to_string(...).unwrap()`
- Test setup: `.unwrap()` to assert preconditions
- Fixture parsing: `serde_json::from_str(...).unwrap()`

**Not Acceptable in Production Code:**
- All `unwrap()` calls in `src/` code prohibited (use `?` or `match`)
- Exception: Internal invariants already verified (very rare)

## Notes on Test Inventory

Test counts verified by `cargo test --workspace`:
- capitoltraders_cli: 57 tests (27 output tests, 20 XML tests, 6 circuit breaker, 4 misc)
- capitoltraders_lib: 174 tests (83 validation, 35 DB enrichment, 20 query operations, 15 analysis/cache/scraping)
- capitoltrades_api: 3 inline query builder snapshot tests
- Integration suite: 51 tests (8 client, 7 deserialization, 36 query builders, 9 schema validation)
- **Total: 294 tests**

---

*Testing analysis: 2026-02-09*
