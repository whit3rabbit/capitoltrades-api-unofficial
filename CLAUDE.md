# Capitol Traders - Agent Development Guide

This guide provides concrete patterns and conventions for agentic coding in this Rust workspace. For project structure overview, see the main documentation.

## Build & Development Commands

```bash
# Primary workspace commands
cargo check --workspace          # Fast compilation check
cargo test --workspace           # Run all 188 tests
cargo clippy --workspace         # Lint with all clippy rules
cargo run -p capitoltraders_cli -- trades --help  # Test CLI

# Single test execution patterns
cargo test -p capitoltrades_api deserialization    # By crate and test name
cargo test -p capitoltrades_lib validation::state_valid  # By module and specific test
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
    client::CachedClient,
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
// CLI command runners - consistent signature
pub async fn run(
    args: &TradesArgs,
    client: &CachedClient,
    format: OutputFormat,
) -> Result<()> {
    let trades = client.get_trades(&args.build_query()).await?;
    output::print_trades(trades, format)?;
    Ok(())
}

// Parameter order: args, client, format (consistent across all commands)
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
    use crate::types::Trade;
    
    #[test]
    fn test_validation_edge_case() {
        // High unwrap usage is acceptable in tests
        let result = validate_state("CA").unwrap();
        assert_eq!(result, "CA");
    }
    
    #[tokio::test]
    async fn test_async_function() {
        let client = CachedClient::new();
        let result = client.get_trades(&TradeQuery::default()).await.unwrap();
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
// Test all validation edge cases (see validation.rs for 83 examples)
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

## CLI Structure Patterns

```rust
// Use clap derive macros with global flags
#[derive(Parser)]
pub struct TradesArgs {
    /// Filter by politician name or ID
    #[arg(long, short)]
    pub politician: Option<String>,
    
    /// Filter by party (democrat, republican, other)
    #[arg(long)]
    pub party: Option<Vec<Party>>,
    
    /// Number of results to return (1-100)
    #[arg(long, default_value = "20")]
    pub page_size: i64,
    
    // Global output flag (shared across commands)
    #[arg(long, global = true, default_value = "table")]
    pub output: OutputFormat,
}

// Use Box for large variants to avoid clippy warnings
#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Query and display trades")]
    Trades(Box<TradesArgs>),
    Politicians(PoliticiansArgs),
    Issuers(IssuersArgs),
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

// SQLite operations - use prepared statements
pub fn insert_trades(conn: &Connection, trades: &[Trade]) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO trades (id, politician_id, issuer_id, amount) 
             VALUES (?, ?, ?, ?)"
        )?;
        for trade in trades {
            stmt.execute((
                &trade.id,
                &trade.politician.id,
                &trade.issuer.id,
                &trade.amount,
            ))?;
        }
    }
    tx.commit()?;
    Ok(())
}
```

## Output Formatting Patterns

```rust
// Support multiple output formats consistently
pub fn print_trades(trades: Vec<Trade>, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Table => {
            let table = tabled::Table::new(&trades)
                .with(tabled::settings::Style::modern())
                .to_string();
            println!("{}", table);
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&trades)?);
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(std::io::stdout());
            for trade in &trades {
                wtr.serialize(trade)?;
            }
            wtr.flush()?;
        }
        OutputFormat::Xml => {
            xml_output::print_trades_xml(trades)?;
        }
        OutputFormat::Markdown => {
            output::markdown::print_trades_md(trades)?;
        }
    }
    Ok(())
}
```

## Red Flags & Anti-Patterns

- **Don't modify vendored capitoltrades_api** without updating the modification log in AGENTS.md
- **Never use unwrap() in production code** - only in tests
- **Avoid raw string parsing** - use the validation module functions
- **Don't bypass cache** unless specifically required
- **Never commit secrets** - all config should be command-line args
- **Don't add new dependencies** without checking existing patterns first
- **Avoid async blocks in CLI entry points** - use async fn directly

## Common Issues & Solutions

- **Large enum variant warnings**: Use `Box<YourArgs>` in Commands enum
- **Missing serde attributes**: Check existing patterns for camelCase vs snake_case
- **Test failures with network**: Use `wiremock` for consistent integration testing
- **Memory leaks in cache**: Ensure DashMap TTL is properly configured (300s default)
- **XML serialization issues**: Use the JSON-to-XML bridge, don't modify vendored types

## When to Ask

- Before modifying the vendored capitoltrades_api crate
- When adding new CLI subcommands (follow existing patterns)  
- If changing the public API of validation functions
- When modifying the SQLite schema
- If performance issues arise in the cache layer