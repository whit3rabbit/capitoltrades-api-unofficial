---
phase: 17-anomaly-detection-output-integration
plan: 03
subsystem: cli-integration
tags: [anomaly-detection, db-queries, cli-subcommand, output-formatting]

# Dependency graph
requires:
  - phase: 17-anomaly-detection-output-integration
    plan: 01
    provides: Pure anomaly detection functions (pre-move, volume, HHI, composite scoring)
  - phase: 17-anomaly-detection-output-integration
    plan: 02
    provides: Analytics and conflict output integration patterns

provides:
  - DB query methods for anomaly signal data (pre-move candidates, trade volume, HHI positions)
  - Anomalies CLI subcommand with filtering, sorting, and confidence thresholds
  - Pre-move trade detection visible in CLI output
  - Unusual volume ratio and sector concentration scores per politician
  - Composite anomaly scoring combining all three signals
  - All 5 output formats for anomaly and pre-move signal data

affects: [db-layer, cli-commands, output-formatting, anomaly-detection-pipeline]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "DB query methods returning specialized row types for anomaly detection input"
    - "CLI command pattern: query DB, convert to detection input types, run pure functions, filter/sort, output"
    - "Politician name resolution with multiple-match handling"

key-files:
  created:
    - capitoltraders_cli/src/commands/anomalies.rs
  modified:
    - capitoltraders_lib/src/db.rs
    - capitoltraders_lib/src/lib.rs
    - capitoltraders_cli/src/commands/mod.rs
    - capitoltraders_cli/src/main.rs
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/xml_output.rs

key-decisions:
  - "Three specialized DB row types (PreMoveCandidateRow, TradeVolumeRow, HHIPositionRow) instead of generic query result types"
  - "Simplified pre-move future price query: ORDER BY t2.tx_date ASC instead of complex JULIANDAY calculation to avoid SQLite scoping issues"
  - "HHI positions query uses subquery for current_price from trades table (positions table has no current_price column)"
  - "Politician filter uses find_politician_by_name with explicit multiple-match handling (user-friendly error messages)"

patterns-established:
  - "DB query pattern: specialized row types for specific use cases, minimal JOIN complexity, subqueries for derived values"
  - "CLI anomaly pattern: query all sources, convert to pure function inputs, compute signals, combine scores, filter/sort, output"

# Metrics
duration: 548s  # 9min 8sec
completed: 2026-02-15
---

# Phase 17 Plan 03: Anomalies CLI Integration and Output Summary

**CLI subcommand for detecting unusual trading patterns with pre-move trades, volume spikes, HHI sector concentration, and composite anomaly scoring**

## Performance

- **Duration:** 9 min 8 sec
- **Started:** 2026-02-15 (epoch 1771182780)
- **Completed:** 2026-02-15 (epoch 1771183328)
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

- Added 3 DB query methods for anomaly signal data (pre-move candidates, trade volume, HHI positions)
- Created anomalies CLI subcommand with 7 filter/sort parameters
- Pre-move trade flags visible with price change percentage and direction
- Unusual volume flags with volume ratio per politician
- Sector concentration (HHI) score per politician
- Composite anomaly score combining all three signals with confidence metric
- --min-confidence filter (ANOM-05) excludes low-confidence results
- --show-pre-move flag displays detailed pre-move trade signals
- All 5 output formats supported (table, JSON, CSV, markdown, XML)
- Politician name filter with multiple-match error handling

## Task Commits

Each task was committed atomically:

1. **Task 1: DB query methods for anomaly signal data** - `a8e8432` (feat)
2. **Task 2: Anomalies CLI subcommand with output formatting** - `cfe694b` (feat)

## Files Created/Modified

**Created:**
- `capitoltraders_cli/src/commands/anomalies.rs` (347 lines) - Anomalies subcommand with AnomaliesArgs, AnomalyRow/PreMoveRow types, run() function

**Modified:**
- `capitoltraders_lib/src/db.rs` - Added PreMoveCandidateRow, TradeVolumeRow, HHIPositionRow types and 3 query methods
- `capitoltraders_lib/src/lib.rs` - Exported new row types
- `capitoltraders_cli/src/commands/mod.rs` - Registered anomalies module
- `capitoltraders_cli/src/main.rs` - Added Anomalies variant to Commands enum and dispatch
- `capitoltraders_cli/src/output.rs` - Added 8 output functions (anomaly + pre-move in table/CSV/markdown formats)
- `capitoltraders_cli/src/xml_output.rs` - Added anomalies_to_xml and pre_move_signals_to_xml

