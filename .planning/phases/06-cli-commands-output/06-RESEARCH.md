# Phase 6: CLI Commands & Output - Research

**Researched:** 2026-02-10
**Domain:** Rust CLI design, command-line arguments, output formatting, progress reporting
**Confidence:** HIGH

## Summary

Phase 6 adds two new CLI subcommands to the existing capitoltraders CLI: `enrich-prices` (already implemented in Phase 4) and `portfolio` (new). The portfolio command queries the positions table and displays holdings with unrealized P&L calculations. All five existing output formats must be supported. The enrich-prices command already has progress reporting with indicatif.

The codebase already has mature patterns for CLI subcommands (clap 4 derive), output formatting (tabled, csv, serde_json, quick-xml), and progress reporting (indicatif 0.17). The portfolio command follows the exact same patterns as existing commands (trades, politicians, issuers), with DB-only mode since portfolio data is materialized.

**Primary recommendation:** Follow existing command patterns exactly. Add portfolio subcommand to Commands enum, create portfolio.rs module with PortfolioArgs struct, implement run_db function with filter building and format dispatch, add output functions for PortfolioPosition rows to output.rs.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| clap | 4.x | CLI argument parsing with derive macros | Industry standard Rust CLI framework, derive API is cleanest for subcommand-heavy apps |
| tabled | 0.17 | ASCII/Markdown table rendering | Already used for all tabular output, integrates with derive macros via Tabled trait |
| indicatif | 0.17 | Progress bars with ETA/elapsed time | De facto standard for Rust CLI progress reporting, handles terminal detection automatically |
| serde_json | 1.x | JSON serialization | Standard Rust serialization, works seamlessly with structs already implementing Serialize |
| csv | 1.3 | CSV output with formula injection protection | Standard Rust CSV library, existing sanitize_csv_field pattern prevents security issues |
| quick-xml | 0.37 | XML serialization | Existing JSON-to-XML bridge pattern avoids modifying vendored types |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| anyhow | 1.x | Error handling with context | Application-level errors in CLI commands |
| chrono | 0.4.x | Date/time formatting | Already used for timestamp display in all output formats |
| rusqlite | 0.x | SQLite parameter binding | DB queries via Db::get_portfolio, already pattern established |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| clap derive | clap builder API | Derive is more declarative, builder is more flexible but verbose |
| tabled | prettytable-rs, cli_table | tabled is actively maintained, has best Markdown support via Style::markdown() |
| indicatif | pb (progress bar), kdam | indicatif has superior terminal detection, tracing integration, and multiprogress |

**Installation:**
```bash
# Already in Cargo.toml, no new dependencies needed
cargo build --release -p capitoltraders_cli
```

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_cli/src/
├── main.rs              # Commands enum, OutputFormat dispatch
├── commands/
│   ├── mod.rs           # Module registration
│   ├── trades.rs        # Existing pattern: run() and run_db()
│   ├── politicians.rs   # Existing pattern: run() and run_db()
│   ├── issuers.rs       # Existing pattern: run() and run_db()
│   ├── sync.rs          # Existing pattern: DB-only operation
│   ├── enrich_prices.rs # Existing pattern: DB-only operation with progress
│   └── portfolio.rs     # NEW: DB-only operation, no run() needed
└── output.rs            # Add print_db_portfolio_* functions
```

### Pattern 1: Subcommand Structure (clap 4 derive)
**What:** Use #[derive(Args)] for argument structs, add to Commands enum with variant
**When to use:** Every new subcommand
**Example:**
```rust
// Source: capitoltraders_cli/src/main.rs
#[derive(Subcommand)]
enum Commands {
    Trades(Box<TradesArgs>),
    Politicians(PoliticiansArgs),
    Portfolio(PortfolioArgs),  // NEW
}

// Source: capitoltraders_cli/src/commands/portfolio.rs
#[derive(Args)]
pub struct PortfolioArgs {
    #[arg(long)]
    pub db: PathBuf,

    #[arg(long)]
    pub politician: Option<String>,

