---
phase: 05-portfolio-calculator-fifo
plan: 01
subsystem: portfolio
tags: [fifo, accounting, position-tracking, tdd]
dependency_graph:
  requires: []
  provides: [fifo-calculator, position-tracking, realized-pnl]
  affects: []
tech_stack:
  added: []
  patterns: [fifo-queue, lot-based-accounting, epsilon-comparison]
key_files:
  created:
    - capitoltraders_lib/src/portfolio.rs
  modified:
    - capitoltraders_lib/src/lib.rs
decisions: []
metrics:
  duration: 2 min
  completed: 2026-02-11T03:05:13Z
  tasks: 1
  files: 2
  tests: 14
---

# Phase 05 Plan 01: FIFO Portfolio Calculator Summary

**One-liner:** FIFO portfolio calculator with lot-based cost basis tracking and realized P&L accumulation using VecDeque for chronological lot processing.

## What Was Built

Implemented a pure logic FIFO (First-In-First-Out) portfolio calculator module that processes chronologically-ordered trades and maintains per-politician per-ticker positions with lot-level cost basis tracking and realized profit/loss accumulation.

### Core Types

- **Lot**: Single buy lot with shares, cost_basis, and tx_date
- **Position**: Tracks VecDeque of lots (FIFO queue) and accumulated realized_pnl per (politician_id, ticker) pair
- **TradeFIFO**: Trade record for FIFO processing with tx_id, politician_id, ticker, tx_type, tx_date, estimated_shares, and trade_date_price

### Core Functions

- **Position::buy()**: Adds lot to back of VecDeque (push_back)
- **Position::sell()**: Consumes lots from front (FIFO), accumulates realized P&L, returns Err on oversold without panicking
- **Position::shares_held()**: Sums all lot shares
- **Position::avg_cost_basis()**: Returns weighted average or 0.0 when empty
- **calculate_positions()**: Groups trades by (politician_id, ticker) and dispatches by tx_type (buy/receive add shares, sell consumes FIFO, exchange is no-op)

### Key Features

- **FIFO Lot Matching**: Sells consume oldest lots first using VecDeque::pop_front()
- **Partial Lot Sales**: When sale quantity exceeds first lot, remaining shares consume subsequent lots
- **Oversold Handling**: Returns Err with descriptive message (politician_id, ticker, remaining shares) instead of panicking
- **Epsilon Comparisons**: Uses 0.0001 constant for floating-point zero checks (no exact f64 equality)
- **Transaction Type Handling**: buy/receive add shares, sell consumes FIFO, exchange is no-op (logged), unknown types skipped with warning
- **Separate Positions**: Each (politician_id, ticker) pair maintains independent Position

## Test Coverage

14 comprehensive test cases covering:

1. Single buy
2. Buy then full sell
3. Buy then partial sell
4. Multiple buys then sell (FIFO verification)
5. Sell from empty position (Err)
6. Oversold position (partial fill then Err)
7. Receive adds shares like buy
8. Exchange is no-op
9. Multiple politicians same ticker (separate positions)
10. Same politician different tickers (separate positions)
11. Epsilon zero check (99.99999 shares sold rounds to effectively 0)
12. avg_cost_basis when empty (returns 0.0)
13. Unknown tx_type skipped
14. Full lifecycle (multiple buys and sells, FIFO P&L accumulation)

All tests passing (294 total workspace tests: 57 + 9 + 228).

## TDD Execution

Followed strict TDD red-green-refactor cycle:

**RED (6c20495)**: Created portfolio.rs with 14 failing tests covering all FIFO behaviors, registered module in lib.rs, verified tests fail as expected.

**GREEN (4ce2399)**: Implemented Position methods (buy, sell, shares_held, avg_cost_basis) and calculate_positions function. All 14 tests pass. No clippy warnings.

**REFACTOR**: Not needed - implementation already clean and follows Rust best practices (VecDeque for FIFO, HashMap for grouping, epsilon comparisons, no unwraps, proper error handling).

## Deviations from Plan

None - plan executed exactly as written.

## Integration Points

- **Exports**: Module registered in capitoltraders_lib/src/lib.rs with public exports: Lot, Position, TradeFIFO, calculate_positions
- **Dependencies**: Uses only std::collections::{HashMap, VecDeque} - no external dependencies added
- **Next Phase**: Ready for Phase 05 Plan 02 (database integration for position materialization)

## Self-Check: PASSED

### Created Files Verification
```
FOUND: capitoltraders_lib/src/portfolio.rs
```

### Modified Files Verification
```
FOUND: capitoltraders_lib/src/lib.rs
```

### Commit Verification
```
FOUND: 6c20495 (test commit)
FOUND: 4ce2399 (feat commit)
```

### Test Verification
```
All 14 portfolio tests passing
Total workspace tests: 294 (14 new)
cargo clippy: no warnings
cargo check --workspace: clean compile
```
