# Capitol Traders - Agent Development Guide

This guide provides concrete patterns and conventions for agentic coding in this Rust workspace. For project structure overview, see the main documentation.

## Build & Development Commands

```bash
# Primary workspace commands
cargo check --workspace          # Fast compilation check
cargo test --workspace           # Run all 513 tests
cargo clippy --workspace         # Lint with all clippy rules
cargo run -p capitoltraders_cli -- trades --help  # Test CLI

# Single test execution patterns
cargo test -p capitoltrades_api deserialization    # By crate and test name
cargo test -p capitoltraders_lib validation::state_valid  # By module and specific test
cargo test validation -- --nocapture               # Show print output in tests
cargo test --workspace cache::tests::cache_set_and_get  # Full path to test
```

## Code Style & Formatting

### Import Organization
```rust
// Order: std, external crates, internal modules (grouped by category)
use std::time::Duration;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    query::{IssuerQuery, PoliticianQuery},
    types::{Trade, Politician},
    client::ScrapeClient,
};
```

### Type Patterns & Serialization
```rust
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum AssetType {
    #[serde(rename = "stock")]
    Stock,
    #[serde(rename = "stock-option")]
    StockOption,
}

// Display implementations for CLI-facing types
impl std::fmt::Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stock => write!(f, "stock"),
            Self::StockOption => write!(f, "stock-option"),
        }
    }
}
```

### Error Handling Patterns
```rust
#[derive(Error, Debug)]
pub enum CapitolTradesError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Upstream API error")]
    Upstream(#[source] capitoltrades_api::Error),
    #[error("Cache error: {0}")]
    Cache(String),
}

// Use thiserror for custom errors, anyhow::Result for application logic
pub type Result<T> = anyhow::Result<T, CapitolTradesError>;
```

### Async & CLI Patterns
```rust
// CLI command runners - two paths: scrape and DB
pub async fn run(args: &TradesArgs, scraper: &ScrapeClient, format: &OutputFormat) -> Result<()> {
    // Scrape mode: fetch from capitoltrades.com
}

pub async fn run_db(args: &TradesArgs, db_path: &Path, format: &OutputFormat) -> Result<()> {
    // DB mode: read from local SQLite
}

// Main dispatches based on --db flag presence
match &args.db {
    Some(path) => run_db(&args, path, &format).await,
    None => run(&args, &scraper, &format).await,
}
```

### Query Builder Patterns
```rust
impl TradeQuery {
    pub fn with_party(mut self, party: Party) -> Self {
        self.parties.push(party);
        self
    }

    pub fn with_page(mut self, page: i64) -> Self {
        self.page = Some(page);
        self
    }
}

// Chain fluent methods returning Self where Self: Sized
let query = TradeQuery::default()
    .with_party(Party::Democrat)
    .with_state("CA")
    .with_page(1);
```

