---
phase: 17-anomaly-detection-output-integration
plan: 02
subsystem: output
tags: [analytics, conflict-detection, output-formatting, cli]

# Dependency graph
requires:
  - phase: 15-performance-scoring
    provides: analytics module with FIFO closed trade matching and performance metrics computation
  - phase: 16-conflict-detection
    provides: conflict module with committee trading score calculation and committee jurisdictions

provides:
  - Enriched trades DB output with optional absolute_return and alpha columns for sell trades
  - Enriched portfolio output with optional gics_sector and in_committee_sector conflict flags
  - Enriched politicians DB output with optional closed_trades, avg_return, win_rate, and percentile columns
  - Best-effort analytics and conflict data integration in primary CLI workflow

affects: [output-formatting, cli-commands, analytics-display, conflict-visualization]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Best-effort enrichment pattern: compute analytics/conflict data with graceful fallback on error"
    - "Enriched row types extending base DB row types with optional analytics/conflict fields"
    - "Separate enriched output functions preserving backward compatibility"

key-files:
  created: []
  modified:
    - capitoltraders_cli/src/commands/trades.rs
    - capitoltraders_cli/src/commands/portfolio.rs
    - capitoltraders_cli/src/commands/politicians.rs
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/xml_output.rs

key-decisions:
  - "Best-effort enrichment with graceful fallback when analytics/conflict data unavailable (price enrichment may not be complete)"
  - "New enriched types (EnrichedDbTradeRow, EnrichedPortfolioPosition, EnrichedDbPoliticianRow) instead of modifying base types for backward compatibility"
  - "In-memory analytics computation in trades and politicians commands using existing FIFO pipeline"
  - "Individual ticker sector queries in portfolio enrichment (small N, acceptable CLI performance overhead)"
  - "Clone DbTradeRow vector to support donor context display after analytics enrichment"

patterns-established:
  - "Enriched output pattern: base row type -> enriched row type with From impl -> enriched output functions"
  - "Best-effort enrichment: try computation, log error, fall back to empty/None fields"
  - "All new fields are Option types with serde skip_serializing_if for backward-compatible JSON/XML"

# Metrics
duration: 10min
completed: 2026-02-15
---

# Phase 17 Plan 02: Analytics and Conflict Output Integration Summary

**Trades, portfolio, and politicians DB output enriched with analytics performance metrics and conflict detection flags in all 5 output formats**

## Performance

- **Duration:** 10 min 20 sec
- **Started:** 2026-02-15T (epoch 1771181972)
- **Completed:** 2026-02-15T (epoch 1771182592)
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Extended trades DB output with optional absolute_return and alpha columns for sell trades
- Extended portfolio output with optional gics_sector and in_committee_sector conflict flags
- Extended politicians DB output with optional closed_trades, avg_return, win_rate, and percentile analytics summary
- All enriched outputs support 5 formats (table, JSON, CSV, markdown, XML) with graceful fallback when data unavailable
- Preserved backward compatibility by keeping original output functions with dead_code allow

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend trades and portfolio DB output with analytics and conflict data** - `48b0d43` (feat)
2. **Task 2: Extend politicians DB output with analytics summary scores** - `d4c78f5` (feat)

## Files Created/Modified
- `capitoltraders_cli/src/commands/trades.rs` - Added EnrichedDbTradeRow type, load_analytics_metrics and enrich_trade_row helpers, modified run_db to compute analytics
- `capitoltraders_cli/src/commands/portfolio.rs` - Added EnrichedPortfolioPosition type, enrich_portfolio_with_conflicts helper, modified run to compute conflict flags
- `capitoltraders_cli/src/commands/politicians.rs` - Added EnrichedDbPoliticianRow type, load_politician_analytics and enrich_politician_row helpers, modified run_db to compute analytics summary
- `capitoltraders_cli/src/output.rs` - Added enriched output functions for trades, portfolio, and politicians in all formats, preserved original functions with dead_code allow
- `capitoltraders_cli/src/xml_output.rs` - Added enriched_trades_to_xml, enriched_portfolio_to_xml, and enriched_politicians_to_xml functions

## Decisions Made

**Best-effort enrichment pattern:** Analytics and conflict enrichment wrapped in match blocks with eprintln on error, graceful fallback to empty HashMap/Vec. Users see informative error messages like "Note: Analytics data unavailable (no price-enriched trades available). Run 'enrich-prices' to enable performance metrics."

**Enriched types over base type modification:** Created new types (EnrichedDbTradeRow, EnrichedPortfolioPosition, EnrichedDbPoliticianRow) extending base DB row types instead of modifying base types directly. Preserves backward compatibility and enables separate output functions.

**In-memory analytics computation:** Politicians and trades commands compute analytics using existing FIFO pipeline (query_trades_for_analytics -> calculate_closed_trades -> compute_trade_metrics -> aggregate_politician_metrics) instead of pre-computing at sync time. Acceptable performance overhead (~100ms) for CLI usage.

**Individual ticker sector queries:** Portfolio enrichment queries gics_sector per ticker instead of bulk query with dynamic IN clause to avoid rusqlite cross-crate dependency issues. Small N (typically <20 positions per politician), acceptable CLI latency.

**Clone for donor context:** Clone DbTradeRow vector before enrichment to support donor context display (--show-donor-context) which needs original rows after analytics enrichment consumed the vector.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

**rusqlite not re-exported:** Initial attempt to use rusqlite::params_from_iter for bulk sector query failed because rusqlite is not re-exported from capitoltraders_lib and not a direct dependency of capitoltraders_cli. Resolved by switching to individual queries per ticker (simpler, acceptable performance for CLI).

**DbTradeRow fields mismatch:** Initial EnrichedDbTradeRow used wrong field names (issuer_id, size_range_low, size_range_high don't exist on DbTradeRow). Fixed by copying actual DbTradeRow fields from db.rs.

**AnalyticsTradeRow field names:** ticker field doesn't exist on AnalyticsTradeRow (it's issuer_ticker), has_sector_benchmark doesn't exist (derived from gics_sector.is_some()). Fixed by using correct field names.

**Option type Tabled display:** Tabled derive doesn't know how to display Option<String> without custom display_with function. Added display_option_str and display_option_usize helpers.

**Value type mismatch:** DbTradeRow.value is i64, not f64. Removed erroneous cast in format_value call.

**Dead code warnings:** Original un-enriched output functions (print_db_trades_table, print_portfolio_table, print_db_politicians_table, etc.) flagged as unused after enriched versions replaced them. Added #[allow(dead_code)] to preserve functions for potential future use.

## Next Phase Readiness

- Analytics and conflict data now visible in primary CLI workflow (trades, portfolio, politicians commands)
- Users can see performance and conflict metrics without running separate analytics/conflicts subcommands
- All enriched outputs support full format suite (table, JSON, CSV, markdown, XML)
- Graceful fallback when analytics/conflict data unavailable ensures commands never fail
- Ready for Phase 17 Plan 03: real-time anomaly detection and alerting

---
*Phase: 17-anomaly-detection-output-integration*
*Completed: 2026-02-15*

## Self-Check: PASSED

All files verified:
- capitoltraders_cli/src/commands/trades.rs ✓
- capitoltraders_cli/src/commands/portfolio.rs ✓
- capitoltraders_cli/src/commands/politicians.rs ✓
- capitoltraders_cli/src/output.rs ✓
- capitoltraders_cli/src/xml_output.rs ✓

All commits verified:
- 48b0d43: feat(17-02): extend trades and portfolio output with analytics and conflict data ✓
- d4c78f5: feat(17-02): extend politicians DB output with analytics summary scores ✓

