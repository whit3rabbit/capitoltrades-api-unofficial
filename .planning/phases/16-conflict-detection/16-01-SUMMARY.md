---
phase: 16-conflict-detection
plan: 01
subsystem: conflict-detection
tags: [committee-jurisdiction, conflict-scoring, yaml-seed-data]
dependency_graph:
  requires: [phase-13-benchmark-enrichment, phase-15-performance-scoring]
  provides: [committee-sector-mapping, committee-trading-score, conflict-types]
  affects: [analytics-module]
tech_stack:
  added: []
  patterns: [compile-time-yaml-validation, pure-scoring-functions, hashset-deduplication]
key_files:
  created:
    - seed_data/committee_sectors.yml
    - capitoltraders_lib/src/committee_jurisdiction.rs
    - capitoltraders_lib/src/conflict.rs
  modified:
    - capitoltraders_lib/src/analytics.rs
    - capitoltraders_lib/src/lib.rs
    - capitoltraders_cli/src/commands/analytics.rs
decisions:
  - decision: Use committee short codes from CapitolTrades scrape data (e.g., "hsba", "ssfi") not full committee names
    rationale: Database stores short codes in politician_committees table; matching YAML format to DB format avoids mapping layer
  - decision: Propagate gics_sector from buy lot in FIFO matching, not sell transaction
    rationale: ClosedTrade represents the entire holding period; sector classification is determined at purchase time
  - decision: Exclude NULL gics_sector trades from committee trading score denominator
    rationale: Cannot determine committee relevance for unknown sectors; scoring should only consider trades with known sector classification
  - decision: Use HashSet for committee sector deduplication
    rationale: Politicians serve on multiple committees with overlapping jurisdictions; single trade should not be counted multiple times
metrics:
  duration_minutes: 7
  tasks_completed: 2
  files_created: 3
  files_modified: 3
  test_count: 15
  commits: 2
completed: 2026-02-15
---

# Phase 16 Plan 01: Committee Jurisdiction Mapping and Conflict Scoring Foundation

**Committee-sector jurisdiction mapping with GICS validation and pure conflict scoring computation.**

## Summary

Created committee-sector jurisdiction mapping YAML with 40+ congressional committee mappings and conflict scoring module with committee trading score calculation. Implemented compile-time YAML validation following Phase 13 sector_mapping.rs pattern exactly, with sector deduplication via HashSet for overlapping committee jurisdictions. Extended analytics.rs ClosedTrade to include gics_sector field propagated from FIFO buy lot for downstream conflict analysis. All workspace tests pass (591 total), clippy clean, no regressions.

## Deviations from Plan

None. Plan executed exactly as written. Both tasks completed successfully with all verification criteria met.

## Key Accomplishments

### Task 1: Committee Jurisdiction Mapping (Commit: 187deb9)
- Created `seed_data/committee_sectors.yml` with 40+ committee mappings:
  - 17 House committees (hsba, hsif, hsag, hspw, hsas, hssm, hswm, hsju, hsed, hsgo, hsfa, hsii, hssm, hsbu, hsru, hsha, hsvc)
  - 19 Senate committees (ssbk, sscm, sseg, sshr, spag, ssas, ssev, ssfi, ssju, ssfr, ssga, ssra, sssb, ssbu, ssva, ssia, slet, slin, scnc)
  - 4 House select committees (hlig)
  - Each mapping includes committee_name (short code), chamber, full_name, sectors array, notes
- Implemented `capitoltraders_lib/src/committee_jurisdiction.rs`:
  - CommitteeJurisdiction struct with Deserialize
  - load_committee_jurisdictions() with include_str! compile-time embedding
  - validate_committee_jurisdictions() reusing validate_sector() from sector_mapping.rs
  - get_committee_sectors() with HashSet deduplication for overlapping jurisdictions
  - 8 unit tests covering load, validation, deduplication, edge cases
- All sectors validated against GICS_SECTORS at compile time
- Chamber validation (House or Senate only)
- Unknown committee codes silently skipped (no error) for robustness
- Empty sectors arrays supported (e.g., Ways and Means, Judiciary)

### Task 2: Conflict Scoring Module (Commit: e178921)
- Created `capitoltraders_lib/src/conflict.rs`:
  - CommitteeTradingScore type with politician_id, committee_names, total_scored_trades, committee_related_trades, committee_trading_pct, disclaimer
  - DonationTradeCorrelation type (for Plan 02) with politician_id, ticker, matching_donor_count, avg_mapping_confidence, donor_employers, total_donation_amount
  - ConflictSummary type combining both signals
  - calculate_committee_trading_score() pure function
  - 7 unit tests covering basic scoring, edge cases, null sector handling, overlapping jurisdictions
