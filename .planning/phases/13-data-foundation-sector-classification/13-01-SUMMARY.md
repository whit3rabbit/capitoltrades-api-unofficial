---
phase: 13-data-foundation-sector-classification
plan: 01
subsystem: data-foundation
tags: [schema, migration, gics-sectors, benchmarks, infrastructure]
dependency_graph:
  requires: [schema-v5, employer-tables]
  provides: [schema-v6, gics-sector-column, sector-benchmarks-table, benchmark-reference-data]
  affects: [issuers-table, sector-classification-pipeline]
tech_stack:
  added: [GICS-sector-ETFs]
  patterns: [versioned-schema-migration, reference-data-population, idempotent-upserts]
key_files:
  created: []
  modified:
    - capitoltraders_lib/src/db.rs: "migrate_v6, populate_sector_benchmarks, get_sector_benchmarks, get_top_traded_tickers + 7 tests"
    - schema/sqlite.sql: "gics_sector column, sector_benchmarks table, idx_issuers_gics_sector index"
decisions:
  - decision: "Use 11 GICS sector-specific SPDR ETFs plus SPY for market benchmark"
    rationale: "SPDR Select Sector funds are liquid, widely-tracked, and map directly to GICS Level 1 sectors"
    alternatives: ["Vanguard sector ETFs (VGT, VHT, etc) - lower expense ratios but less liquidity", "iShares sector ETFs - comparable but SPDR has longer history"]
  - decision: "Store benchmarks in dedicated sector_benchmarks table vs hardcoding"
    rationale: "Database table enables future extensibility (custom benchmarks, user-defined sectors) and keeps reference data queryable"
    alternatives: ["Hardcode in Rust constants - simpler but less flexible"]
  - decision: "Call populate_sector_benchmarks in init() after schema.sql execution"
    rationale: "Ensures reference data always present, migrations are self-contained, and fresh DBs work immediately"
    alternatives: ["Lazy population on first query - adds complexity, delays error detection"]
metrics:
  duration_mins: 6
  completed_at: "2026-02-15T03:47:32Z"
  tasks_completed: 2
  tests_added: 7
  total_tests: 520
  files_modified: 2
  lines_added: 380
  lines_removed: 20
---

# Phase 13 Plan 01: Schema v6 Migration - GICS Sector Infrastructure

**One-liner:** Schema v6 migration adds GICS sector classification column to issuers table and sector_benchmarks reference table with 12 ETF benchmarks (SPY + 11 GICS sector SPDRs)

## Overview

Established the data foundation for v1.3 sector-relative analytics by adding GICS sector infrastructure. Schema v6 migration adds gics_sector column to issuers table, creates sector_benchmarks reference table, and populates 12 benchmark ETF definitions (S&P 500 market benchmark + 11 GICS Level 1 sector benchmarks).

This migration enables Phase 13 Plan 02 (sector mapping module) and Phase 14 (benchmark price enrichment).

## What Was Built

### Schema v6 Migration

**Migration Method:** `migrate_v6()`
- ALTER TABLE issuers ADD COLUMN gics_sector TEXT (with duplicate column + no such table error handling)
- CREATE TABLE sector_benchmarks (sector TEXT PRIMARY KEY, etf_ticker TEXT NOT NULL, etf_name TEXT NOT NULL)
- CREATE INDEX idx_issuers_gics_sector ON issuers(gics_sector) (with no such table error handling)

**Reference Data Population:** `populate_sector_benchmarks()`
- Idempotent insert (checks COUNT(*) first, returns early if > 0)
- 12 benchmark rows:
  - Market: SPY (SPDR S&P 500 ETF Trust)
  - Communication Services: XLC
  - Consumer Discretionary: XLY
  - Consumer Staples: XLP
  - Energy: XLE
  - Financials: XLF
  - Health Care: XLV
  - Industrials: XLI
  - Information Technology: XLK
  - Materials: XLB
  - Real Estate: XLRE
  - Utilities: XLU

### Query Methods

**`get_sector_benchmarks(&self) -> Result<Vec<(String, String, String)>>`**
- Returns all 12 benchmarks as (sector, etf_ticker, etf_name) tuples
- Ordered alphabetically by sector name
- Used by future benchmark price enrichment pipeline