## Testing Conventions

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_edge_case() {
        // High unwrap usage is acceptable in tests
        let result = validate_state("CA").unwrap();
        assert_eq!(result, "CA");
    }

    #[tokio::test]
    async fn test_async_function() {
        let scraper = ScrapeClient::new();
        let result = scraper.trades(1).await.unwrap();
        assert!(!result.is_empty());
    }
}
```

### Integration Tests
```rust
// Use wiremock for HTTP integration tests
#[tokio::test]
async fn get_trades_with_filters_sends_query_params() {
    let mock_server = wiremock::MockServer::start().await;
    mock_server.register(
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/trades"))
            .and(wiremock::matchers::query_param("party", "d"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(fixture)),
    ).await;

    let client = Client::with_base_url(mock_server.uri());
    let query = TradeQuery::default().with_party(Party::Democrat);
    let result = client.get_trades(&query).await.unwrap();

    assert!(result.len() > 0);
}
```

### Validation Testing
```rust
// Test all validation edge cases (see validation.rs for many examples)
#[test]
fn state_invalid() {
    assert!(matches!(
        validate_state("XX"),
        Err(CapitolTradesError::InvalidInput(_))
    ));
}

#[test]
fn state_valid_lowercase() {
    assert_eq!(validate_state("ca").unwrap(), "CA");
}
```

### Fixture-based Scrape Testing
```rust
// HTML fixtures in tests/fixtures/, loaded via include_str!
const TRADE_DETAIL_FIXTURE: &str = include_str!("../tests/fixtures/trade_detail.html");

#[test]
fn extract_trade_detail_from_fixture() {
    let rsc = extract_rsc_payload(TRADE_DETAIL_FIXTURE).unwrap();
    let detail = extract_trade_detail(&rsc).unwrap();
    assert_eq!(detail.asset_type.as_deref(), Some("stock"));
}
```

### DB Testing
```rust
// In-memory SQLite for test isolation
fn test_db() -> Db {
    Db::open(":memory:").unwrap()
}

#[test]
fn query_trades_with_party_filter() {
    let db = test_db();
    // Insert test data, then query with filter
    let filter = DbTradeFilter { party: Some("Democrat".into()), ..Default::default() };
    let rows = db.query_trades(&filter).unwrap();
    assert!(rows.iter().all(|r| r.party == "Democrat"));
}
```

## CLI Structure Patterns

```rust
// Use clap derive macros with global flags
#[derive(Parser)]
pub struct TradesArgs {
    /// Filter trades by politician name (two-step lookup)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by party (comma-separated): democrat (d), republican (r), other
    #[arg(long)]
    pub party: Option<String>,

    /// Results per page
    #[arg(long, default_value = "12")]
    pub page_size: i64,

    /// Read trades from local SQLite database (requires prior sync)
    #[arg(long)]
    pub db: Option<PathBuf>,
}

// Use Box for large variants to avoid clippy warnings
#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Query and display trades")]
    Trades(Box<TradesArgs>),
    Politicians(PoliticiansArgs),
    Issuers(IssuersArgs),
    Sync(SyncArgs),
    SyncFec(SyncFecArgs),
    EnrichPrices(EnrichPricesArgs),   // DB-only: Yahoo Finance price enrichment
    Portfolio(PortfolioArgs),          // DB-only: per-politician positions with P&L
    SyncDonations(SyncDonationsArgs),
    Donations(DonationsArgs),
    MapEmployers(MapEmployersArgs),
}
```

## Database & Validation Patterns

```rust
// Input validation - early returns with typed errors
pub fn validate_party(input: &str) -> Result<Party> {
    let normalized = input.trim().to_lowercase();
    match normalized.as_str() {
        "d" | "democrat" => Ok(Party::Democrat),
        "r" | "republican" => Ok(Party::Republican),
        "other" => Ok(Party::Other),
        _ => Err(CapitolTradesError::InvalidInput(
            format!("Invalid party: {}", input)
        )),
    }
}

// SQLite operations - use prepared statements with unchecked_transaction
pub fn update_trade_detail(&self, trade_id: &str, detail: &ScrapedTradeDetail) -> Result<()> {
    let tx = self.conn.unchecked_transaction()?;
    // Update trade fields with sentinel protection
    tx.execute(
        "UPDATE trades SET asset_type = CASE WHEN asset_type = 'unknown' THEN ?1 ELSE asset_type END,
         enriched_at = datetime('now') WHERE tx_id = ?2",
        params![detail.asset_type, trade_id],
    )?;
    tx.commit()?;
    Ok(())
}

// Schema migration pattern
pub fn migrate_v1(conn: &Connection) -> Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        conn.execute_batch("ALTER TABLE trades ADD COLUMN enriched_at TEXT;")?;
        conn.pragma_update(None, "user_version", 1)?;
    }
    Ok(())
}
```

## Enrichment Pipeline Patterns

```rust
// Concurrent enrichment with Semaphore + JoinSet + mpsc
let semaphore = Arc::new(Semaphore::new(concurrency));
let (tx, mut rx) = mpsc::channel(concurrency * 2);
let mut join_set = JoinSet::new();

