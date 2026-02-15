---
phase: 17
plan: 01
subsystem: analytics
tags: [anomaly-detection, pure-functions, tdd]
dependencies:
  requires: []
  provides: [anomaly-detection-functions]
  affects: [capitoltraders_lib]
tech-stack:
  added: []
  patterns: [pure-computation, tdd-red-green, division-by-zero-safety]
key-files:
  created:
    - capitoltraders_lib/src/anomaly.rs
  modified:
    - capitoltraders_lib/src/lib.rs
decisions: []
metrics:
  duration_seconds: 263
  completed_date: 2026-02-15
---

# Phase 17 Plan 01: Anomaly Detection Pure Functions Summary

**One-liner:** Pure computation functions for pre-move trade detection, unusual volume detection, HHI sector concentration, and composite anomaly scoring

## What Was Built

Created `capitoltraders_lib/src/anomaly.rs` with four core anomaly detection functions and their supporting types, following TDD methodology (RED-GREEN phases).

### Input Types (Decoupled from DB)
- `TradeWithFuturePrice` - Trade record with 30-day future price for pre-move detection
- `TradeVolumeRecord` - Minimal trade record (politician_id, tx_date) for volume analysis
- `PortfolioPositionForHHI` - Position record (ticker, sector, value) for concentration scoring

### Output Signal Types
- `PreMoveSignal` - Trade that preceded significant price movement (>threshold_pct)
- `VolumeSignal` - Trading frequency analysis with ratio and is_unusual flag
- `ConcentrationScore` - HHI-based portfolio diversification with sector weights
- `AnomalyScore` - Composite score combining all signals with confidence metric

### Core Functions

1. **`detect_pre_move_trades(trades, threshold_pct)`**
   - Filters trades by abs(price_change_pct) > threshold
   - Excludes trades with None price_30d_later
   - Determines direction: buy_before_rise | sell_before_drop | buy_before_drop | sell_before_rise
   - Default threshold: 10.0%

2. **`detect_unusual_volume(trades, politician_id, reference_date, lookback_days, baseline_days)`**
   - Compares recent trading frequency to historical baseline
   - Division-by-zero safe: returns ratio=0.0 if historical_avg=0.0
   - Flags as unusual if volume_ratio > 2.0
   - Default windows: 90-day lookback, 365-day baseline

3. **`calculate_sector_concentration(positions)`**
   - Calculates HHI on 0-1 scale using decimal weights (not percentages)
   - Excludes positions with None gics_sector or estimated_value <= 0.0
   - Flags as concentrated if HHI > 0.25
   - Returns sector weights as percentages for display

4. **`calculate_composite_anomaly_score(pre_move_count, volume_ratio, hhi_score)`**
   - Normalizes: pre_move (count/10), volume (ratio/5.0), concentration (hhi directly)
   - Caps all normalized values at 1.0
   - Composite: equal weights (33.3% each), average of normalized signals
   - Confidence: proportion of non-zero signals (0-1)

## Implementation Notes

### TDD Process
- **RED phase** (commit d1e89dc): Created 20 failing tests covering all edge cases
- **GREEN phase** (commit 6f1b5c3): Implemented functions to pass all tests

### Key Design Decisions
- **Pure functions**: No DB access, no I/O - only computation on provided data
- **Input decoupling**: Custom input types prevent tight coupling to DB row types
- **Division-by-zero safety**: Explicit checks in volume detection and HHI calculation
- **Percentage vs decimal**: HHI uses 0-1 decimal weights (standard formula), sector_weights returns percentages for display
- **Direction classification**: 4-way classification for pre-move signals based on tx_type and price movement direction

### Edge Cases Handled
- Empty input collections (return zero/empty results)
- None values in optional fields (excluded from calculations)
- Division by zero (return 0.0 safely)
- Extreme values in composite scoring (capped at 1.0)
- Date parsing failures (trades excluded from volume analysis)

## Test Coverage

20 unit tests covering:
- **Pre-move detection** (7 tests): basic detection, threshold filtering, None exclusion, negative changes, empty input, all 4 direction types
- **Volume detection** (4 tests): unusual spike, normal activity, no historical data, no recent trades
- **HHI concentration** (6 tests): single sector, two equal sectors, four equal sectors, None exclusion, empty positions, negative value exclusion
- **Composite scoring** (3 tests): all signals present, no signals (zero case), capping at 1.0

All tests pass with 0 warnings from clippy.

## Module Registration

Added to `capitoltraders_lib/src/lib.rs`:
- Module declaration: `pub mod anomaly;`
- Public exports: All 4 signal types, all 3 input types, all 4 functions

## Verification

```bash
cargo test -p capitoltraders_lib anomaly
# 20 passed; 0 failed

cargo clippy -p capitoltraders_lib -- -D warnings
# Finished with no warnings

wc -l capitoltraders_lib/src/anomaly.rs
# 889 lines (exceeds 150 min_lines requirement)
```

## Deviations from Plan

None - plan executed exactly as written.

## Files Changed

- **Created**: `capitoltraders_lib/src/anomaly.rs` (889 lines)
- **Modified**: `capitoltraders_lib/src/lib.rs` (+2 sections: module declaration, pub use exports)

## Commits

| Hash    | Type | Description                                      |
|---------|------|--------------------------------------------------|
| d1e89dc | test | Add failing tests for anomaly detection (RED)    |
| 6f1b5c3 | feat | Implement anomaly detection functions (GREEN)    |

## Next Steps

Plan 17-02 will add DB query methods to fetch the input data for these functions:
- `get_trades_with_future_prices()` - Query trades joined with 30-day-later prices
- `get_politician_trade_volume()` - Query (politician_id, tx_date) trade records
- `get_politician_portfolio_for_hhi()` - Query open positions with sectors and values

Plan 17-03 will add CLI commands to expose anomaly detection results with formatted output.

## Self-Check: PASSED

Created files exist:
```bash
[ -f "capitoltraders_lib/src/anomaly.rs" ] && echo "FOUND"
# FOUND
```

Commits exist:
```bash
git log --oneline | grep -E "(d1e89dc|6f1b5c3)"
# 6f1b5c3 feat(17-01): implement anomaly detection functions
# d1e89dc test(17-01): add failing tests for anomaly detection module
```

Module exports verified:
```bash
grep -E "(pub mod anomaly|pub use anomaly)" capitoltraders_lib/src/lib.rs
# pub mod anomaly;
# pub use anomaly::{...}
```

All 20 tests pass, 0 clippy warnings, module properly integrated.
