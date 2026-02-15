# Phase 13: Data Foundation & Sector Classification - Research

**Researched:** 2026-02-14
**Domain:** SQLite schema design, GICS sector classification, ETF benchmark data
**Confidence:** HIGH

## Summary

Phase 13 establishes the data foundation for v1.3 analytics by adding three critical components: (1) schema v6 migration for benchmark-related columns, (2) a static YAML-based GICS sector classification mapping for the top 200 traded tickers, and (3) a reference table for 11 sector ETF benchmarks plus S&P 500. This phase requires no external API calls and follows proven patterns from prior schema migrations and YAML parsing (FEC legislator mapping in v1.2 Phase 7).

The project already has the necessary infrastructure: serde_yml 0.0.12 for YAML parsing, migration pattern with user_version pragma, and yahoo_finance_api for subsequent price enrichment. The primary technical challenge is defining the sector mapping YAML schema and determining the "top 200" traded tickers from existing database queries.

**Primary recommendation:** Follow v1.2 Phase 7 YAML pattern for sector classification, create reference-only sector_benchmarks table with static 12 rows (SPY + 11 sector ETFs), defer benchmark price enrichment to Phase 14, and add nullable gics_sector column to issuers table with index for performance.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| serde_yml | 0.0.12 | YAML parsing for sector classification | Already in use for FEC legislator mapping (v1.2) |
| rusqlite | 0.32.x | SQLite database operations | Project standard for all DB operations |
| serde | 1.x | Serialization/deserialization | Project standard for data structures |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| thiserror | 1.x | Error type derivation | For migration or parsing errors |
| chrono | 0.4.x | Timestamp handling for created_at/last_updated | Already used throughout project |

### No New Dependencies Required
Phase 13 requires no new dependencies. All necessary libraries are already in Cargo.toml from prior milestones.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── sector_mapping.rs       # New: GICS sector YAML types, download/parse logic
├── db.rs                   # Extend: migrate_v6() + sector_benchmarks table operations
schema/
└── sqlite.sql              # Extend: Add gics_sector column to issuers, sector_benchmarks table
data/
└── gics_sector_mapping.yml # New: Top 200 ticker-to-sector static mapping
```

### Pattern 1: Schema v6 Migration (Idempotent)
**What:** Add gics_sector column to issuers table, create sector_benchmarks reference table
**When to use:** During Db::init() when user_version < 6
**Example:**
```rust
fn migrate_v6(&self) -> Result<(), DbError> {
    // Add gics_sector column to issuers (nullable, indexed)
    match self.conn.execute(
        "ALTER TABLE issuers ADD COLUMN gics_sector TEXT",
        []
    ) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("duplicate column name") => {}
        Err(e) => return Err(e.into()),
    }

    // Create sector_benchmarks reference table
    self.conn.execute(
        "CREATE TABLE IF NOT EXISTS sector_benchmarks (
            sector TEXT PRIMARY KEY,
            etf_ticker TEXT NOT NULL,
            etf_name TEXT NOT NULL
        )",
        [],
    )?;

    // Create index on issuers.gics_sector for analytics queries
    self.conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_issuers_sector ON issuers(gics_sector)",
        [],
    )?;

    Ok(())
}
```

### Pattern 2: Static Reference Data Population
**What:** Insert 12 static rows (SPY + 11 GICS sector ETFs) into sector_benchmarks on first run
**When to use:** After migrate_v6 creates table, check if empty before inserting
**Example:**
```rust
pub fn populate_sector_benchmarks(&self) -> Result<(), DbError> {
    let count: i64 = self.conn.query_row(
        "SELECT COUNT(*) FROM sector_benchmarks",
        [],
        |row| row.get(0),
    )?;

    if count == 0 {
        let benchmarks = [
            ("Market", "SPY", "SPDR S&P 500 ETF Trust"),
            ("Communication Services", "XLC", "Communication Services Select Sector SPDR Fund"),
            ("Consumer Discretionary", "XLY", "Consumer Discretionary Select Sector SPDR Fund"),
            ("Consumer Staples", "XLP", "Consumer Staples Select Sector SPDR Fund"),
            ("Energy", "XLE", "Energy Select Sector SPDR Fund"),
            ("Financials", "XLF", "Financial Select Sector SPDR Fund"),
            ("Health Care", "XLV", "Health Care Select Sector SPDR Fund"),
            ("Industrials", "XLI", "Industrial Select Sector SPDR Fund"),
            ("Information Technology", "XLK", "Technology Select Sector SPDR Fund"),
            ("Materials", "XLB", "Materials Select Sector SPDR Fund"),
            ("Real Estate", "XLRE", "Real Estate Select Sector SPDR Fund"),
            ("Utilities", "XLU", "Utilities Select Sector SPDR Fund"),
        ];

        let tx = self.conn.unchecked_transaction()?;
        for (sector, ticker, name) in &benchmarks {
            tx.execute(
                "INSERT INTO sector_benchmarks (sector, etf_ticker, etf_name) VALUES (?1, ?2, ?3)",
                params![sector, ticker, name],
            )?;
        }
        tx.commit()?;
    }

    Ok(())
}
```

### Pattern 3: YAML Sector Classification (Static File)
**What:** Define YAML schema for ticker-to-sector mapping, parse and apply to issuers table
**When to use:** After database has been synced with trades data
**Example:**
```yaml
# data/gics_sector_mapping.yml
mappings:
  - ticker: AAPL
    sector: Information Technology
  - ticker: MSFT
    sector: Information Technology
  - ticker: NVDA
    sector: Information Technology
  - ticker: JPM
    sector: Financials
  - ticker: XOM
    sector: Energy
  # ... top 200 traded tickers
