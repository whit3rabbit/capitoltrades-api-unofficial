---
phase: 14-benchmark-price-enrichment
plan: 01
subsystem: database
tags: [schema, migration, benchmark, enrichment]
dependency_graph:
  requires: [13-02-gics-sector-mapping]
  provides: [schema-v7, benchmark-db-methods]
  affects: [trades-table, db-init]
tech_stack:
  added: []
  patterns: [migration-v7, benchmark-enrichment-row]
key_files:
  created: []
  modified:
    - schema/sqlite.sql
    - capitoltraders_lib/src/db.rs
decisions:
  - title: Separate BenchmarkEnrichmentRow from PriceEnrichmentRow
    rationale: Benchmark enrichment has different data needs (needs gics_sector, does NOT need size_range_low/high/value). Separate struct avoids coupling and makes intent clear.
    alternatives: [extend PriceEnrichmentRow with optional gics_sector field]
  - title: Benchmark enrichment does not touch price_enriched_at
    rationale: Benchmark price enrichment is independent of trade price enrichment. Trades can have benchmark prices without trade_date_price, or vice versa. Separate concerns.
    alternatives: [reuse price_enriched_at for both types of enrichment]
metrics:
  duration_minutes: 4.7
  tasks_completed: 2
  tests_added: 6
  files_modified: 2
  commits: 2
  completed_at: "2026-02-15T14:55:34Z"
---

# Phase 14 Plan 01: Schema v7 Migration and Benchmark DB Methods Summary

Schema v7 migration adds benchmark_price column to trades table with supporting DB methods for benchmark price enrichment.

## What Was Built

### Schema Changes (v7)
- Added `benchmark_price REAL` column to trades table
- Added `idx_trades_benchmark_price` index for query performance
- Migration handles both fresh DBs (via base schema) and existing DBs (via migrate_v7)

### DB Methods
- `migrate_v7()`: ALTER TABLE trades ADD COLUMN benchmark_price with idempotent error handling
- `get_benchmark_unenriched_trades()`: Returns trades WHERE benchmark_price IS NULL with gics_sector from issuers JOIN
- `update_benchmark_price()`: Writes benchmark_price for single trade (does NOT touch price_enriched_at)

### Data Structures
- `BenchmarkEnrichmentRow`: Separate from PriceEnrichmentRow with fields tx_id, issuer_ticker, tx_date, gics_sector

### Tests (6 new)
1. `test_migrate_v7_from_v6`: Verify v6-to-v7 migration adds column, index works, data preserved
2. `test_migrate_v7_idempotent`: Verify calling init() twice on v7 DB does not error
3. `test_v7_version_check`: Verify fresh DB has user_version = 7
4. `test_get_benchmark_unenriched_trades`: Verify query returns trades with NULL benchmark_price, includes gics_sector from JOIN, excludes enriched trades
5. `test_get_benchmark_unenriched_trades_with_limit`: Verify limit parameter works
6. `test_update_benchmark_price`: Verify writing Some(f64) and None both work

## Technical Implementation

**Migration Pattern (v7):**
```rust
fn migrate_v7(&self) -> Result<(), DbError> {
    // ALTER TABLE with duplicate column name error handling
    match self.conn.execute("ALTER TABLE trades ADD COLUMN benchmark_price REAL", []) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
            if msg.contains("duplicate column name") || msg.contains("no such table") => {}
        Err(e) => return Err(e.into()),
    }
    // CREATE INDEX with no-such-table error handling
    match self.conn.execute("CREATE INDEX IF NOT EXISTS idx_trades_benchmark_price ON trades(benchmark_price)", []) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(_, Some(ref msg))) if msg.contains("no such table") => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
```

**Query Pattern:**
```sql
SELECT t.tx_id, i.issuer_ticker, t.tx_date, i.gics_sector
FROM trades t
JOIN issuers i ON t.issuer_id = i.issuer_id
WHERE i.issuer_ticker IS NOT NULL
  AND i.issuer_ticker <> ''
  AND t.tx_date IS NOT NULL
  AND t.benchmark_price IS NULL
ORDER BY t.tx_id
```

**Update Pattern:**
```sql
UPDATE trades SET benchmark_price = ?1 WHERE tx_id = ?2
```

## Deviations from Plan

None - plan executed exactly as written.

## Verification Results

- Schema v7 migration adds benchmark_price column to trades table: VERIFIED
- BenchmarkEnrichmentRow type defined with tx_id, issuer_ticker, tx_date, gics_sector fields: VERIFIED
- get_benchmark_unenriched_trades returns trades needing benchmark enrichment with sector data from issuers JOIN: VERIFIED
- update_benchmark_price writes benchmark price for individual trades: VERIFIED
- All tests pass including 6 new tests: VERIFIED (519 total workspace tests)
- Clippy clean: VERIFIED (0 warnings)

## Files Changed

### schema/sqlite.sql
- Added benchmark_price REAL column to trades CREATE TABLE (line 69)
- Added idx_trades_benchmark_price index (line 245)

### capitoltraders_lib/src/db.rs
- Added migrate_v7 method after migrate_v6 (lines 328-351)
- Called migrate_v7 from init() for version < 7 (lines 91-94)
- Added BenchmarkEnrichmentRow struct after PriceEnrichmentRow (lines 3479-3484)
- Added get_benchmark_unenriched_trades method (lines 1532-1572)
- Added update_benchmark_price method (lines 1574-1586)
- Added 6 new tests (lines 8652-8849)
- Updated 15 version assertions from 6 to 7 across existing tests

## Commits

- 12e8d51: feat(14-01): add schema v7 migration and benchmark DB methods
- c96ac7a: test(14-01): add schema v7 migration and benchmark DB method tests

## Self-Check: PASSED

All files and commits verified:
- FOUND: schema/sqlite.sql
- FOUND: capitoltraders_lib/src/db.rs
- FOUND: 12e8d51 (feat commit)
- FOUND: c96ac7a (test commit)

## Next Steps

Plan 14-02 will implement the benchmark price enrichment pipeline that uses these DB methods to fetch SPY and sector ETF prices for each trade.