for id in unenriched_ids {
    let permit = semaphore.clone().acquire_owned().await?;
    let client = scraper.clone();  // reqwest::Client is Arc-backed, cheap to clone
    let tx = tx.clone();
    join_set.spawn(async move {
        let result = client.trade_detail(&id).await;
        let _ = tx.send((id, result)).await;
        drop(permit);
    });
}
drop(tx);

// Single-threaded DB writes from channel receiver
while let Some((id, result)) = rx.recv().await {
    match result {
        Ok(detail) => db.update_trade_detail(&id, &detail)?,
        Err(e) => circuit_breaker.record_failure(),
    }
}

// CircuitBreaker: simple consecutive failure counter
struct CircuitBreaker { consecutive_failures: usize, threshold: usize }
impl CircuitBreaker {
    fn record_success(&mut self) { self.consecutive_failures = 0; }
    fn record_failure(&mut self) { self.consecutive_failures += 1; }
    fn is_tripped(&self) -> bool { self.consecutive_failures >= self.threshold }
}
```

## OpenFEC Rate Limiting Patterns

```rust
// Sliding-window rate limiter (shared across concurrent tasks)
let rate_limiter = Arc::new(RateLimiter::default()); // 900 req/hr

// Acquire a slot before each API call (sleeps if window full)
rate_limiter.acquire().await;

// Wrap API calls with retry on 429
let result = with_retry(&rate_limiter, 3, Duration::from_secs(60), || {
    client.get_schedule_a(&query)
}).await;

// Check remaining budget (non-blocking, returns None if lock contended)
let remaining = rate_limiter.remaining_budget(); // Option<u64>

// Post-run summary from atomic counters
let summary = rate_limiter.tracker().summary();
// summary.requests_made, .requests_succeeded, .requests_rate_limited, .requests_failed, .total_backoff_secs
```

## Output Formatting Patterns

```rust
// Two output paths: scrape types and DB types
// Scrape: print_trades_table(&trades), print_trades_csv(&trades), etc.
// DB:     print_db_trades_table(&rows), print_db_trades_csv(&rows), etc.

match format {
    OutputFormat::Table => print_db_trades_table(&rows),
    OutputFormat::Json => print_json(&rows),
    OutputFormat::Csv => print_db_trades_csv(&rows)?,
    OutputFormat::Markdown => print_db_trades_markdown(&rows),
    OutputFormat::Xml => print_db_trades_xml(&rows),
}

// CSV formula injection sanitization
fn sanitize_csv_field(field: &str) -> String {
    if field.starts_with('=') || field.starts_with('+')
        || field.starts_with('-') || field.starts_with('@') {
        format!("\t{}", field)
    } else {
        field.to_string()
    }
}
```

## DB Mode Filter Pattern

```rust
// Unsupported filters bail with explicit supported-filter list
let unsupported: &[(&str, bool)] = &[
    ("--committee", args.committee.is_some()),
    ("--trade-size", args.trade_size.is_some()),
];
for (flag, present) in unsupported {
    if *present {
        bail!("{} is not yet supported with --db. Supported filters: --party, --state, ...", flag);
    }
}

// Dynamic filter builder: push WHERE clauses and params into vecs
let mut clauses = Vec::new();
let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
let mut param_idx = 1;

if let Some(ref party) = filter.party {
    clauses.push(format!("p.party = ?{}", param_idx));
    params.push(Box::new(party.clone()));
    param_idx += 1;
}

let where_clause = if clauses.is_empty() {
    "WHERE 1=1".to_string()
} else {
    format!("WHERE {}", clauses.join(" AND "))
};
```

## Yahoo Finance & Portfolio Patterns

```rust
// YahooClient wraps yahoo_finance_api with DashMap caching
// Arc<YahooClient> for sharing across tasks (YahooConnector is not Clone)
let yahoo = Arc::new(YahooClient::new()?);