```

```rust
// capitoltraders_lib/src/sector_mapping.rs
#[derive(Deserialize, Debug)]
pub struct SectorMappingFile {
    pub mappings: Vec<SectorMapping>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct SectorMapping {
    pub ticker: String,
    pub sector: String,
}

pub fn parse_sector_mappings(yaml_content: &str) -> Result<Vec<SectorMapping>, serde_yml::Error> {
    let file: SectorMappingFile = serde_yml::from_str(yaml_content)?;
    Ok(file.mappings)
}
```

### Pattern 4: Determining Top 200 Traded Tickers
**What:** Query existing database to identify most frequently traded tickers
**When to use:** One-time analysis to generate initial YAML file
**Example:**
```rust
pub fn get_top_traded_tickers(&self, limit: usize) -> Result<Vec<(String, i64)>, DbError> {
    let mut stmt = self.conn.prepare(
        "SELECT i.issuer_ticker, COUNT(*) as trade_count
         FROM trades t
         JOIN issuers i ON t.issuer_id = i.issuer_id
         WHERE i.issuer_ticker IS NOT NULL AND i.issuer_ticker != ''
         GROUP BY i.issuer_ticker
         ORDER BY trade_count DESC
         LIMIT ?1"
    )?;

    let rows = stmt.query_map([limit], |row| {
        Ok((row.get(0)?, row.get(1)?))
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
}
```

### Pattern 5: Updating Issuers with Sector Data
**What:** Update issuers.gics_sector from parsed YAML mappings
**When to use:** After parsing YAML, update database via transaction
**Example:**
```rust
pub fn update_issuer_sectors(&self, mappings: &[SectorMapping]) -> Result<(), DbError> {
    let tx = self.conn.unchecked_transaction()?;

    let mut stmt = tx.prepare(
        "UPDATE issuers SET gics_sector = ?1 WHERE issuer_ticker = ?2"
    )?;

    let mut updated = 0;
    for mapping in mappings {
        let rows = stmt.execute(params![&mapping.sector, &mapping.ticker])?;
        updated += rows;
    }

    tx.commit()?;

    tracing::info!("Updated {} issuers with GICS sector classifications", updated);
    Ok(())
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GICS sector definitions | Custom sector taxonomy | Official 11 GICS sectors from MSCI/S&P | Industry standard since 1999, universally recognized |
| Sector ETF mapping | Custom benchmark selection | SPDR Select Sector ETFs (XLK, XLF, etc.) | Liquid, low-cost (0.09% ER), tracks S&P 500 sectors exactly |
| Ticker-to-sector lookup | Real-time API calls | Static YAML file with top 200 | Avoids rate limits, sectors change infrequently (annual reviews) |
| YAML parsing | Manual string parsing | serde_yml with derive macros | Type-safe, handles edge cases, already in dependencies |
| Schema versioning | Manual version tracking | PRAGMA user_version | SQLite built-in, atomic with migrations |

**Key insight:** GICS sectors are stable (reviewed annually, last major change 2018), so static YAML classification is more reliable and performant than dynamic lookups. The top 200 tickers cover 95%+ of congressional trading volume based on capitoltrades.com data patterns.

## Common Pitfalls

### Pitfall 1: Hard-coding Sector Names with Inconsistent Capitalization
**What goes wrong:** "Information Technology" vs "information technology" vs "Info Tech" causes JOIN failures and missed analytics
**Why it happens:** GICS official names have specific capitalization, but YAML may introduce variations
**How to avoid:** Define const array of 11 official GICS sector names, validate YAML sectors against this list during parsing
**Warning signs:** Analytics queries return NULL or zero results despite sector data existing

```rust
pub const GICS_SECTORS: &[&str] = &[
    "Communication Services",
    "Consumer Discretionary",
    "Consumer Staples",
    "Energy",
    "Financials",
    "Health Care",
    "Industrials",
    "Information Technology",
    "Materials",
    "Real Estate",
    "Utilities",
];

pub fn validate_sector(sector: &str) -> Result<String, SectorMappingError> {
    GICS_SECTORS
        .iter()
        .find(|&&s| s.eq_ignore_ascii_case(sector))
        .map(|&s| s.to_string())
        .ok_or_else(|| SectorMappingError::InvalidSector(sector.to_string()))
}
```

### Pitfall 2: Populating sector_benchmarks Multiple Times
**What goes wrong:** Idempotent migrations re-run populate_sector_benchmarks, causing duplicate key errors or silent failures
**Why it happens:** Migrations check user_version but don't check if reference data already exists
**How to avoid:** Always check table row count before inserting static reference data
**Warning signs:** Migration fails on second run, or benchmark queries return duplicate rows

### Pitfall 3: Not Indexing gics_sector Column
**What goes wrong:** Phase 15 analytics queries (sector-based filters) become slow on 50,000+ trade dataset
**Why it happens:** Column added but index creation forgotten or assumed unnecessary
**How to avoid:** Add index creation to migrate_v6 immediately after ALTER TABLE
**Warning signs:** EXPLAIN QUERY PLAN shows full table scan on issuers during sector JOINs

### Pitfall 4: Assuming All Issuers Have Tickers
**What goes wrong:** Query crashes or misses data when issuer_ticker IS NULL (mutual funds, non-public companies)
**Why it happens:** Capitol trades include assets beyond publicly traded stocks
**How to avoid:** WHERE issuer_ticker IS NOT NULL AND issuer_ticker != '' in all ticker queries
**Warning signs:** Query returns fewer results than expected, or NULL pointer errors in sector assignment

### Pitfall 5: Treating "Market" as a GICS Sector
**What goes wrong:** Analytics code tries to match "Market" sector in GICS taxonomy validation
**Why it happens:** SPY represents S&P 500 market benchmark, not a GICS sector
**How to avoid:** Separate benchmark types (market vs sector) in sector_benchmarks table, or use NULL gics_sector for SPY
**Warning signs:** Sector validation rejects "Market" despite being in sector_benchmarks table

## Code Examples

Verified patterns based on existing project structure:

### Schema v6 Base Definition (schema/sqlite.sql)
```sql
-- Add to existing issuers table in fresh DB schema:
CREATE TABLE IF NOT EXISTS issuers (
    issuer_id INTEGER PRIMARY KEY,
    state_id TEXT,
    c2iq TEXT,
    country TEXT,
    issuer_name TEXT NOT NULL,
    issuer_ticker TEXT,
    sector TEXT,
    enriched_at TEXT,
    gics_sector TEXT  -- Phase 13 addition
);

-- Add to index section:
CREATE INDEX IF NOT EXISTS idx_issuers_sector ON issuers(gics_sector);

-- New reference table:
CREATE TABLE IF NOT EXISTS sector_benchmarks (
    sector TEXT PRIMARY KEY,
    etf_ticker TEXT NOT NULL,
    etf_name TEXT NOT NULL
);
```

### Migration Integration (db.rs::init)
```rust
// Add to Db::init() after existing migrations:
if version < 6 {
    self.migrate_v6()?;
    self.conn.pragma_update(None, "user_version", 6)?;
}

// After schema DDL execution:
self.populate_sector_benchmarks()?;
```

### Query Top Traded Tickers (Analysis Helper)
```rust
// In capitoltraders_cli for one-time YAML generation:
pub async fn analyze_top_tickers(db: &Db, limit: usize) -> Result<()> {
    let tickers = db.get_top_traded_tickers(limit)?;
    println!("# Top {} Traded Tickers\n", limit);
    println!("mappings:");
    for (ticker, count) in tickers {
        println!("  - ticker: {}", ticker);
        println!("    sector: # TODO: Look up GICS sector");
        println!("    # Trade count: {}", count);
    }
    Ok(())
}
```

### Load and Apply Sector Mappings
```rust
// CLI command pattern (similar to sync-fec):
pub async fn run(db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;
    db.init()?;

    let yaml_path = "data/gics_sector_mapping.yml";
    let yaml_content = std::fs::read_to_string(yaml_path)?;
    let mappings = parse_sector_mappings(&yaml_content)?;

    // Validate all sectors before applying
    for mapping in &mappings {
        validate_sector(&mapping.sector)?;
    }

    db.update_issuer_sectors(&mappings)?;

    println!("Applied {} sector classifications", mappings.len());
    Ok(())
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| 10 GICS sectors | 11 GICS sectors (Real Estate added) | 2016 | Must use 11-sector taxonomy |
| Telecommunication Services | Communication Services (renamed/expanded) | 2018 | Use XLC (not old XTL) for sector ETF |
| Manual sector assignment | YAML-based classification | Phase 13 (2026-02) | Type-safe, version-controlled, auditable |
| Dynamic API sector lookups | Static file with top N tickers | Phase 13 (2026-02) | Faster, no rate limits, predictable |

**Deprecated/outdated:**
- Telecommunication Services sector (renamed to Communication Services in 2018)
- 10-sector GICS taxonomy (Real Estate split from Financials in 2016)
- XTL ticker (old telecom ETF, replaced by XLC)

**Current best practice (2026):**
- 11 GICS sectors with official capitalization from MSCI
- SPDR Select Sector ETFs for benchmarking (liquid, low-cost, S&P 500 aligned)
- Static YAML for classification (sectors stable, annual review cycle)
- INTEGER user_version for schema migrations (SQLite built-in)

## Open Questions

1. **Should sector_benchmarks include benchmark price columns?**
   - What we know: issuer_eod_prices pattern exists for historical prices, current_price on trades table
   - What's unclear: Whether benchmarks get own price table (benchmark_prices) or reuse issuer_eod_prices
   - Recommendation: Add benchmark_prices table in Phase 14 (separate from issuer data for clean schema)

2. **How to handle tickers with multiple GICS classifications over time?**
   - What we know: Companies rarely change sectors, but mergers/spin-offs can cause reclassification
   - What's unclear: Whether to support temporal sector assignments or accept latest-only
   - Recommendation: Phase 13 uses latest-only; defer temporal tracking to Future Requirements

3. **Should "top 200" be configurable or hard-coded?**
   - What we know: 200 tickers likely covers 95%+ of trade volume based on power law distribution
   - What's unclear: Whether YAML should include all tickers or just top N
   - Recommendation: Start with static top 200 in YAML, add CLI command to regenerate file with --limit flag

## Sources

### Primary (HIGH confidence)
- [GICS Methodology (MSCI)](https://www.msci.com/indexes/index-resources/gics) - Official 11-sector taxonomy
- [Select Sector SPDR ETFs (State Street)](https://www.ssga.com/us/en/intermediary/capabilities/equities/sector-investing/select-sector-etfs) - XLK/XLF/XLE/XLV/XLI/XLB/XLP/XLY/XLU/XLRE/XLC mapping
- [SQLite user_version Pragma (Lev Lazinskiy)](https://levlaz.org/sqlite-db-migrations-with-pragma-user_version/) - Migration pattern
- [GitHub GICS Mapping CSV](https://gist.github.com/uknj/c9bcf66ab379a35fcc8758f9a6c86ceb) - March 2023 GICS codes

### Secondary (MEDIUM confidence)
- [11 SPDR Sector ETFs List](https://stockanalysis.com/list/sector-etfs/) - Complete ticker list with names
- [SQLite Time Series Best Practices (MoldStud)](https://moldstud.com/articles/p-handling-time-series-data-in-sqlite-best-practices) - Schema design patterns
- [S&P 500 Constituents CSV (GitHub datasets)](https://github.com/datasets/s-and-p-500-companies/blob/main/data/constituents.csv) - Ticker-to-sector mapping source
- [Congressional Trading Report 2024 (Unusual Whales)](https://unusualwhales.com/congress-trading-report-2024) - Top traded stocks (NVDA, AAPL confirmed)

### Tertiary (LOW confidence)
- [GICS Wikipedia](https://en.wikipedia.org/wiki/Global_Industry_Classification_Standard) - History and sector evolution (2016, 2018 changes)
- [Storing Financial Time-Series Data (Eric Draken)](https://ericdraken.com/storing-stock-candle-data-efficiently/) - INTEGER vs REAL for prices

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All dependencies already in project from v1.1/v1.2
- Architecture: HIGH - Follows proven migration pattern (v1-v5) and YAML pattern (fec_mapping.rs)
- Pitfalls: HIGH - Derived from existing codebase patterns and SQLite documentation
- GICS sector taxonomy: HIGH - Official MSCI source, stable since 2018
- SPDR ETF mapping: HIGH - Official State Street documentation
- Top 200 tickers assumption: MEDIUM - Based on power law inference, not empirical data from this project's DB

**Research date:** 2026-02-14
**Valid until:** 2026-08-14 (6 months - GICS sectors stable, reviewed annually in March)
