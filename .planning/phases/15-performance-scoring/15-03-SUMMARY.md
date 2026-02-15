---
phase: 15-performance-scoring
plan: 03
subsystem: cli
tags: [analytics, leaderboard, output-formats, filters]
dependency_graph:
  requires:
    - analytics.rs (FIFO matching, metric calculation, aggregation)
    - db::query_trades_for_analytics (trade data source)
    - validation module (party, state validators)
  provides:
    - analytics CLI subcommand
    - LeaderboardRow type
    - 4 leaderboard output formatters (table, CSV, markdown, XML)
  affects:
    - main.rs Commands enum
    - commands/mod.rs module registry
    - output.rs formatters
    - xml_output.rs serializers
tech_stack:
  added: [analytics.rs CLI command]
  patterns:
    - Time period filtering on closed trades (ytd/1y/2y/all)
    - Politician metadata enrichment via HashMap
    - Percentile rank recomputation after filtering
    - Sort-by metric dispatch (return/win-rate/alpha)
    - CSV formula injection sanitization on politician names
key_files:
  created:
    - capitoltraders_cli/src/commands/analytics.rs
  modified:
    - capitoltraders_cli/src/commands/mod.rs
    - capitoltraders_cli/src/main.rs
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/xml_output.rs
decisions:
  - title: "Filter closed trades before computing metrics"
    rationale: "ClosedTrade has sell_date, TradeMetrics doesn't. Filtering before metrics avoids needing to store sell_date in TradeMetrics just for display purposes."
  - title: "Prefer SPY alpha over sector alpha in LeaderboardRow.avg_alpha"
    rationale: "Market benchmark (SPY) is more universal than sector benchmarks. Display whichever is available, preferring SPY."
  - title: "Re-compute percentile ranks after politician-level filtering"
    rationale: "Percentile rank is relative to the filtered pool. If user filters by party or min_trades, percentiles should reflect position within that subset, not the global set."
  - title: "Sort-by metric defaults to return, not alpha"
    rationale: "Absolute return is simpler to understand than alpha for most users. Alpha is opt-in via --sort-by alpha."
metrics:
  duration_seconds: 257
  duration_minutes: 4.2
  tasks_completed: 2
  tests_total: 574
  tests_added: 0
  clippy_warnings: 0
  lines_added: 522
  completed_date: 2026-02-15
---

# Phase 15 Plan 03: Analytics CLI Command Summary

**One-liner:** User-facing analytics CLI subcommand with performance leaderboards, filters, and all 5 output formats

## What was built

Created the `analytics` CLI subcommand that wires together FIFO matching, metric calculation, politician aggregation, and output formatting into a complete user-facing feature. Users can now run `capitoltraders analytics --db path/to/db.sqlite` to see politician performance rankings with flexible filtering and sorting.

### Analytics CLI Command

**capitoltraders_cli/src/commands/analytics.rs (412 lines):**

**AnalyticsArgs struct (8 fields):**
- db: PathBuf (required)
- period: String (ytd/1y/2y/all, default: all)
- min_trades: usize (default: 5)
- sort_by: String (return/win-rate/alpha, default: return)
- party: Option<String> (democrat/republican)
- state: Option<String> (2-letter state code)
- top: usize (default: 25)

**LeaderboardRow struct (10 fields):**
- rank: usize
- politician_name: String
- party: String
- state: String
- total_trades: usize
- win_rate: f64
- avg_return: f64
- avg_alpha: Option<f64> (prefers SPY over sector)
- avg_holding_days: Option<f64>
- percentile: f64

**run() function flow:**
1. Validate filters (period, sort_by, party, state)
2. Query trades via db.query_trades_for_analytics()
3. Convert AnalyticsTradeRow -> AnalyticsTrade (add has_sector_benchmark flag)
4. Run FIFO matching via calculate_closed_trades()
5. Filter closed trades by period (before computing metrics)
6. Compute TradeMetrics for each closed trade
7. Aggregate by politician via aggregate_politician_metrics()
8. Load politician metadata (name, party, state) into HashMap
9. Apply politician-level filters (min_trades, party, state)
10. Re-compute percentile ranks (pool changed after filtering)
11. Sort by selected metric (return/win-rate/alpha)
12. Truncate to top N
13. Enrich with politician metadata to build LeaderboardRow vec
14. Dispatch to output formatter
15. Print summary to stderr

**Helper functions:**
- row_to_analytics_trade(): Convert DB row to AnalyticsTrade
- filter_closed_trades_by_period(): Filter by sell_date >= cutoff
- load_politician_metadata(): Query politicians table into HashMap
- recompute_percentile_ranks(): Recalculate after filtering (1.0 - index/(n-1))
- sort_by_metric(): Sort by return/win-rate/alpha descending

### Leaderboard Output Formatters

**capitoltraders_cli/src/output.rs (+110 lines):**

**LeaderboardOutputRow struct (10 fields):**
- Flattened row for tabular display
- Formats: win_rate as "XX.X%", avg_return as "+/-XX.X%", alpha as "+/-XX.X%" or "N/A", avg_hold as "XX days" or "N/A", percentile as "XX%"

