# Phase 1: Schema Migration & Data Model - Research

**Researched:** 2026-02-09
**Domain:** SQLite schema migration and financial data modeling
**Confidence:** HIGH

## Summary

Phase 1 adds price storage columns to the trades table and creates a positions table for materialized portfolio holdings. The existing migrate_v1 pattern (PRAGMA user_version with error handling for duplicate columns) provides a proven template. SQLite's ALTER TABLE ADD COLUMN supports nullable REAL and INTEGER columns without default values, and idempotent migrations are achieved by catching "duplicate column name" errors. The positions table stores net FIFO positions per politician per ticker, avoiding repeated calculation overhead for large datasets.

**Primary recommendation:** Extend the existing migrate_v1 pattern to migrate_v2, adding 5 nullable columns to trades (trade_date_price REAL, current_price REAL, price_enriched_at TEXT, estimated_shares INTEGER, estimated_value REAL) and creating a positions table with 6 columns (politician_id, issuer_ticker, shares_held REAL, cost_basis REAL, realized_pnl REAL, last_updated TEXT). Use the established duplicate column error handling pattern for idempotency.

**Contradiction Alert:** STATE.md decision says "Materialized positions table (avoids FIFO recalculation on every query)" but ARCHITECTURE.md Pattern 2 recommends "Portfolio as Pure Calculation Module (No DB Storage)". This research follows STATE.md as the locked user decision. ROADMAP and REQUIREMENTS both specify a positions table in success criteria.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rusqlite | (existing) | SQLite database interface | Already in use for all DB operations, proven migration pattern exists |
| PRAGMA user_version | SQLite built-in | Schema version tracking | Lightweight integer at fixed offset, no migration table overhead |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| sqlite.sql | (schema file) | Base schema definitions | CREATE TABLE IF NOT EXISTS pattern for schema initialization |
| DbError | (existing) | Typed error handling | Migration failures, constraint violations |

**Installation:**
No new dependencies needed - all functionality exists in current stack.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── db.rs                # EXTEND: add migrate_v2(), update DbTradeRow
├── error.rs             # (no change needed - DbError already exists)
schema/
└── sqlite.sql           # EXTEND: add positions table CREATE statement
```

### Pattern 1: Idempotent Migration with Duplicate Column Handling

**What:** Migration functions check user_version and catch duplicate column errors to allow re-running without failure.

**When to use:** Schema changes that add columns or tables to existing databases, especially when deployment may run migration multiple times.

**Example:**
```rust
// Source: Existing migrate_v1 pattern in capitoltraders_lib/src/db.rs:66-78
fn migrate_v2(&self) -> Result<(), DbError> {
    for sql in &[
        "ALTER TABLE trades ADD COLUMN trade_date_price REAL",
        "ALTER TABLE trades ADD COLUMN current_price REAL",
        "ALTER TABLE trades ADD COLUMN price_enriched_at TEXT",
        "ALTER TABLE trades ADD COLUMN estimated_shares INTEGER",
        "ALTER TABLE trades ADD COLUMN estimated_value REAL",
    ] {
        match self.conn.execute(sql, []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("duplicate column name") =>
            {
                // Column already exists (migration already ran) - skip silently
            }
            Err(e) => return Err(DbError::Migration(e.to_string())),
        }
    }
    Ok(())
}
```

**Pattern details:**
- Execute each ALTER TABLE statement individually (not batch) to catch specific errors
- Match on `rusqlite::Error::SqliteFailure` with message containing "duplicate column name"
- Silently skip when column exists (idempotency)
- Return error on any other failure (halt migration)

### Pattern 2: PRAGMA user_version Versioning

**What:** Use SQLite's built-in user_version integer to track schema version, checking before applying migrations.

**When to use:** All schema changes that need to execute once per database lifetime.

**Example:**
```rust
// Source: Existing migration pattern in capitoltraders_lib/src/db.rs:51-64
fn apply_migrations(&self) -> Result<(), DbError> {
    let version: i32 = self
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        self.migrate_v1()?;
        self.conn.pragma_update(None, "user_version", 1)?;
    }

    if version < 2 {
        self.migrate_v2()?;
        self.conn.pragma_update(None, "user_version", 2)?;
    }

    let schema = include_str!("../../schema/sqlite.sql");
    self.conn.execute_batch(schema)?;

    Ok(())
}
```

**Key insight:** user_version is "just an integer at fixed offset in the SQLite file" - much faster than querying a migrations table. Supports ~2 billion migrations (i32 field).

### Pattern 3: Nullable Columns for Gradual Enrichment

**What:** New price columns default to NULL, allowing trades to exist before enrichment and enabling resumable batch processing.

**When to use:** Data that is populated asynchronously or in batches after initial record creation.

**Example:**
```sql
-- All new columns are nullable (no NOT NULL constraint)
ALTER TABLE trades ADD COLUMN trade_date_price REAL;
ALTER TABLE trades ADD COLUMN current_price REAL;
ALTER TABLE trades ADD COLUMN price_enriched_at TEXT;
ALTER TABLE trades ADD COLUMN estimated_shares INTEGER;
ALTER TABLE trades ADD COLUMN estimated_value REAL;
```

**Why nullable:**
- Existing trades have no price data initially
- Enrichment pipeline can resume by querying WHERE trade_date_price IS NULL
- No need for DEFAULT 0.0 (NULL is semantically correct - "not yet enriched")
- SQLite automatically fills with NULL for existing rows when ALTER TABLE adds column

### Pattern 4: Positions Table with REAL for Fractional Shares

**What:** Store portfolio positions with REAL (float64) for shares_held and cost_basis to handle fractional shares and dividends.

**When to use:** Stock holdings that may include fractional shares (DRIP, stock splits, fractional trading).

**Example:**
```sql
CREATE TABLE IF NOT EXISTS positions (
    politician_id TEXT NOT NULL,
    issuer_ticker TEXT NOT NULL,
    shares_held REAL NOT NULL,
    cost_basis REAL NOT NULL,
    realized_pnl REAL NOT NULL DEFAULT 0.0,
    last_updated TEXT NOT NULL,
    PRIMARY KEY (politician_id, issuer_ticker),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);
