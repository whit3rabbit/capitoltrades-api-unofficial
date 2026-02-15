---
phase: 16-conflict-detection
plan: 02
subsystem: conflict-detection
tags: [cli-command, db-queries, donation-correlation, output-formatting]
dependency_graph:
  requires: [phase-16-plan-01, phase-10-donation-sync, phase-14-employer-mapping]
  provides: [conflicts-cli, donation-trade-correlation-query, conflict-output-functions]
  affects: [cli-interface]
tech_stack:
  added: []
  patterns: [db-query-methods, cli-subcommand, multi-format-output, csv-sanitization]
key_files:
  created:
    - capitoltraders_cli/src/commands/conflicts.rs
  modified:
    - capitoltraders_lib/src/db.rs
    - capitoltraders_cli/src/commands/mod.rs
    - capitoltraders_cli/src/main.rs
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/xml_output.rs
decisions:
  - decision: Use find_politician_by_name for --politician filter resolution
    rationale: Better UX than requiring politician_id; follows donation query pattern from Phase 11
  - decision: Separate output functions for conflicts and donation correlations
    rationale: Two distinct data types with different schemas; cleaner than unified function with conditional logic
  - decision: Print disclaimer to stderr in all output modes
    rationale: Ensures visibility regardless of stdout redirection; follows best practice for warnings
  - decision: CSV sanitization on donor_employers field only
    rationale: User-contributed content from FEC donations; politician names and committees are controlled data
metrics:
  duration_minutes: 7
  tasks_completed: 2
  files_created: 1
  files_modified: 5
  test_count: 4
  commits: 2
completed: 2026-02-15
---

# Phase 16 Plan 02: Conflicts CLI Subcommand and Donation-Trade Correlation

**Wire conflict scoring to database queries and create conflicts CLI subcommand with multi-format output.**

## Summary

Created conflicts CLI subcommand with committee trading score calculation, donation-trade correlation queries, and output formatting in all 5 formats (table/JSON/CSV/Markdown/XML). Added three new DB query methods: get_politician_committee_names, get_all_politicians_with_committees, and query_donation_trade_correlations (joins trades->issuers->employer_mappings->donations via ticker and employer matching). All workspace tests pass (595 total), clippy clean, help command works.

## Deviations from Plan

None. Plan executed exactly as written. Both tasks completed successfully with all verification criteria met.

## Key Accomplishments

### Task 1: DB Conflict Query Methods (Commit: a7babc8)
- Added `get_politician_committee_names(&self, politician_id: &str) -> Result<Vec<String>>`
  - Simple SELECT from politician_committees table
  - Returns empty Vec if politician has no committees (not an error)
  - Unit test verifies retrieval of 2 committees for test politician
- Added `get_all_politicians_with_committees() -> Result<Vec<(String, String, Vec<String>)>>`
  - Returns (politician_id, full_name, committees) tuples
  - Uses GROUP_CONCAT to aggregate committees, then splits on comma
  - Only includes politicians with at least one committee assignment
  - Unit test verifies 2 politicians returned (3rd has no committees, excluded)
- Added `query_donation_trade_correlations(&self, min_confidence: f64) -> Result<Vec<DonationTradeCorrelation>>`
  - Complex JOIN path: trades -> issuers (ticker) -> employer_mappings (ticker match) -> donations (employer match) -> donation_sync_meta (politician_id link)
  - Ensures same politician received donation AND made trade (dsm.politician_id = t.politician_id)
  - Filters by employer_mappings.confidence >= min_confidence parameter
  - Groups by politician_id, ticker; aggregates donor count, avg confidence, total donation amount
  - Returns empty Vec if employer_mappings table empty (no error)
  - Unit test verifies empty result when no mapping data present
- All methods include comprehensive unit tests (4 total)
- Tests use in-memory DB with full schema initialization

### Task 2: Conflicts CLI Subcommand (Commit: 0d21ea1)
- Created `capitoltraders_cli/src/commands/conflicts.rs` (256 lines)
  - ConflictsArgs with 7 parameters: db (required), politician, committee, min-committee-pct (0-100), include-donations flag, min-confidence (0.0-1.0), top (default 25)
  - Validation: min-committee-pct range 0-100, min-confidence range 0.0-1.0
  - Pipeline: query_trades_for_analytics -> convert to AnalyticsTrade -> calculate_closed_trades (FIFO) -> get_all_politicians_with_committees -> filter by politician/committee -> calculate_committee_trading_score -> sort by pct descending -> truncate to top
  - Politician filter uses find_politician_by_name (Phase 10 pattern) for name resolution
  - Committee filter exact match on committee name string
  - Disclaimer printed to stderr before output: "Based on current committee assignments. Historical committee membership not tracked. Trades with unknown sector excluded from scoring."
  - Summary printed to stderr after output: "Showing N/total politicians with committee assignments (M scored trades, min threshold: X%)"
  - If --include-donations: query_donation_trade_correlations and output separately
  - ConflictRow: rank, politician_name, committees (comma-joined), total_scored_trades, committee_related_trades, committee_trading_pct
  - DonationCorrelationRow: politician_name, ticker, matching_donors, total_donations, donor_employers
- Added conflict output functions to `output.rs` (210 lines added)
  - print_conflict_table/markdown/csv/xml for ConflictRow
  - print_donation_correlation_table/markdown/csv/xml for DonationCorrelationRow
  - Table format uses tabled with formatted committee_trading_pct (e.g., "66.7%")
  - CSV sanitization applied to politician_name, committees, and donor_employers fields
  - Markdown uses tabled Style::markdown()