    #[arg(long)]
    pub party: Option<String>,

    #[arg(long)]
    pub state: Option<String>,

    #[arg(long)]
    pub ticker: Option<String>,
}
```

### Pattern 2: Filter Validation with Comma-Separated Values
**What:** Split on comma, validate each item, collect into Vec, use in retain closure
**When to use:** Multi-value filters like --party, --state
**Example:**
```rust
// Source: capitoltraders_cli/src/commands/trades.rs lines 212-219
if let Some(ref val) = args.party {
    let mut allowed = Vec::new();
    for item in val.split(',') {
        let p = validation::validate_party(item.trim())?;
        allowed.push(p.to_string());
    }
    trades.retain(|t| allowed.iter().any(|p| p == &t.politician.party));
}
```

### Pattern 3: DB-Only Command with Format Dispatch
**What:** Command that only operates on DB (no scrape mode), dispatches to format-specific output functions
**When to use:** Commands that read from materialized tables (portfolio, enrich-prices)
**Example:**
```rust
// Source: capitoltraders_cli/src/commands/issuers.rs (run_db pattern)
pub fn run_db(args: &IssuersArgs, db_path: &Path, format: &OutputFormat) -> Result<()> {
    let db = Db::open(db_path)?;

    // Build filter from args
    let filter = DbIssuerFilter {
        search: args.search.clone(),
        sector: args.sector.clone(),
        state: args.state.clone(),
        // ...
    };

    // Query DB
    let rows = db.query_issuers(&filter)?;

    // Format dispatch
    match format {
        OutputFormat::Table => print_db_issuers_table(&rows),
        OutputFormat::Json => print_json(&rows),
        OutputFormat::Csv => print_db_issuers_csv(&rows)?,
        OutputFormat::Markdown => print_db_issuers_markdown(&rows),
        OutputFormat::Xml => print_db_issuers_xml(&rows),
    }

    Ok(())
}
```

### Pattern 4: Output Row Struct with Tabled + Serialize
**What:** Flat struct with #[derive(Tabled, Serialize)] and field annotations
**When to use:** Every new data type that needs all five output formats
**Example:**
```rust
// Source: capitoltraders_cli/src/output.rs
#[derive(Tabled, Serialize)]
struct PortfolioRow {
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,

    #[tabled(rename = "Shares")]
    #[serde(rename = "Shares")]
    shares_held: String,  // Formatted as "100.00"

    #[tabled(rename = "Avg Cost")]
    #[serde(rename = "Avg Cost")]
    avg_cost_basis: String,  // "$50.00"

    #[tabled(rename = "Current Price")]
    #[serde(rename = "Current Price")]
    current_price: String,  // "$75.00" or "-"

    #[tabled(rename = "Current Value")]
    #[serde(rename = "Current Value")]
    current_value: String,  // "$7,500.00" or "-"

    #[tabled(rename = "Unrealized P&L")]
    #[serde(rename = "Unrealized P&L")]
    unrealized_pnl: String,  // "$2,500.00" or "-"

    #[tabled(rename = "P&L %")]
    #[serde(rename = "P&L %")]
    unrealized_pnl_pct: String,  // "+50.0%" or "-"
}
```

### Pattern 5: Progress Reporting with Indicatif
**What:** ProgressBar with template, set_message updates, finish_with_message on completion
**When to use:** Long-running operations with countable steps (already used in enrich-prices)
**Example:**
```rust
// Source: capitoltraders_cli/src/commands/enrich_prices.rs lines 136-147
let pb = ProgressBar::new(unique_pairs as u64);
pb.set_style(
    ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} ({eta}) {msg}",
    )
    .unwrap(),
);
pb.set_message("fetching historical prices...");

// In loop:
pb.set_message(format!("{} ok, {} err, {} skip", enriched, failed, skipped));
pb.inc(1);

