---
phase: 12-employer-correlation-analysis
plan: 02
subsystem: employer-correlation
tags: [db, schema, migration, employer-mapping, donor-context]
dependency_graph:
  requires: [12-01-employer-mapping-module]
  provides: [schema-v5, employer-db-layer, donor-context-queries]
  affects: [donations-table, issuers-table]
tech_stack:
  added: [employer_mappings-table, employer_lookup-table]
  patterns: [employer-lookup-bridge, sql-joins, sector-aggregation]
key_files:
  created: []
  modified:
    - schema/sqlite.sql
    - capitoltraders_lib/src/db.rs
    - capitoltraders_lib/src/lib.rs
decisions:
  - issuer_ticker used as FK instead of issuer_id for cross-database portability
  - employer_lookup table enables SQL JOINs without Rust normalization calls
  - Donation summary includes ALL donations (total) + top 5 sectors from matched employers only
  - Schema version tests updated from 4 to 5 expectations
metrics:
  duration: 9min
  completed: 2026-02-14T01:36:37Z
  tasks: 2
  files: 3
  commits: 2
---

# Phase 12 Plan 02: Employer Mapping DB Layer Summary

Schema v5 migration with employer_mappings and employer_lookup tables, plus 8 DB operations for employer correlation and donor context queries.

## One-Liner

Schema v5 adds employer_mappings/employer_lookup tables with 8 DB methods for matching, unmatched employer queries, and donor context aggregation by sector.

## Tasks Completed

| Task | Name                                                  | Commit  | Key Changes                                                                 |
| ---- | ----------------------------------------------------- | ------- | --------------------------------------------------------------------------- |
| 1    | Schema v5 migration with employer tables              | beca064 | employer_mappings, employer_lookup tables + 4 indexes + 4 migration tests   |
| 2    | DB operations for employer mappings and donor context | d39ac46 | 8 DB methods, 3 types, 9 tests, version expectation updates                 |

## Deviations from Plan

None - plan executed exactly as written.

## Implementation Details

### Schema v5 Migration

**Tables added:**
- `employer_mappings`: normalized_employer (PK), issuer_ticker (FK), confidence, match_type, created_at, last_updated, notes
- `employer_lookup`: raw_employer_lower (PK), normalized_employer (FK to employer_mappings)

**Indexes added:**
- idx_employer_mappings_ticker (issuer_ticker)
- idx_employer_mappings_confidence (confidence)
- idx_employer_mappings_type (match_type)
- idx_employer_lookup_normalized (normalized_employer)

**Migration pattern:**
- migrate_v5() uses CREATE TABLE IF NOT EXISTS for idempotency
- Fresh databases include tables in base DDL (schema/sqlite.sql)
- Existing databases migrate via version check in init()

**Tests (4):**
- test_migrate_v5_from_v4: v4 to v5 upgrade
- test_fresh_db_has_employer_tables: both tables in fresh DB
- test_migrate_v5_idempotent: safe to run multiple times
- test_v5_version_check: version 5 after init

### DB Operations

**Methods added (8):**

1. `upsert_employer_mappings(&self, mappings: &[(String, String, f64, &str)]) -> Result<usize>`
   - Batch insert/update employer mappings
   - Uses unchecked_transaction for atomicity
   - datetime('now') for timestamps

2. `get_unmatched_employers(&self, limit: Option<i64>) -> Result<Vec<String>>`
   - Returns lowercased employer names not in employer_lookup
   - Filters NULL and empty strings
   - Caller normalizes before matching

3. `get_all_issuers_for_matching(&self) -> Result<Vec<(i64, String, String)>>`
   - Returns (issuer_id, issuer_name, issuer_ticker)
   - COALESCE(issuer_ticker, '') for missing tickers
   - Filters NULL and empty issuer_name

4. `issuer_exists_by_ticker(&self, ticker: &str) -> Result<bool>`
   - Single-query existence check
   - LIMIT 1 for performance

5. `get_employer_mapping_count(&self) -> Result<i64>`
   - Simple COUNT(*) query

6. `insert_employer_lookups(&self, lookups: &[(String, String)]) -> Result<()>`
   - Batch insert raw-to-normalized mappings
   - INSERT OR REPLACE for idempotency
   - Bridges raw donation employer strings to normalized keys

7. `get_donor_context_for_sector(&self, politician_id: &str, sector: &str, limit: i64) -> Result<Vec<DonorContext>>`
   - Top employers by donation amount for politician/sector
   - JOINs: donations -> donation_sync_meta -> employer_lookup -> employer_mappings -> issuers
   - Groups by contributor_employer (raw), sums donation_receipt_amount
   - Returns: employer name, total_amount, donation_count

8. `get_donation_summary(&self, politician_id: &str) -> Result<Option<DonationSummary>>`
   - Two-query pattern:
     - Query 1: SUM all donations for politician (includes non-mapped employers)
     - Query 2: Top 5 sectors from matched employers only
   - Returns None if no donations or NULL sum
   - Returns: total_amount, donation_count, top_sectors (Vec<SectorTotal>)