**`get_top_traded_tickers(&self, limit: usize) -> Result<Vec<(String, i64)>>`**
- Returns top N most-traded tickers across all trades
- JOIN trades + issuers, GROUP BY ticker, ORDER BY count DESC
- Enables popularity analysis for sector classification priority

### Base Schema Update

Updated `schema/sqlite.sql` for fresh database initialization:
- issuers table now includes gics_sector TEXT column
- sector_benchmarks table included in base schema
- idx_issuers_gics_sector index included

### Test Coverage (7 new tests, 520 total)

1. **test_migrate_v6_from_v5:** Verify v5 to v6 migration creates gics_sector column, sector_benchmarks table, can INSERT/SELECT with new column
2. **test_migrate_v6_idempotent:** Double init() is safe, version stays 6
3. **test_v6_version_check:** Fresh DB has version 6
4. **test_fresh_db_has_sector_benchmarks:** Fresh DB has exactly 12 benchmark rows
5. **test_sector_benchmarks_populated_once:** Calling populate_sector_benchmarks() twice doesn't duplicate rows (still 12 total)
6. **test_get_sector_benchmarks:** Returns 12 rows, SPY and XLK present with correct metadata, alphabetically ordered
7. **test_get_top_traded_tickers:** Respects limit parameter, returns descending count order

All 520 tests pass (513 existing + 7 new).

## Implementation Notes

### Migration Error Handling Pattern

Both ALTER TABLE and CREATE INDEX in migrate_v6() handle "no such table" errors gracefully. This allows migrations to run before schema.sql creates base tables (important for test isolation and migration-first init flow).

### Populate Timing

`populate_sector_benchmarks()` is called in `init()` AFTER `execute_batch(schema.sql)` to ensure sector_benchmarks table exists. The method also handles "no such table" errors during the COUNT(*) check for extra safety during migrations.

### Version Bump Impact

Updating schema version from 5 to 6 required updating 12 existing tests that asserted `version == 5`. All updated to `version == 6`. This is expected for schema versioning.

### GICS Sector Selection

11 GICS Level 1 sectors match the industry-standard Global Industry Classification Standard. SPDR Select Sector ETFs chosen for:
- High liquidity (all have >$1B AUM)
- Direct GICS sector mapping
- Long trading history (enables historical comparisons)
- Institutional adoption (widely used as sector benchmarks)

## Deviations from Plan

None - plan executed exactly as written.

## Dependencies Satisfied

**Requires:**
- schema-v5 (employer_mappings and employer_lookup tables)

**Provides:**
- schema-v6 migration
- gics_sector column on issuers table
- sector_benchmarks reference table
- 12 benchmark ETF definitions

**Affects:**
- issuers table (new nullable column)
- Future sector classification pipeline (Phase 13 Plan 02)
- Future benchmark price enrichment (Phase 14)

## Next Steps

Phase 13 Plan 02 will create the sector mapping module (NAICS/SIC to GICS sector mapping logic) using this infrastructure.

---

## Self-Check: PASSED

All created artifacts verified:

**Schema modifications:**
```bash
# gics_sector column exists in base schema
grep -q "gics_sector TEXT" schema/sqlite.sql
# sector_benchmarks table exists in base schema
grep -q "CREATE TABLE IF NOT EXISTS sector_benchmarks" schema/sqlite.sql
# Index exists in base schema
grep -q "idx_issuers_gics_sector" schema/sqlite.sql
```

**Migration method:**
```bash
# migrate_v6 exists in db.rs
grep -q "fn migrate_v6" capitoltraders_lib/src/db.rs
# populate_sector_benchmarks exists
grep -q "fn populate_sector_benchmarks" capitoltraders_lib/src/db.rs
# Query methods exist
grep -q "pub fn get_sector_benchmarks" capitoltraders_lib/src/db.rs
grep -q "pub fn get_top_traded_tickers" capitoltraders_lib/src/db.rs
```

**Commits:**
- fef9091: feat(13-01): add schema v6 migration with GICS sector infrastructure
- 5fe1a7c: test(13-01): add schema v6 migration and benchmark tests

All files modified as planned. All tests pass. All verification criteria met.