**4 output functions:**
1. print_leaderboard_table(): ASCII table via tabled
2. print_leaderboard_markdown(): Markdown table with Style::markdown()
3. print_leaderboard_csv(): CSV with sanitized politician names (formula injection protection)
4. print_leaderboard_xml(): XML via xml_output::leaderboard_to_xml()

JSON output uses existing generic print_json() (no new function needed).

**capitoltraders_cli/src/xml_output.rs (+5 lines):**
- leaderboard_to_xml(): Root element `<leaderboard>`, child element `<politician>`

### Command Registration

**capitoltraders_cli/src/commands/mod.rs (+1 line):**
- Added `pub mod analytics;`

**capitoltraders_cli/src/main.rs (+6 lines):**
- Added Analytics variant to Commands enum
- Added dispatch case: `Commands::Analytics(args) => commands::analytics::run(args, &format)?`

## Deviations from Plan

None - plan executed exactly as written.

## Key Implementation Details

### Time Period Filtering

Filters closed trades by sell_date before computing metrics:
- ytd: sell_date >= Jan 1 of current year
- 1y: sell_date >= today - 365 days
- 2y: sell_date >= today - 730 days
- all: no filter

Uses chrono::NaiveDate::parse_from_str() with "%Y-%m-%d" format. Invalid dates excluded (filter returns false).

### Politician Metadata Enrichment

Single query at start of run():
```rust
SELECT politician_id, first_name, last_name, party, state_id FROM politicians
```
Stored in HashMap<String, PoliticianMetadata> for O(1) lookup during row building.

### Percentile Rank Recomputation

After politician-level filtering (min_trades, party, state), pool size changes. Re-sort by avg_return descending and recalculate:
```rust
percentile_rank = 1.0 - (index / (n - 1))
```
Edge case: single politician -> percentile = 1.0

### Sort-by Metric Dispatch

Default sort: avg_return descending (from aggregate_politician_metrics).
- "return": sort by avg_return descending
- "win-rate": sort by win_rate descending
- "alpha": sort by max(avg_alpha_spy, avg_alpha_sector) descending (None -> f64::NEG_INFINITY)

### Output Format Details

**Table columns:** # | Politician | Party | State | Trades | Win Rate | Avg Return | Alpha | Avg Hold | Pctl

**CSV headers:** rank, politician, party, state, trades, win_rate, avg_return, alpha, avg_holding_days, percentile

**CSV sanitization:** sanitize_csv_field() on politician_name (prevents formula injection via names starting with =+-@)

**XML structure:**
```xml
<leaderboard>
  <politician>
    <rank>1</rank>
    <politician_name>John Doe</politician_name>
    <party>Democrat</party>
    ...
  </politician>
</leaderboard>
```

### Edge Cases Handled

- Empty trades: Print hint to run sync + enrich-prices
- No closed trades: Print hint about needing sell transactions
- No trades in period: Print period-specific message
- No politicians match filters: Print filter mismatch message
- Single politician after filtering: percentile = 1.0
- Missing alpha: Display "N/A" instead of None
- Missing holding_days: Display "N/A" instead of None

## Verification

```bash
cargo check --workspace
# Finished in 0.10s

cargo test --workspace
# test result: ok. 574 passed; 0 failed

cargo clippy --workspace -- -D warnings
# Finished in 0.32s (no warnings)

cargo run -p capitoltraders_cli -- analytics --help
# Shows all 7 flags: db, period, min-trades, sort-by, party, state, top
```

## Success Criteria Met

- [x] `capitoltraders analytics --db test.db` command exists
- [x] All 5 output formats work (table, JSON, CSV, markdown, XML)
- [x] Time period filter correctly limits to ytd/1y/2y/all
- [x] Min-trades filter excludes politicians below threshold
- [x] Party and state filters work
- [x] Sort-by changes ordering (return vs win-rate vs alpha)
- [x] Percentile rank displayed for each politician
- [x] All 574 existing tests pass (no regression)
- [x] No clippy warnings

## Commits

- 7311387: feat(15-03): add analytics CLI command with leaderboard output

## Self-Check: PASSED

**Created files exist:**
- ✓ capitoltraders_cli/src/commands/analytics.rs (412 lines)

**Modified files verified:**
- ✓ capitoltraders_cli/src/commands/mod.rs (analytics module added)
- ✓ capitoltraders_cli/src/main.rs (Analytics variant + dispatch)
- ✓ capitoltraders_cli/src/output.rs (4 leaderboard formatters)
- ✓ capitoltraders_cli/src/xml_output.rs (leaderboard_to_xml)

**Commits exist:**
- ✓ 7311387: feat(15-03): add analytics CLI command with leaderboard output

**Verification commands:**
```bash
git log --oneline | head -1
# 7311387 feat(15-03): add analytics CLI command with leaderboard output

cargo run -p capitoltraders_cli -- analytics --help
# Shows all flags correctly

cargo check --workspace && cargo clippy --workspace -- -D warnings
# Both pass with no warnings
```

## Next Steps

Phase 15 is complete after this plan. Next milestone (v1.3 Analytics & Scoring) will continue with:
- Phase 16: Sector correlation analysis
- Phase 17: Export and reporting features

The analytics CLI command is now fully functional and ready for user testing with real database files.