## Decisions Made

**Three specialized DB row types:** PreMoveCandidateRow, TradeVolumeRow, HHIPositionRow instead of reusing generic DbTradeRow or PortfolioPosition. Rationale: Each detection function needs different data (pre-move needs future price, volume needs date only, HHI needs sector + value). Specialized types make the query intent explicit and avoid JOIN bloat.

**Simplified pre-move future price ordering:** Initial plan used `ORDER BY ABS(JULIANDAY(t2.tx_date) - JULIANDAY(t.tx_date) - 30)` to find the nearest trade to 30 days later. SQLite threw "no such column: t.tx_date" error due to scoping issues in the nested JULIANDAY expression within the subquery's ORDER BY. Changed to simple `ORDER BY t2.tx_date ASC` which gets the earliest trade in the 28-32 day window. Trade-off: slightly less precise (may not be closest to day 30) but functionally equivalent for pre-move detection.

**HHI positions query uses current_price subquery:** positions table has no current_price column (only shares_held, cost_basis). Followed get_portfolio() pattern: correlated subquery finding latest current_price from trades table joined by ticker. Estimated value = shares_held * current_price (or cost_basis as fallback).

**Politician filter with multiple-match handling:** find_politician_by_name returns Vec<(id, name)> not Option<id>. Check .is_empty() for no match, .len() > 1 for ambiguous match, print friendly error listing all matches. More user-friendly than silently picking first match or erroring cryptically.

## Deviations from Plan

**Rule 3 auto-fix:** Simplified pre-move query ORDER BY due to SQLite scoping limitation. Blocking issue preventing query execution.

## Issues Encountered

**SQLite correlated subquery scoping:** Complex ORDER BY expression referencing outer query column within nested function call failed. Simplified to basic column ordering.

**Positions table schema mismatch:** Plan assumed current_price column exists on positions table. Actual schema has it on trades table. Used subquery pattern from existing get_portfolio() method.

**find_politician_by_name return type:** Expected Option<String> but actual signature is Vec<(String, String)>. Adjusted filter logic to handle empty/multiple matches.

**Clippy needless_borrows warning:** CSV write_record takes &[&str] not &&[&str]. Removed & from array literals.

**Clippy or_insert_with suggestion:** Vec::new is Default, use .or_default() instead of .or_insert_with(Vec::new).

## Next Phase Readiness

- Users can run `capitoltraders anomalies --db path` to detect unusual patterns
- Pre-move trade flags identify trades before significant price movements (>10% threshold)
- Volume ratio flags politicians with trading frequency spikes (>2x historical baseline)
- HHI scores identify concentrated portfolios (single-sector dominance)
- Composite anomaly score combines all signals with confidence weighting
- All output formats (table, JSON, CSV, markdown, XML) supported for main results and pre-move details
- --min-confidence filter enables high-signal filtering (ANOM-05 requirement)
- Phase 17 complete: anomaly detection pipeline functional from pure functions to CLI output

---

## Self-Check: PASSED

All files verified:
- capitoltraders_cli/src/commands/anomalies.rs (created) ✓
- capitoltraders_lib/src/db.rs (3 new methods + 3 row types) ✓
- capitoltraders_lib/src/lib.rs (exports) ✓
- capitoltraders_cli/src/commands/mod.rs (module registration) ✓
- capitoltraders_cli/src/main.rs (Commands enum + dispatch) ✓
- capitoltraders_cli/src/output.rs (8 output functions) ✓
- capitoltraders_cli/src/xml_output.rs (2 XML functions) ✓

All commits verified:
- a8e8432: feat(17-03): add DB query methods for anomaly signal data ✓
- cfe694b: feat(17-03): add anomalies CLI subcommand with output formatting ✓

CLI verification:
```bash
cargo run -p capitoltraders_cli -- anomalies --help
# Usage: capitoltraders anomalies [OPTIONS] --db <DB>
# Options: --db, --politician, --min-score, --min-confidence, --show-pre-move, --top, --sort-by

cargo run -p capitoltraders_cli -- --help | grep anomalies
# anomalies       Detect unusual trading patterns (pre-move trades, volume spikes, sector concentration)
```

All workspace tests pass: 516 tests (3 new DB query tests added)
Clippy: 0 warnings