```

**Schema decisions:**
- `shares_held REAL NOT NULL` - fractional shares possible (splits, DRIP), cannot be null
- `cost_basis REAL NOT NULL` - total dollar cost for remaining shares, zero for fully sold positions
- `realized_pnl REAL NOT NULL DEFAULT 0.0` - accumulates profit/loss from closed lots
- `last_updated TEXT NOT NULL` - ISO 8601 timestamp of last position recalculation
- Composite primary key on (politician_id, issuer_ticker) - one position per politician per ticker
- Foreign key cascade delete - remove positions when politician removed

### Anti-Patterns to Avoid

**Anti-Pattern 1: Using INTEGER for Share Counts**
- Stock splits create fractional shares (e.g., 3-for-2 split: 100 shares -> 150 shares)
- Dividend reinvestment plans (DRIP) create fractional shares
- Modern brokers support fractional trading
- **Instead:** Use REAL for shares_held, estimated_shares to preserve precision

**Anti-Pattern 2: Batch ALTER TABLE Statements**
- SQLite execute_batch() runs all statements in transaction, failing atomically
- Cannot catch individual "duplicate column name" errors
- **Instead:** Execute each ALTER TABLE individually, catch errors per statement

**Anti-Pattern 3: Adding NOT NULL Columns to Existing Tables**
- SQLite requires default value for NOT NULL columns when adding to populated tables
- Forces choosing arbitrary default (0.0) when NULL is semantically correct
- **Instead:** Use nullable columns for gradual enrichment, enforce NOT NULL in new tables only

**Anti-Pattern 4: Storing Timestamps as INTEGER Unix Epoch**
- Less human-readable in direct DB queries (1707494400 vs "2024-02-09T12:00:00Z")
- Requires conversion functions for date arithmetic
- No native ISO 8601 parsing
- **Instead:** Use TEXT with ISO 8601 format (YYYY-MM-DD HH:MM:SS.SSS), sortable and compatible with datetime('now')

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Schema version tracking | Custom migrations table | PRAGMA user_version | Built-in integer at fixed offset, no table overhead, faster DB open |
| Migration idempotency | IF NOT EXISTS for columns | Catch "duplicate column name" error | SQLite lacks ALTER TABLE IF NOT EXISTS, error handling is standard workaround |
| Timestamp storage format | Custom epoch seconds | TEXT with ISO 8601 | SQLite datetime() functions expect ISO 8601, sortable as strings, human-readable |
| Decimal precision for shares | Custom fixed-point integers | REAL with validation | SQLite REAL is IEEE 754 float64 (15-17 digit precision), sufficient for share counts |

**Key insight:** SQLite's built-in facilities (user_version, TEXT datetime, REAL floats) are designed for common use cases. Custom solutions add complexity without benefits for this scale (100K trades).

## Common Pitfalls

### Pitfall 1: Forgetting to Update DbTradeRow Struct

**What goes wrong:** New columns added to schema but DbTradeRow doesn't include them, causing silent data loss or runtime errors on query.

**Why it happens:** Migration code is separate from query mapping code - easy to update one without the other.

**How to avoid:**
- Add new fields to DbTradeRow immediately after migration function
- Update query_trades() SELECT statement to include new columns
- Add mapping in row iteration code: `row.get(idx)?`
- Write test: insert trade, migrate, query, verify new fields present

**Warning signs:**
- Test failures with "column not found" errors
- Query returns rows but new fields are missing/wrong
- Serde JSON output missing expected fields

### Pitfall 2: Re-running Migration on Already-Migrated DB Without Idempotency

**What goes wrong:** Migration fails with "duplicate column name" error, halting application startup.

**Why it happens:** SQLite has no ALTER TABLE IF NOT EXISTS, migration code doesn't handle duplicate columns.

**How to avoid:**
- Match on `rusqlite::Error::SqliteFailure(_, Some(ref msg))` where msg contains "duplicate column name"
- Silently skip when column exists (not an error condition)
- Test: run migration twice, verify second run succeeds without adding columns again

**Warning signs:**
- Application crashes on startup after re-deployment
- Error message contains "duplicate column name"
- user_version incremented but columns weren't added (partial failure)

### Pitfall 3: Using NOT NULL for Gradually Enriched Columns

**What goes wrong:** Cannot insert trades without price data, or forced to use meaningless default (0.0) that looks like valid data.

**Why it happens:** Misunderstanding of when to use NOT NULL - appropriate for always-available data, not optional enrichments.

**How to avoid:**
- Use NULL for columns populated asynchronously
- Add NOT NULL only when data is available at insert time
- Query unenriched trades with WHERE column IS NULL
- Validate in application logic, not database constraints

**Warning signs:**
- Migration fails: "Cannot add a NOT NULL column with default value NULL"
- Database filled with default 0.0 values that mean "not enriched"
- Cannot distinguish "price is zero" from "price not fetched"

### Pitfall 4: Forgetting user_version Increment After Migration

**What goes wrong:** Migration runs every time database opens, attempting to add columns repeatedly (hitting duplicate column errors).

**Why it happens:** Migration code executes conditional check but forgets to update version after success.

**How to avoid:**
- Immediately follow migrate_vN() with pragma_update(None, "user_version", N)
- Pattern: `if version < N { migrate_vN()?; update_version(N)?; }`
- Test: check user_version after migration completes

**Warning signs:**
- Migration code runs on every DB open (performance degradation)
- Logs filled with "duplicate column name" skip messages
- user_version stuck at old value after migration should have run

### Pitfall 5: Positions Table Without CASCADE DELETE

**What goes wrong:** Deleting politician leaves orphaned positions, causing foreign key constraint violations or stale data.

**Why it happens:** Forgot FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE.

**How to avoid:**
- All foreign keys should specify ON DELETE CASCADE or ON DELETE SET NULL
- Test: insert politician + positions, delete politician, verify positions removed
- Use PRAGMA foreign_keys = ON to enforce constraints

**Warning signs:**
- Foreign key constraint errors on DELETE
- Positions table grows indefinitely (dead rows for deleted politicians)
- Query joins return null politician names for valid positions

## Code Examples

Verified patterns from existing codebase:

### Migration Function (Based on migrate_v1)
```rust
// Source: capitoltraders_lib/src/db.rs:66-78
fn migrate_v2(&self) -> Result<(), DbError> {
    for sql in &[
        "ALTER TABLE trades ADD COLUMN trade_date_price REAL",
        "ALTER TABLE trades ADD COLUMN current_price REAL",
        "ALTER TABLE trades ADD COLUMN price_enriched_at TEXT",
        "ALTER TABLE trades ADD COLUMN estimated_shares INTEGER",
        "ALTER TABLE trades ADD COLUMN estimated_value REAL",
    ] {
        match self.conn.execute(sql, []) {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("duplicate column name") =>
            {
                // Column already exists - migration already ran
            }
            Err(e) => return Err(DbError::Migration(e.to_string())),
        }
    }
    Ok(())
}
```

### Applying Migration with Version Check
```rust
// Source: capitoltraders_lib/src/db.rs:51-64 (modified for v2)
fn apply_migrations(&self) -> Result<(), DbError> {
    let version: i32 = self
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < 1 {
        self.migrate_v1()?;
        self.conn.pragma_update(None, "user_version", 1)?;
    }

    if version < 2 {
        self.migrate_v2()?;
        self.conn.pragma_update(None, "user_version", 2)?;
    }

    let schema = include_str!("../../schema/sqlite.sql");
    self.conn.execute_batch(schema)?;

    Ok(())
}
```

### Updated DbTradeRow Struct
```rust
// Source: capitoltraders_lib/src/db.rs:1625-1645 (extended)
#[derive(Debug, Clone, Serialize)]
pub struct DbTradeRow {
    pub tx_id: i64,
    pub pub_date: String,
    pub tx_date: String,
    pub tx_type: String,
    pub value: i64,
    pub price: Option<f64>,
    pub size: Option<i64>,
    pub filing_url: String,
    pub reporting_gap: i64,
    pub enriched_at: Option<String>,