// Price fetching with weekend/holiday fallback (7-day lookback)
let price: Option<f64> = yahoo.get_price_on_date_with_fallback("AAPL", date).await?;
let current: Option<f64> = yahoo.get_current_price("AAPL").await?;

// Dollar range parsing and share estimation
let range = parse_trade_range(Some(15001.0), Some(50000.0));  // TradeRange with midpoint
let estimate = estimate_shares(&range.unwrap(), 150.0);        // ShareEstimate

// DB price enrichment operations
let unenriched = db.get_unenriched_price_trades(Some(50))?;   // PriceEnrichmentRow vec
db.update_trade_prices(tx_id, price, shares, value)?;          // Always sets price_enriched_at
db.update_current_price(ticker, price)?;                       // Phase 2 of enrichment

// FIFO portfolio calculator (pure logic, no DB)
let positions = calculate_positions(trades);  // HashMap<(politician_id, ticker), Position>

// DB portfolio operations
db.upsert_positions(&positions)?;             // ON CONFLICT update
let portfolio = db.get_portfolio(&filter)?;   // Vec<PortfolioPosition> with unrealized P&L
let option_count = db.count_option_trades(Some("P000197"))?;
```

## Red Flags & Anti-Patterns

- **Don't modify vendored capitoltrades_api** without documenting in project memory
- **Never use unwrap() in production code** - only in tests
- **Avoid raw string parsing** - use the validation module functions
- **Don't bypass cache** unless specifically required
- **Never commit secrets** - all config should be command-line args
- **Don't add new dependencies** without checking existing patterns first
- **Avoid async blocks in CLI entry points** - use async fn directly
- **Don't overwrite enriched data** - upserts use sentinel CASE protection
- **Don't write to SQLite from multiple threads** - use mpsc channel pattern
- **Don't clone YahooClient** - wrap in Arc instead (YahooConnector is not Clone)
- **Don't forget issuer JOIN** - ticker lives on issuers table, not trades; all price queries need JOIN

## Common Issues & Solutions

- **Large enum variant warnings**: Use `Box<YourArgs>` in Commands enum
- **Missing serde attributes**: Check existing patterns for camelCase vs snake_case
- **Test failures with network**: Use `wiremock` for HTTP integration tests, fixture-based tests for scraping
- **Memory leaks in cache**: Ensure DashMap TTL is properly configured (300s default)
- **XML serialization issues**: Use the JSON-to-XML bridge, don't modify vendored types
- **SQLite contention**: Use mpsc channel for concurrent writes, unchecked_transaction for reads
- **Enrichment data loss**: Sentinel CASE in upserts prevents re-sync from overwriting enriched fields
- **Stale page-size note**: Scrape mode ignores --page-size (fixed at 12); DB mode respects it
- **YahooConnector not Clone**: Wrap YahooClient in Arc for sharing across spawned tasks
- **Ticker on issuers table**: All price-related DB queries must JOIN issuers to access issuer_ticker
- **yahoo_finance_api Decimal is f64**: No conversion needed (type alias without 'decimal' feature)
- **response.quotes() errors**: Yahoo API returns Ok(response) but quotes() can fail with NoQuotes
- **Schema v2 fresh DBs**: Base schema.sql includes all columns; migrations only for existing DBs

## When to Ask

- Before modifying the vendored capitoltrades_api crate
- When adding new CLI subcommands (follow existing patterns)
- If changing the public API of validation functions
- When modifying the SQLite schema (must add versioned migration)
- If performance issues arise in the cache layer
- Before changing enrichment pipeline concurrency patterns
- Before modifying FIFO portfolio calculator logic (affects P&L correctness)
- When changing Yahoo Finance rate limiting or circuit breaker thresholds
- Before changing OpenFEC rate limiter budget or retry parameters