pb.finish_with_message(format!("done: {} enriched", enriched));
```

### Pattern 6: Financial Data Formatting
**What:** Helper functions for currency, percentages, large numbers with +/- prefix for P&L
**When to use:** Portfolio P&L display, issuer performance metrics
**Example:**
```rust
// Source: capitoltraders_cli/src/output.rs lines 478-484
fn format_percent(value: f64) -> String {
    if value >= 0.0 {
        format!("+{:.1}%", value * 100.0)
    } else {
        format!("{:.1}%", value * 100.0)
    }
}

// For portfolio, need new formatter for shares (2 decimals) and currency (2 decimals with commas)
fn format_shares(shares: f64) -> String {
    format!("{:.2}", shares)
}

fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}
```

### Anti-Patterns to Avoid
- **Box<PortfolioArgs> without clippy warning:** Only use Box if clap large_enum_variant warning appears. Start without Box, add if needed.
- **Validating filters in run_db instead of validation module:** Use existing validation::validate_* functions for consistency
- **Building SQL in command layer:** Use PortfolioFilter struct and db.get_portfolio(&filter), follow existing pattern
- **Separate progress reporting for Phase 2 current prices:** Already implemented in enrich-prices, no changes needed
- **Custom XML serialization for portfolio:** Use existing xml_output::items_to_xml pattern with singular() mapping

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Progress bar rendering | Manual print with carriage returns | indicatif::ProgressBar | Handles terminal detection (prevents control chars in pipes), TTY vs non-TTY, multi-progress coordination, cursor positioning edge cases |
| Table formatting | Manual column width calculation and padding | tabled::Table with Style::markdown() | Handles Unicode width calculation, overflow strategies, header formatting, consistent with existing commands |
| CSV formula injection protection | Manual escaping or field validation | Existing sanitize_csv_field() | Spreadsheet formula injection is subtle (=, +, -, @ prefixes), centralized pattern already reviewed |
| Comma-separated filter parsing | Custom split/trim/validate loop | Existing pattern in trades.rs | Consistent validation error messages, handles whitespace, multi-value validation already proven |
| Financial percentage formatting | f64 to string with decimal places | Existing format_percent() with +/- prefix | Positive gains need "+" prefix for clarity, negative automatically has "-", consistent precision (1 decimal for %) |

**Key insight:** CLI formatting edge cases are deceptively complex. Terminal width detection, Unicode character widths, CSV injection vectors, progress bar cursor positioning, and TTY vs pipe detection all have subtle failure modes. Using battle-tested libraries prevents production bugs.

## Common Pitfalls

### Pitfall 1: Option Positions Displayed in Main Table
**What goes wrong:** Users expect to see option positions alongside stock positions, but options lack price data and estimated shares, causing confusing "-" values
**Why it happens:** query_trades_for_portfolio filters asset_type = 'stock', option trades never enter FIFO calculator
**How to avoid:** Display stock positions in main table, add separate section below with count_option_trades() result and explanatory note
**Warning signs:** If main table shows lots of "-" for unrealized P&L, or user reports "missing positions"

### Pitfall 2: Negative Percentage Without Plus Sign
**What goes wrong:** "+50.0%" vs "50.0%" looks inconsistent, users confused whether gain or loss
**Why it happens:** Rust format! only adds "-" for negatives, not "+" for positives
**How to avoid:** Use format_percent() which explicitly checks >= 0.0 and prefixes with "+"
**Warning signs:** Financial display conventions require explicit "+" for gains, review any percentage formatting

### Pitfall 3: Filter Validation Inconsistency
**What goes wrong:** Some filters use validation::validate_state(), others do raw .to_uppercase()
**Why it happens:** Copy-paste from different commands that evolved separately
**How to avoid:** Always use validation module functions, even for simple cases like state
**Warning signs:** If validation::validate_state() exists but code does manual uppercase/trim, refactor to use validator

### Pitfall 4: Missing --db Flag Validation
**What goes wrong:** User runs `capitoltraders portfolio` without --db, gets confusing error from db.rs instead of clap usage
**Why it happens:** --db is optional on some commands (trades has scrape mode), but required on portfolio
**How to avoid:** Use #[arg(long)] (required by default) not #[arg(long)] with Option<PathBuf> for DB-only commands
**Warning signs:** If clap doesn't show --db as required in --help output, fix the Args struct

### Pitfall 5: Closed Position Display Confusion
**What goes wrong:** User sees position with 0 shares in table, wonders why it's shown
**Why it happens:** PortfolioFilter.include_closed defaults to false, but if manually set true positions with shares_held near 0.0 appear
**How to avoid:** Document that default hides closed positions, --include-closed flag makes them visible
**Warning signs:** Complaints about "missing sold positions" or "zero share rows showing up"

### Pitfall 6: CSV Formula Injection on Politician Names
**What goes wrong:** Politician name like "=SUM(A1:A10) Smith" executes as formula when CSV opened in Excel
**Why it happens:** Names starting with =, +, -, @ are interpreted as formulas
**How to avoid:** Use sanitize_csv_field() on all string fields in CSV output, already implemented for other commands
**Warning signs:** If CSV output bypasses sanitize_csv_field, spreadsheet users are vulnerable

### Pitfall 7: Unrealized P&L Calculation Without Null Check
**What goes wrong:** current_price is None (no enriched trades), P&L calculation panics or shows wrong value
**Why it happens:** Forgetting that current_price comes from subquery and can be NULL
**How to avoid:** PortfolioPosition already has Option<f64> for current_price and unrealized_pnl, display "-" when None
**Warning signs:** If format code does .unwrap() on Option fields, will panic on missing price data

## Code Examples

Verified patterns from official sources and existing codebase:

### Add Subcommand to Commands Enum
```rust
// Source: capitoltraders_cli/src/main.rs lines 34-46
#[derive(Subcommand)]
enum Commands {
    Trades(Box<commands::trades::TradesArgs>),
    Politicians(commands::politicians::PoliticiansArgs),
    Issuers(commands::issuers::IssuersArgs),
    Sync(commands::sync::SyncArgs),
    EnrichPrices(commands::enrich_prices::EnrichPricesArgs),
    Portfolio(commands::portfolio::PortfolioArgs),  // NEW
}