    // NEW: Price enrichment fields
    pub trade_date_price: Option<f64>,
    pub current_price: Option<f64>,
    pub price_enriched_at: Option<String>,
    pub estimated_shares: Option<i64>,
    pub estimated_value: Option<f64>,

    pub politician_name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub issuer_name: String,
    pub issuer_ticker: String,
    pub asset_type: String,
    pub committees: Vec<String>,
    pub labels: Vec<String>,
}
```

### Positions Table Schema
```sql
-- Add to schema/sqlite.sql
CREATE TABLE IF NOT EXISTS positions (
    politician_id TEXT NOT NULL,
    issuer_ticker TEXT NOT NULL,
    shares_held REAL NOT NULL,
    cost_basis REAL NOT NULL,
    realized_pnl REAL NOT NULL DEFAULT 0.0,
    last_updated TEXT NOT NULL,
    PRIMARY KEY (politician_id, issuer_ticker),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_positions_politician ON positions(politician_id);
CREATE INDEX IF NOT EXISTS idx_positions_ticker ON positions(issuer_ticker);
CREATE INDEX IF NOT EXISTS idx_positions_updated ON positions(last_updated);
```

### Query Trades with New Columns
```rust
// Pattern: Extend existing query_trades SELECT statement
let mut stmt = self.conn.prepare(
    "SELECT
        t.tx_id, t.pub_date, t.tx_date, t.tx_type, t.value,
        t.price, t.size, t.filing_url, t.reporting_gap, t.enriched_at,
        t.trade_date_price, t.current_price, t.price_enriched_at,
        t.estimated_shares, t.estimated_value,
        p.first_name || ' ' || p.last_name AS politician_name,
        p.party, p.state_id, p.chamber,
        i.issuer_name, i.issuer_ticker,
        a.asset_type,
        GROUP_CONCAT(DISTINCT tc.committee) AS committees,
        GROUP_CONCAT(DISTINCT tl.label) AS labels
     FROM trades t
     JOIN politicians p ON t.politician_id = p.politician_id
     JOIN issuers i ON t.issuer_id = i.issuer_id
     JOIN assets a ON t.asset_id = a.asset_id
     LEFT JOIN trade_committees tc ON t.tx_id = tc.tx_id
     LEFT JOIN trade_labels tl ON t.tx_id = tl.tx_id
     WHERE [filter conditions]
     GROUP BY t.tx_id"
)?;

// Row mapping (extend existing pattern)
let row = DbTradeRow {
    tx_id: row.get(0)?,
    pub_date: row.get(1)?,
    tx_date: row.get(2)?,
    tx_type: row.get(3)?,
    value: row.get(4)?,
    price: row.get(5)?,
    size: row.get(6)?,
    filing_url: row.get(7)?,
    reporting_gap: row.get(8)?,
    enriched_at: row.get(9)?,
    trade_date_price: row.get(10)?,
    current_price: row.get(11)?,
    price_enriched_at: row.get(12)?,
    estimated_shares: row.get(13)?,
    estimated_value: row.get(14)?,
    politician_name: row.get(15)?,
    party: row.get(16)?,
    state: row.get(17)?,
    chamber: row.get(18)?,
    issuer_name: row.get(19)?,
    issuer_ticker: row.get(20)?,
    asset_type: row.get(21)?,
    committees: parse_group_concat(row.get(22)?),
    labels: parse_group_concat(row.get(23)?),
};
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Migration tables (schema_migrations) | PRAGMA user_version | 2020+ | Faster DB open, simpler migration code, no table overhead |
| INTEGER Unix timestamps | TEXT ISO 8601 | SQLite best practice | Human-readable, sortable as strings, compatible with datetime() |
| IF NOT EXISTS for columns | Error handling for duplicates | N/A (SQLite never supported) | Idempotent migrations without native support |
| Strict INTEGER for shares | REAL for fractional shares | Modern trading (2018+) | Handles splits, DRIP, fractional trading |

**Deprecated/outdated:**
- Migration tables: rusqlite_migration crate still uses this but acknowledges user_version is lighter
- DEFAULT 0.0 for gradually enriched columns: NULL is semantically correct for "not yet enriched"
- STRICT tables in SQLite 3.37+: Interesting but not widely adopted, type affinity is sufficient

## Open Questions

1. **Positions Table Update Frequency**
   - What we know: Positions are materialized (not computed on-demand)
   - What's unclear: How often to recalculate positions (on every trade sync? on demand? scheduled?)
   - Recommendation: Phase 1 creates table, Phase 5 determines update strategy. Table includes last_updated for staleness detection.

2. **Handling Option Trades in Positions**
   - What we know: REQ-P4 says exclude options from stock position calculations
   - What's unclear: Should positions table store option positions separately, or exclude entirely?
   - Recommendation: Positions table is for stocks only (asset_type = 'stock'), options handled separately in Phase 5. Filter on JOIN: WHERE a.asset_type = 'stock'.

3. **Migration Testing with Real DBs**
   - What we know: Test exists for migrate_v1 (test_migration_on_existing_db)
   - What's unclear: Need to test v1->v2 migration with production-scale DB?
   - Recommendation: Unit test with fixture DB (~100 trades), integration test with copy of real DB if available.

4. **Positions Table Indexes**
   - What we know: Need to query by politician_id and issuer_ticker
   - What's unclear: Also need composite index on (politician_id, issuer_ticker) or is PK sufficient?
   - Recommendation: PRIMARY KEY creates unique index on (politician_id, issuer_ticker). Add separate indexes on politician_id and issuer_ticker for partial queries. Add index on last_updated for staleness queries.

## Sources

### Primary (HIGH confidence)
- [Capitol Traders db.rs](file:///Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs) - Existing migrate_v1 pattern, DbTradeRow struct, query_trades implementation
- [SQLite ALTER TABLE Documentation](https://www.sqlite.org/lang_altertable.html) - Official syntax and constraints (HIGH confidence)
- [SQLite Date And Time Functions](https://sqlite.org/lang_datefunc.html) - ISO 8601 format specification (HIGH confidence)

### Secondary (MEDIUM confidence)
- [SQLite ALTER TABLE ADD COLUMN IF NOT EXISTS Workaround](https://www.w3tutorials.net/blog/alter-table-add-column-if-not-exists-in-sqlite/) - Duplicate column error handling pattern
- [SQLite Versioning and Migration Strategies](https://www.sqliteforum.com/p/sqlite-versioning-and-migration-strategies) - PRAGMA user_version best practices
- [rusqlite_migration crate docs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/) - User_version approach validation
- [Handling Timestamps in SQLite](https://blog.sqlite.ai/handling-timestamps-in-sqlite) - TEXT ISO 8601 recommendation
- [SQLite Floating Point Numbers](https://sqlite.org/floatingpoint.html) - REAL precision for financial data

### Tertiary (LOW confidence - context only)
- [Stack Overflow: SQLite duplicate column name](https://stackoverflow.com/questions/tagged/sqlite+alter-table) - Community error handling patterns
- [Medium: Stock Prices in SQLite](https://medium.com/cassandra-cryptoassets/download-and-store-stock-prices-using-python-and-sqlite-e5fa0ea372cc) - Schema examples for trading data

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - reusing existing rusqlite, proven migrate_v1 pattern
- Architecture: HIGH - extending existing migration system, SQLite features well-documented
- Pitfalls: MEDIUM - migration pitfalls documented, positions table is new (no project history)

**Research date:** 2026-02-09
**Valid until:** 60 days (stable technology - SQLite, rusqlite)

**Notes:**
- No CONTEXT.md exists - no user constraints to honor
- Contradiction between STATE.md (materialized positions table) and ARCHITECTURE.md (computed on-demand) - followed STATE.md as user decision
- All research based on existing Capitol Traders codebase patterns