- Extended analytics module for conflict analysis:
  - Added gics_sector: Option<String> to AnalyticsTrade, ClosedTrade, AnalyticsLot
  - Updated buy() method to capture gics_sector in lot
  - Updated sell() method to propagate gics_sector from buy lot to ClosedTrade
  - All 31 existing analytics tests still pass with gics_sector: None
- Updated CLI analytics command row_to_analytics_trade() to pass gics_sector from DB row
- Null sector handling: trades with None gics_sector excluded from both numerator and denominator
- Overlapping jurisdiction deduplication verified (Health Care counted once even when politician on both hsif and hsvc)
- Disclaimer field: "Based on current committee assignments; may not reflect assignment at trade time"

## Critical Implementation Details

### Committee Code Format
- YAML uses short codes ("hsba", "ssfi") matching politician_committees table
- Verified via `SELECT DISTINCT committee FROM politician_committees` query
- No mapping layer needed between YAML and DB queries

### GICS Sector Propagation
- ClosedTrade.gics_sector comes from buy lot, not sell transaction
- FIFO matching preserves sector classification from purchase time
- Enables committee trading score to work on closed trades without JOIN to issuers table

### Deduplication Logic
- get_committee_sectors() returns HashSet<String>
- Politician on House Energy and Commerce (hsif) + Veterans' Affairs (hsvc) has Health Care sector counted once
- Prevents inflated committee trading percentages from overlapping jurisdictions

## Testing Summary

All tests passing:
- 8 committee_jurisdiction tests (load, validate, dedup, edge cases)
- 7 conflict tests (basic, no committees, no trades, null sectors, overlapping, disclaimer, type)
- 31 analytics tests (existing tests unbroken after ClosedTrade extension)
- 591 total workspace tests (no regressions)
- Clippy clean (no warnings)

## Integration Points

### Upstream Dependencies
- Phase 13 sector_mapping.rs: GICS_SECTORS constant, validate_sector() function
- Phase 15 analytics.rs: ClosedTrade type extended with gics_sector

### Downstream Impact
- Plan 02 will use CommitteeTradingScore for DB queries and CLI output
- Plan 02 will use DonationTradeCorrelation for employer-trade correlation
- Plan 02 will use get_committee_sectors() for committee-related trade flagging

## Files Modified

**Created:**
- `seed_data/committee_sectors.yml` (220 lines, 40+ committee mappings)
- `capitoltraders_lib/src/committee_jurisdiction.rs` (282 lines, YAML loader + validation)
- `capitoltraders_lib/src/conflict.rs` (442 lines, scoring types + pure functions)

**Modified:**
- `capitoltraders_lib/src/analytics.rs` (added gics_sector to 3 structs, updated buy/sell methods, added field to all test constructors)
- `capitoltraders_lib/src/lib.rs` (added committee_jurisdiction and conflict module declarations + pub use exports)
- `capitoltraders_cli/src/commands/analytics.rs` (updated row_to_analytics_trade to pass gics_sector)

## Verification

```bash
cargo test -p capitoltraders_lib committee_jurisdiction -- --nocapture  # 8 passed
cargo test -p capitoltraders_lib conflict -- --nocapture                # 7 passed
cargo test -p capitoltraders_lib analytics -- --nocapture               # 31 passed
cargo test --workspace                                                  # 591 passed
cargo clippy --workspace                                                # clean
cargo check --workspace                                                 # clean
```

## Self-Check

Verifying created files and commits:

**Created files:**
```bash
[ -f "seed_data/committee_sectors.yml" ] && echo "FOUND: committee_sectors.yml" || echo "MISSING"
[ -f "capitoltraders_lib/src/committee_jurisdiction.rs" ] && echo "FOUND: committee_jurisdiction.rs" || echo "MISSING"
[ -f "capitoltraders_lib/src/conflict.rs" ] && echo "FOUND: conflict.rs" || echo "MISSING"
```
FOUND: committee_sectors.yml
FOUND: committee_jurisdiction.rs
FOUND: conflict.rs

**Commits:**
```bash
git log --oneline --all | grep -E "(187deb9|e178921)" && echo "FOUND commits" || echo "MISSING"
```
187deb9 feat(16-01): add committee jurisdiction mapping with GICS sector validation
e178921 feat(16-01): add conflict scoring module with committee trading score
FOUND commits

## Self-Check: PASSED

All created files exist, all commits present, all tests passing.