// In main():
Commands::Portfolio(args) => commands::portfolio::run(args, &format).await?,
```

### Portfolio Command Args Structure
```rust
// NEW: capitoltraders_cli/src/commands/portfolio.rs
use anyhow::Result;
use capitoltraders_lib::{Db, PortfolioFilter};
use clap::Args;
use std::path::PathBuf;
use crate::output::OutputFormat;

#[derive(Args)]
pub struct PortfolioArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Filter by politician ID (e.g., P000001)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by party (comma-separated): democrat (d), republican (r)
    #[arg(long)]
    pub party: Option<String>,

    /// Filter by state (comma-separated, e.g., CA,TX)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by ticker symbol
    #[arg(long)]
    pub ticker: Option<String>,

    /// Include closed positions (shares_held near zero)
    #[arg(long)]
    pub include_closed: bool,
}

pub async fn run(args: &PortfolioArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Build filter from args
    let filter = PortfolioFilter {
        politician_id: args.politician.clone(),
        ticker: args.ticker.clone(),
        party: args.party.clone(),
        state: args.state.clone(),
        include_closed: args.include_closed,
    };

    // Query portfolio positions
    let positions = db.get_portfolio(&filter)?;

    // Count option trades separately
    let option_count = db.count_option_trades(filter.politician_id.as_deref())?;

    // Format dispatch
    match format {
        OutputFormat::Table => {
            crate::output::print_portfolio_table(&positions);
            if option_count > 0 {
                eprintln!("\nNote: {} option positions excluded (valuation deferred)", option_count);
            }
        }
        OutputFormat::Json => crate::output::print_json(&positions),
        OutputFormat::Csv => crate::output::print_portfolio_csv(&positions)?,
        OutputFormat::Markdown => crate::output::print_portfolio_markdown(&positions),
        OutputFormat::Xml => crate::output::print_portfolio_xml(&positions),
    }

    Ok(())
}
```

### Portfolio Output Row Builder
```rust
// NEW: capitoltraders_cli/src/output.rs
#[derive(Tabled, Serialize)]
struct PortfolioRow {
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,