**Types added (3):**
- `DonorContext`: employer, total_amount, donation_count
- `SectorTotal`: sector, total_amount, employer_count
- `DonationSummary`: total_amount, donation_count, top_sectors

**Tests (9):**
- test_upsert_employer_mappings: insert 3 mappings
- test_upsert_employer_mappings_update: verify INSERT OR REPLACE updates ticker
- test_get_unmatched_employers: excludes employers in employer_lookup
- test_get_all_issuers_for_matching: returns all non-empty issuers
- test_issuer_exists_by_ticker: AAPL exists, ZZZZ does not
- test_get_employer_mapping_count: 0 initially, 2 after insert
- test_donor_context_empty: empty vec with no mappings
- test_donation_summary_no_donations: None with no donations
- test_employer_lookup_insert: round-trip verification

### Key Decisions

**issuer_ticker FK instead of issuer_id:**
- issuer_id values are database-specific (auto-increment)
- Employer mappings should work across re-synced databases
- Queries JOIN through issuers table to get issuer_id when needed

**employer_lookup bridge pattern:**
- Enables SQL JOINs without calling Rust normalization functions
- Raw donation employer strings map to normalized keys
- Populated by map-employers CLI (Plan 03) when processing employers

**Donation summary dual-source approach:**
- total_amount/donation_count includes ALL donations (even without employer matches)
- top_sectors only includes matched employers (requires employer_lookup + employer_mappings + issuers)
- Provides complete donation picture + sector breakdown from matchable data

**Schema version test updates:**
- 7 tests expected version 4 after init
- Updated to expect version 5 (current schema version)
- All migration tests now pass

## Testing

**Test suite:**
- 4 schema v5 migration tests (fresh DB, v4-to-v5, idempotency, version check)
- 9 DB operation tests (upsert, query, existence checks, empty state handling)
- All 143 DB tests pass
- 356 total workspace tests pass
- Clippy clean

**Key test patterns:**
- open_test_db() for in-memory DB with init()
- Raw SQL inserts for test fixture setup (politicians, donations, sync_meta)
- Verify empty state handling (donor_context_empty, donation_summary_no_donations)
- Round-trip verification (employer_lookup_insert)

## Integration

**Ready for Plan 03 (map-employers CLI):**
- get_unmatched_employers() provides batch of employers to match
- get_all_issuers_for_matching() provides matching candidates
- issuer_exists_by_ticker() validates matches
- upsert_employer_mappings() stores match results
- insert_employer_lookups() populates raw-to-normalized bridge

**Ready for Plan 04 (trades/portfolio donor context):**
- get_donor_context_for_sector() provides sector drill-down for trade pages
- get_donation_summary() provides politician-level donation overview for portfolio page

## Files Modified

**schema/sqlite.sql:**
- Added employer_mappings table DDL (7 columns)
- Added employer_lookup table DDL (2 columns)
- Added 4 indexes

**capitoltraders_lib/src/db.rs:**
- Added migrate_v5() function (39 lines)
- Updated init() to call migrate_v5 and set version 5
- Added 8 DB methods (223 lines)
- Added 3 types (DonorContext, SectorTotal, DonationSummary)
- Added 9 unit tests (227 lines)
- Updated 7 schema version assertions from 4 to 5

**capitoltraders_lib/src/lib.rs:**
- Added DonorContext, DonationSummary, SectorTotal to re-exports

## Performance Considerations

**Query optimization:**
- All JOINs use indexed columns (committee_id, politician_id, raw_employer_lower, issuer_ticker)
- LIMIT clauses on get_unmatched_employers and donor_context queries
- Single-query existence check (issuer_exists_by_ticker)

**Batch operations:**
- upsert_employer_mappings uses unchecked_transaction for batch insert
- insert_employer_lookups uses unchecked_transaction for batch insert
- Reduces DB round-trips

**Index coverage:**
- employer_mappings.issuer_ticker indexed (JOIN to issuers)
- employer_lookup.normalized_employer indexed (JOIN to employer_mappings)
- employer_lookup.raw_employer_lower indexed (JOIN from donations)

## Self-Check: PASSED

**Created files:**
- None (all modifications to existing files)

**Modified files:**
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/schema/sqlite.sql: EXISTS
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs: EXISTS
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/lib.rs: EXISTS

**Commits:**
- beca064 (Task 1): EXISTS in git log
- d39ac46 (Task 2): EXISTS in git log

**Verification commands:**
```bash
ls -l schema/sqlite.sql  # exists
ls -l capitoltraders_lib/src/db.rs  # exists
git log --oneline | grep beca064  # found
git log --oneline | grep d39ac46  # found
cargo test --workspace  # 356 tests pass
cargo clippy --workspace  # no warnings
```

All claims verified. Plan 02 complete.