- Added XML serialization to `xml_output.rs`
  - conflicts_to_xml: items_to_xml("conflicts", "conflict", rows)
  - donation_correlations_to_xml: items_to_xml("donation_correlations", "correlation", rows)
- Wired into CLI dispatch in `main.rs`
  - Commands::Conflicts variant added to enum
  - Match arm added to dispatch logic
- Module export added to `commands/mod.rs`

## Critical Implementation Details

### DB Query JOIN Path
- query_donation_trade_correlations uses 6-table JOIN:
  - trades t -> issuers i (ON t.issuer_id = i.issuer_id)
  - -> politicians p (ON t.politician_id = p.politician_id)
  - -> employer_mappings em (ON i.issuer_ticker = em.ticker)
  - -> donations d (ON LOWER(d.contributor_employer) = LOWER(em.employer))
  - -> donation_sync_meta dsm (ON d.sub_id = dsm.sub_id)
- Critical WHERE clause: `dsm.politician_id = t.politician_id` ensures correlation is for same politician
- Handles empty employer_mappings table gracefully (check COUNT before query, return empty Vec)

### AnalyticsTrade Conversion
- conflicts.rs uses same row_to_analytics_trade pattern as analytics.rs
- Must set has_sector_benchmark = gics_sector.is_some() AND benchmark_price.is_some()
- Field names: estimated_shares, trade_date_price (NOT shares, price)
- calculate_closed_trades takes ownership (Vec, not &Vec)

### Output Formatting Patterns
- Table/Markdown use tabled with inline struct definitions (ConflictTableRow, DonationTableRow)
- CSV uses csv::Writer with manual record writing for control over sanitization
- XML delegates to xml_output module via items_to_xml helper
- JSON uses generic print_json from output.rs (works with any Serialize type)

## Testing Summary

All tests passing:
- 4 new DB query tests (get_politician_committee_names, _empty, get_all_politicians_with_committees, query_donation_trade_correlations_empty)
- 595 total workspace tests (no regressions)
- Clippy clean (no warnings)
- Help command displays all 7 arguments correctly

## Integration Points

### Upstream Dependencies
- Plan 01: CommitteeTradingScore, DonationTradeCorrelation types, calculate_committee_trading_score function, load_committee_jurisdictions
- Phase 15: AnalyticsTrade, ClosedTrade, calculate_closed_trades (FIFO matching)
- Phase 10: find_politician_by_name (politician filter resolution)
- Phase 14: employer_mappings table (donation-trade correlation)

### Downstream Impact
- New conflicts CLI command available to users
- Enables CONF-01 through CONF-04 requirements from REQUIREMENTS.md
- Provides foundation for future conflict alert features

## Files Modified

**Created:**
- `capitoltraders_cli/src/commands/conflicts.rs` (256 lines, CLI subcommand + output types)

**Modified:**
- `capitoltraders_lib/src/db.rs` (+265 lines, 3 new query methods + 4 unit tests)
- `capitoltraders_cli/src/commands/mod.rs` (+1 line, module export)
- `capitoltraders_cli/src/main.rs` (+2 lines, Commands variant + dispatch)
- `capitoltraders_cli/src/output.rs` (+210 lines, 8 output functions)
- `capitoltraders_cli/src/xml_output.rs` (+10 lines, 2 XML functions + import)

## Verification

```bash
cargo test -p capitoltraders_lib get_politician_committee_names -- --nocapture  # 2 passed
cargo test -p capitoltraders_lib get_all_politicians_with_committees -- --nocapture  # 1 passed
cargo test -p capitoltraders_lib query_donation_trade_correlations -- --nocapture  # 1 passed
cargo test --workspace  # 595 passed
cargo clippy --workspace  # clean
cargo build --workspace  # success
cargo run -p capitoltraders_cli -- conflicts --help  # displays all arguments
```

## Usage Examples

```bash
# Basic committee trading scores
capitoltraders conflicts --db trades.db

# Filter by politician (name resolution)
capitoltraders conflicts --db trades.db --politician "Pelosi"

# Filter by committee
capitoltraders conflicts --db trades.db --committee "hsba"

# Minimum threshold (50%+ committee-related trades)
capitoltraders conflicts --db trades.db --min-committee-pct 50

# Include donation-trade correlations
capitoltraders conflicts --db trades.db --include-donations

# CSV output
capitoltraders conflicts --db trades.db --output csv > conflicts.csv

# Top 10 with donations (90%+ confidence)
capitoltraders conflicts --db trades.db --top 10 --include-donations --min-confidence 0.90
```

## Self-Check

Verifying created files and commits:

**Created files:**
```bash
[ -f "capitoltraders_cli/src/commands/conflicts.rs" ] && echo "FOUND: conflicts.rs" || echo "MISSING"
```
FOUND: conflicts.rs

**Commits:**
```bash
git log --oneline --all | grep -E "(a7babc8|0d21ea1)" && echo "FOUND commits" || echo "MISSING"
```
0d21ea1 feat(16-02): add conflicts CLI subcommand with output formatting
a7babc8 feat(16-02): add DB conflict query methods
FOUND commits

**Help command:**
```bash
cargo run -p capitoltraders_cli -- conflicts --help | grep -E "(politician|committee|min-committee-pct|include-donations)" && echo "FOUND args" || echo "MISSING"
```
FOUND args

## Self-Check: PASSED

All created files exist, all commits present, help command displays correctly, all tests passing.