    #[tabled(rename = "Shares")]
    #[serde(rename = "Shares")]
    shares_held: String,

    #[tabled(rename = "Avg Cost")]
    #[serde(rename = "Avg Cost")]
    avg_cost_basis: String,

    #[tabled(rename = "Current Price")]
    #[serde(rename = "Current Price")]
    current_price: String,

    #[tabled(rename = "Current Value")]
    #[serde(rename = "Current Value")]
    current_value: String,

    #[tabled(rename = "Unrealized P&L")]
    #[serde(rename = "Unrealized P&L")]
    unrealized_pnl: String,

    #[tabled(rename = "P&L %")]
    #[serde(rename = "P&L %")]
    unrealized_pnl_pct: String,
}

fn build_portfolio_rows(positions: &[PortfolioPosition]) -> Vec<PortfolioRow> {
    positions
        .iter()
        .map(|p| PortfolioRow {
            ticker: p.ticker.clone(),
            shares_held: format_shares(p.shares_held),
            avg_cost_basis: format_currency(p.cost_basis),
            current_price: p.current_price
                .map(format_currency)
                .unwrap_or_else(|| "-".to_string()),
            current_value: p.current_value
                .map(|v| format!("${:,.2}", v))
                .unwrap_or_else(|| "-".to_string()),
            unrealized_pnl: p.unrealized_pnl
                .map(|pnl| {
                    if pnl >= 0.0 {
                        format!("+${:,.2}", pnl)
                    } else {
                        format!("-${:,.2}", pnl.abs())
                    }
                })
                .unwrap_or_else(|| "-".to_string()),
            unrealized_pnl_pct: p.unrealized_pnl_pct
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string()),
        })
        .collect()
}

pub fn print_portfolio_table(positions: &[PortfolioPosition]) {
    println!("{}", Table::new(build_portfolio_rows(positions)));
}

pub fn print_portfolio_markdown(positions: &[PortfolioPosition]) {
    let mut table = Table::new(build_portfolio_rows(positions));
    table.with(Style::markdown());
    println!("{}", table);
}

pub fn print_portfolio_csv(positions: &[PortfolioPosition]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_portfolio_rows(positions) {
        row.ticker = sanitize_csv_field(&row.ticker);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn print_portfolio_xml(positions: &[PortfolioPosition]) {
    println!("{}", xml_output::portfolio_to_xml(positions));
}

fn format_shares(shares: f64) -> String {
    format!("{:.2}", shares)
}

fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}
```

### XML Serialization for Portfolio
```rust
// NEW: capitoltraders_cli/src/xml_output.rs
use capitoltraders_lib::PortfolioPosition;

pub fn portfolio_to_xml(positions: &[PortfolioPosition]) -> String {
    items_to_xml("portfolio", "position", positions)
}
```

### Enrich-Prices Progress Display (Already Implemented)
```rust
// Source: capitoltraders_cli/src/commands/enrich_prices.rs lines 324-342
eprintln!();
eprintln!(
    "Price enrichment complete: {} enriched, {} failed, {} skipped",
    enriched, failed, skipped + skipped_parse_errors
);
eprintln!(
    "  ({} total trades, {} unique ticker-date pairs, {} unique tickers)",
    total_trades, unique_pairs, unique_tickers
);

if breaker.is_tripped() {
    eprintln!(
        "Warning: Circuit breaker tripped after {} consecutive failures -- some trades were not processed",
        CIRCUIT_BREAKER_THRESHOLD
    );
    bail!("Enrichment aborted due to circuit breaker");
}

Ok(())
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| clap 3 builder API | clap 4 derive macros | 2023 | Derive API now recommended for all new commands, cleaner subcommand definitions |
| Manual CSV escaping | sanitize_csv_field() with tab prefix | Phase 3 security review | Prevents spreadsheet formula injection consistently |
| println! for progress | indicatif ProgressBar | 2020s (mature library) | Handles TTY detection, prevents control chars in pipes, clean terminal management |
| prettytable-rs | tabled 0.17 | 2022+ | Better Markdown support, more active maintenance, cleaner derive integration |
| Separate functions per format | Generic print_json() | Codebase evolution | JSON serialization works for any Serialize type, reduces duplication |

**Deprecated/outdated:**
- **clap 2.x app! macro:** Deprecated, use clap 4 derive API
- **Manual terminal width detection:** Use tabled with default settings, auto-detects
- **Custom progress bar formatting:** Use indicatif templates with {bar}, {pos}, {len}, {eta}, {msg} placeholders

## Open Questions

1. **Should portfolio command support politician name lookup like trades --politician flag?**
   - What we know: trades.rs has --politician that does two-step lookup (search politicians by name, use ID in filter)
   - What's unclear: Whether portfolio needs this or if --politician should only accept politician_id
   - Recommendation: Start with politician_id only (simpler), add name lookup in follow-up if users request it

2. **How to display option count note in non-table formats (JSON, CSV)?**
   - What we know: Table output can use eprintln! for note, but JSON/CSV should be pure data
   - What's unclear: Whether to add option_trade_count field to JSON output or omit entirely
   - Recommendation: Omit from JSON/CSV (pure data), only show in table/markdown output via eprintln!

3. **Should closed positions be included by default?**
   - What we know: PortfolioFilter.include_closed defaults to false (filters shares_held > 0.0001)
   - What's unclear: User expectation for default behavior
   - Recommendation: Keep default as false (hide closed), add --include-closed flag for users who want full history

## Sources

### Primary (HIGH confidence)
- Existing codebase patterns:
  - capitoltraders_cli/src/main.rs - Commands enum structure
  - capitoltraders_cli/src/commands/trades.rs - Filter validation pattern
  - capitoltraders_cli/src/commands/enrich_prices.rs - Progress reporting pattern
  - capitoltraders_cli/src/output.rs - Output formatting patterns
  - capitoltraders_lib/src/db.rs - PortfolioPosition and PortfolioFilter structs (lines 2109-2131)
- Official documentation:
  - [clap derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) - Subcommand patterns
  - [indicatif documentation](https://docs.rs/indicatif/latest/indicatif/) - Progress bar API
  - [tabled documentation](https://docs.rs/tabled) - Table formatting

### Secondary (MEDIUM confidence)
- Best practices guides:
  - [Rain's Rust CLI recommendations - Handling arguments](https://rust-cli-recommendations.sunshowers.io/handling-arguments.html) - Clap subcommand organization
  - [How to Build CLI Applications with Clap in Rust (2026-02-03)](https://oneuptime.com/blog/post/2026-02-03-rust-clap-cli-applications/view) - Modern clap 4 patterns
  - [Understanding Portfolio Returns & Calculations](https://support.simplywall.st/hc/en-us/articles/9423775242383-Understanding-the-Portfolio-Returns-Analysis-Calculations) - P&L calculation formulas
  - [Position and P&L - Interactive Brokers](https://www.ibkrguides.com/traderworkstation/position-and-pnl.htm) - Financial display conventions

### Tertiary (LOW confidence)
- Web search findings:
  - [CLI output mode - CLI for Microsoft 365](https://pnp.github.io/cli-microsoft365/user-guide/cli-output-mode/) - Multi-format output examples
  - [Tips on Adding JSON Output to Your CLI App](https://blog.kellybrazil.com/2021/12/03/tips-on-adding-json-output-to-your-cli-app/) - General JSON output best practices
  - [Progress bar variables in cli package](https://rdrr.io/cran/cli/man/progress-variables.html) - Progress reporting tokens (not Rust-specific)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries already in use, versions confirmed in Cargo.toml
- Architecture patterns: HIGH - Verified via existing codebase, multiple command examples
- Pitfalls: MEDIUM - Based on codebase patterns and general CLI experience, some hypothetical
- Code examples: HIGH - Direct excerpts from working codebase with line numbers

**Research date:** 2026-02-10
**Valid until:** 60 days (stable domain - CLI patterns and libraries evolve slowly)
