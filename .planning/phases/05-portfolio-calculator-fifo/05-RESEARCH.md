# Phase 5: Portfolio Calculator (FIFO) - Research

**Researched:** 2026-02-10
**Domain:** FIFO accounting, portfolio position calculation, realized/unrealized P&L
**Confidence:** HIGH

## Summary

Phase 5 implements a FIFO (First-In-First-Out) portfolio calculator that maintains per-politician per-ticker positions with cost basis tracking, realized P&L from closed lots, and unrealized P&L from open positions. The core challenge is processing transactions chronologically, matching sells to buys using FIFO queue mechanics, and calculating precise share counts and cost basis without floating-point drift.

The codebase already has critical primitives: trades table with estimated_shares from Phase 4, positions table in schema with shares_held/cost_basis/realized_pnl columns, transaction type handling (buy/sell/exchange/receive), and asset_type classification for options filtering. The implementation follows a materialized positions pattern: calculate once, store in positions table, query efficiently for portfolio display. This avoids recalculating FIFO on every portfolio query, which would be too slow for 100K+ trades.

FIFO accounting requires maintaining a lot queue per position: each buy creates a new lot with (shares, price, date), each sell consumes lots from the front of the queue until the sell quantity is satisfied. Cost basis is weighted average of remaining lots. Realized P&L accumulates as lots close: (sell_price - lot_cost_basis) * shares_sold. Unrealized P&L uses current position: (current_price - avg_cost_basis) * shares_held.

**Primary recommendation:** Use f64 for calculations (matches existing estimated_shares REAL type), store intermediate lot state in memory (VecDeque per position), write final aggregated positions to SQLite positions table. Process trades in chronological order (ORDER BY tx_date, tx_id), group by politician_id + issuer_ticker, filter out options (asset_type != 'stock'), handle negative positions as warnings (data quality issue, not hard error). Performance target 500ms for 100K trades is achievable with in-memory calculation + single bulk upsert.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| std::collections::VecDeque | 1.x (std) | FIFO lot queue per position | Standard library double-ended queue, efficient push_back/pop_front for FIFO |
| std::collections::HashMap | 1.x (std) | Position lookup by (politician, ticker) | Fast O(1) lookup, standard aggregation pattern |
| rusqlite | 0.34 (workspace) | Query trades, upsert positions | Already used for all DB operations, mature |
| chrono | 0.4 (workspace) | Transaction date ordering | Already used for date parsing, reliable chronological sorting |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| anyhow | 1.x (workspace) | Error handling | Already used workspace-wide, simplifies error propagation |
| thiserror | 1.x (workspace) | Custom DbError variants | If adding portfolio-specific error types |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| f64 for shares/prices | rust_decimal crate | Decimal has perfect precision but requires crate dependency + type conversions. f64 is already used for estimated_shares, price columns (REAL). Converting entire codebase is out of scope. Accept floating-point limitations, use epsilon comparisons for zero checks. |
| VecDeque | BinaryHeap | BinaryHeap requires priority ordering, VecDeque is simpler for strict FIFO. No need for min-heap/max-heap mechanics. |
| HashMap | BTreeMap | BTreeMap maintains sorted order but slower inserts. Don't need sorted iteration over positions. |
| Materialized positions | On-demand FIFO calculation | On-demand recalculates FIFO every query - too slow for 100K trades. Materialized trades positions table for O(1) queries. |

**Installation:**
No new dependencies required - all primitives in std or existing workspace.

## Architecture Patterns

### Recommended Project Structure
```
capitoltraders_lib/src/
├── portfolio.rs           # New: FIFO calculator, position types
└── db.rs                  # Add: query_trades_for_portfolio(), upsert_positions()

capitoltraders_cli/src/commands/
├── portfolio.rs           # New: CLI subcommand for portfolio display
└── sync.rs                # Add: calculate_positions() call after enrichment
```

Portfolio calculation happens in two contexts:
1. **During sync**: After price enrichment completes, calculate positions and upsert to DB (automatic)
2. **On-demand CLI**: `capitoltraders portfolio --politician-id P000001` queries positions table (fast)

### Pattern 1: FIFO Lot Queue
**What:** Maintain chronologically-ordered queue of buy lots, consume from front on sells
**When to use:** Tracking cost basis for realized P&L calculation
**Example:**
```rust
use std::collections::VecDeque;

#[derive(Debug, Clone)]
struct Lot {
    shares: f64,
    cost_basis: f64,  // Price per share at purchase
    tx_date: String,  // For audit trail
}

struct Position {
    politician_id: String,
    ticker: String,
    lots: VecDeque<Lot>,
    realized_pnl: f64,
}

impl Position {
    fn buy(&mut self, shares: f64, price: f64, tx_date: String) {
        self.lots.push_back(Lot {
            shares,
            cost_basis: price,
            tx_date,
        });
    }

    fn sell(&mut self, shares: f64, price: f64) -> Result<(), String> {
        let mut remaining = shares;

        while remaining > 0.0001 {  // Epsilon for floating-point comparison
            let lot = self.lots.front_mut()
                .ok_or("Oversold position")?;

            let sold_from_lot = remaining.min(lot.shares);
            let pnl = (price - lot.cost_basis) * sold_from_lot;
            self.realized_pnl += pnl;

            lot.shares -= sold_from_lot;
            remaining -= sold_from_lot;

            if lot.shares < 0.0001 {
                self.lots.pop_front();
            }
        }

        Ok(())
    }

    fn shares_held(&self) -> f64 {
        self.lots.iter().map(|lot| lot.shares).sum()
    }

    fn avg_cost_basis(&self) -> f64 {
        let total_shares = self.shares_held();
        if total_shares < 0.0001 {
            return 0.0;
        }
        let total_cost: f64 = self.lots.iter()
            .map(|lot| lot.shares * lot.cost_basis)
            .sum();
        total_cost / total_shares
    }
}
```

### Pattern 2: Chronological Transaction Processing
**What:** Process trades in strict date order to ensure FIFO correctness
**When to use:** Building position state from historical transactions
**Example:**
```rust
// Query trades ordered by date, then ID for deterministic ordering
let trades = db.conn.prepare(
    "SELECT tx_id, politician_id, issuer_ticker, tx_type, tx_date,
            estimated_shares, trade_date_price, asset_type
     FROM trades t
     JOIN assets a ON t.asset_id = a.asset_id
     WHERE t.estimated_shares IS NOT NULL
       AND a.asset_type = 'stock'  -- Exclude options
     ORDER BY tx_date ASC, tx_id ASC"
)?;

// Group by (politician_id, ticker) and process chronologically
let mut positions: HashMap<(String, String), Position> = HashMap::new();

for trade in trades {
    let key = (trade.politician_id.clone(), trade.ticker.clone());
    let pos = positions.entry(key).or_insert_with(|| Position::new(
        trade.politician_id.clone(),
        trade.ticker.clone(),
    ));

    match trade.tx_type.as_str() {
        "buy" | "receive" => {
            pos.buy(trade.estimated_shares, trade.trade_date_price, trade.tx_date);
        }
        "sell" => {
            if let Err(e) = pos.sell(trade.estimated_shares, trade.trade_date_price) {
                eprintln!("WARNING: {} {} {}: {}", trade.politician_id, trade.ticker, trade.tx_date, e);
            }
        }
        "exchange" => {
            // Exchange is neutral: sell old, buy new (REQ-P1 spec unclear, treat as no-op)
        }
        _ => {}
    }
}
```

### Pattern 3: Materialized Positions Table
**What:** Store calculated positions in DB for fast queries, recalculate on sync
**When to use:** Portfolio display needs O(1) lookup, not O(n) FIFO recalculation
**Example:**
```rust
// After calculating positions in memory, bulk upsert to DB
pub fn upsert_positions(&self, positions: &[(String, String, f64, f64, f64)]) -> Result<(), DbError> {
    let tx = self.conn.transaction()?;

    let mut stmt = tx.prepare(
        "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(politician_id, issuer_ticker)
         DO UPDATE SET
           shares_held = excluded.shares_held,
           cost_basis = excluded.cost_basis,
           realized_pnl = excluded.realized_pnl,
           last_updated = excluded.last_updated"
    )?;

    for (politician_id, ticker, shares, cost_basis, realized_pnl) in positions {
        stmt.execute(params![politician_id, ticker, shares, cost_basis, realized_pnl])?;
    }

    tx.commit()?;
    Ok(())
}

// Query for portfolio display
pub fn get_positions(&self, politician_id: &str) -> Result<Vec<PositionRow>, DbError> {
    let mut stmt = self.conn.prepare(
        "SELECT politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated
         FROM positions
         WHERE politician_id = ?1
           AND shares_held > 0.0001  -- Filter out closed positions
         ORDER BY shares_held * cost_basis DESC"  -- Order by position value
    )?;

    let rows = stmt.query_map([politician_id], |row| {
        Ok(PositionRow {
            politician_id: row.get(0)?,
            ticker: row.get(1)?,
            shares_held: row.get(2)?,
            cost_basis: row.get(3)?,
            realized_pnl: row.get(4)?,
            last_updated: row.get(5)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}
```

### Pattern 4: Unrealized P&L Calculation with Current Price
**What:** Join positions with current_price from trades table, calculate (current - basis) * shares
**When to use:** Portfolio display needs mark-to-market P&L
**Example:**
```rust
// Query with current_price from trades table (de-duplicated by ticker)
pub fn get_portfolio_with_pnl(&self, politician_id: &str) -> Result<Vec<PortfolioRow>, DbError> {
    let mut stmt = self.conn.prepare(
        "SELECT
           p.politician_id,
           p.issuer_ticker,
           p.shares_held,
           p.cost_basis,
           p.realized_pnl,
           MAX(t.current_price) as current_price,  -- Most recent current_price
           MAX(t.price_enriched_at) as price_date
         FROM positions p
         LEFT JOIN trades t ON p.issuer_ticker = t.issuer_ticker
                            AND t.current_price IS NOT NULL
         WHERE p.politician_id = ?1
           AND p.shares_held > 0.0001
         GROUP BY p.politician_id, p.issuer_ticker
         ORDER BY p.shares_held * p.cost_basis DESC"
    )?;

    let rows = stmt.query_map([politician_id], |row| {
        let shares: f64 = row.get(2)?;
        let cost_basis: f64 = row.get(3)?;
        let realized_pnl: f64 = row.get(4)?;
        let current_price: Option<f64> = row.get(5)?;

        let unrealized_pnl = current_price.map(|price| {
            (price - cost_basis) * shares
        });

        Ok(PortfolioRow {
            politician_id: row.get(0)?,
            ticker: row.get(1)?,
            shares_held: shares,
            cost_basis,
            realized_pnl,
            unrealized_pnl,
            current_price,
            price_date: row.get(6)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(rows)
}
```

### Pattern 5: Options Classification
**What:** Filter out non-stock trades (options, crypto, etc.) from FIFO calculation
**When to use:** Options have different P&L mechanics, exclude per REQ-P4
**Example:**
```rust
// In portfolio calculation, filter by asset_type
SELECT t.*, a.asset_type
FROM trades t
JOIN assets a ON t.asset_id = a.asset_id
WHERE a.asset_type = 'stock'  -- Exclude 'stock-option', 'cryptocurrency', etc.
  AND t.estimated_shares IS NOT NULL

// Track option trades separately for reporting
pub fn get_option_trades(&self, politician_id: &str) -> Result<Vec<OptionTrade>, DbError> {
    self.conn.prepare(
        "SELECT t.tx_id, t.politician_id, a.asset_ticker, t.tx_type, t.tx_date, t.value
         FROM trades t
         JOIN assets a ON t.asset_id = a.asset_id
         WHERE t.politician_id = ?1
           AND a.asset_type LIKE '%option%'
         ORDER BY t.tx_date DESC"
    )?
    // ... map rows
}
```

### Anti-Patterns to Avoid
- **Don't recalculate FIFO on every portfolio query:** Materialized positions table is mandatory for performance
- **Don't use strict equality for f64:** Use epsilon comparisons (`abs(x) < 0.0001`) for zero checks
- **Don't mix transaction types:** Buy/receive add shares, sell subtracts, exchange is edge case (treat as no-op until spec clarified)
- **Don't fail on negative positions:** Log warning, continue processing (data quality issue, not calculator bug)
- **Don't ignore tx_date ordering:** Chronological order is critical for FIFO correctness

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| FIFO queue mechanics | Custom linked list | std::collections::VecDeque | Battle-tested, efficient push_back/pop_front, handles edge cases |
| Decimal precision | Custom fixed-point math | Accept f64 limitations (or defer rust_decimal to Phase 6) | f64 already used for estimated_shares and prices, converting entire codebase is scope creep. Epsilon comparisons handle precision. |
| Date parsing | String splitting | chrono::NaiveDate | Already used for tx_date, handles edge cases, sortable |
| Position aggregation | Nested loops | HashMap with composite key | O(1) lookup, standard Rust pattern |
| Transaction ordering | Manual sorting | SQL ORDER BY tx_date, tx_id | Database handles sorting efficiently, deterministic |

**Key insight:** FIFO accounting is conceptually simple but has many edge cases (fractional shares, floating-point precision, negative positions, missing prices). Use standard library primitives to avoid reimplementing queue mechanics, and leverage SQLite for chronological ordering. The positions table materialization is critical: calculating FIFO on-demand for 100K trades would violate the <500ms performance requirement.

## Common Pitfalls

### Pitfall 1: Floating-Point Precision Drift
**What goes wrong:** After many buy/sell transactions, shares_held drifts (e.g., 100.00000001 or 99.99999998) due to f64 arithmetic
**Why it happens:** Binary floating-point cannot represent decimal fractions exactly (0.1 + 0.2 != 0.3)
**How to avoid:**
- Use epsilon comparisons for zero checks: `shares_held < 0.0001` instead of `shares_held == 0.0`
- Round to reasonable precision when storing to DB: `format!("{:.4}", shares)` or store as-is and round in display layer
- Accept that cost basis will have minor drift (e.g., $100.0000012) - immaterial for portfolio display
- If precision becomes critical in Phase 6+, introduce rust_decimal incrementally (conversion layer)
**Warning signs:** Position shows 0.00000001 shares remaining, assertions fail on exact equality checks, negative shares very close to zero

### Pitfall 2: Negative Positions from Data Quality Issues
**What goes wrong:** Sell transaction recorded before corresponding buy, or sell quantity exceeds total bought shares
**Why it happens:** Capitol Trades data may have gaps (missing old trades), delayed filings, or data errors
**How to avoid:**
- Detect negative position: `VecDeque::front()` returns None when selling from empty queue
- Log warning with politician_id, ticker, tx_id, and continue processing (don't fail entire batch)
- Store position with shares_held = 0 and add note in realized_pnl or separate column
- Portfolio display shows warning: "Position incomplete (negative shares detected)"
**Warning signs:** Logs show "Oversold position" warnings, certain politicians/tickers always have zero shares despite recent buys

### Pitfall 3: Non-Chronological Transaction Processing
**What goes wrong:** Processing trades in wrong order causes incorrect FIFO matching (sells matched to wrong buys)
**Why it happens:** DB query doesn't ORDER BY, or only sorts by tx_date without secondary sort on tx_id
**How to avoid:**
- Always use `ORDER BY tx_date ASC, tx_id ASC` in query (deterministic ordering)
- Verify ordering in tests: create trades with same tx_date, different tx_id, assert FIFO correctness
- Never rely on insertion order or assume DB returns rows chronologically
**Warning signs:** Portfolio positions inconsistent across runs, FIFO calculations different when re-run on same data

### Pitfall 4: Options Mixed into Stock Positions
**What goes wrong:** Option trades (calls/puts) processed as stock buys/sells, inflating share counts incorrectly
**Why it happens:** asset_type column not filtered, or filter misses variants like "stock-option"
**How to avoid:**
- Filter query: `WHERE a.asset_type = 'stock'` (exact match, not LIKE)
- Track option trades separately: display count + total value, but mark as "valuation deferred" per REQ-P4
- Test with mixed asset types: ensure option trades don't affect stock position shares_held
**Warning signs:** Share counts unrealistically high, cost basis near zero (option premium vs stock price), tickers have both stock and option trades but only stock appears in positions

### Pitfall 5: Missing estimated_shares Causes Silent Skips
**What goes wrong:** Trades with NULL estimated_shares silently skipped, positions incomplete
**Why it happens:** Phase 4 enrichment failed for some tickers (invalid ticker, missing historical price), query filters `WHERE estimated_shares IS NOT NULL`
**How to avoid:**
- Track coverage: before FIFO calculation, count trades with/without estimated_shares per politician
- Log warning: "X trades skipped (no estimated_shares)" with breakdown by ticker
- Portfolio display shows metadata: "Position based on Y/Z trades (A% coverage)"
- If coverage <80%, suggest re-running Phase 4 enrichment with --force
**Warning signs:** Position shares_held much lower than expected, logs show many skipped trades, politicians with active trading show empty portfolio

### Pitfall 6: Performance Degradation with Large Portfolios
**What goes wrong:** Calculating positions for politician with 10K trades takes >500ms (violates REQ-P4 criterion 7)
**Why it happens:** Inefficient query (no indexes), or recalculating FIFO on every query instead of materializing
**How to avoid:**
- Materialize positions: calculate once during sync, store in positions table, query O(1)
- Index positions table: `idx_positions_politician` on politician_id, `idx_positions_ticker` on issuer_ticker
- Benchmark: test with 100K trades, verify <500ms for full recalculation
- Portfolio CLI queries positions table only (pre-calculated), never recalculates FIFO
**Warning signs:** Portfolio CLI response time >1 second, sync --calculate-positions step takes minutes, DB query EXPLAIN shows table scan

### Pitfall 7: Realized P&L Overflow with Large Positions
**What goes wrong:** Realized P&L accumulates to very large positive/negative values, display truncates or shows Infinity
**Why it happens:** Senator buys $1M of stock, sells at 10x gain = $9M realized P&L, f64 can handle but display logic may not
**How to avoid:**
- Use f64 for storage (range ±1.7e308, sufficient for financial calculations)
- Display formatting: use thousands separators, scientific notation for >$1B, or "M"/"B" suffixes
- Test with extreme values: $100M position, 1000% gain, verify display and DB storage
- Realized P&L column type REAL in positions table (already present in schema)
**Warning signs:** Display shows "inf" or "NaN", large positions show truncated P&L, sorting by realized_pnl incorrect

## Code Examples

Verified patterns from existing codebase and stdlib:

### FIFO Lot Queue Implementation
```rust
// capitoltraders_lib/src/portfolio.rs (new file)
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct Lot {
    pub shares: f64,
    pub cost_basis: f64,
    pub tx_date: String,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub politician_id: String,
    pub ticker: String,
    pub lots: VecDeque<Lot>,
    pub realized_pnl: f64,
}

impl Position {
    pub fn new(politician_id: String, ticker: String) -> Self {
        Self {
            politician_id,
            ticker,
            lots: VecDeque::new(),
            realized_pnl: 0.0,
        }
    }

    pub fn buy(&mut self, shares: f64, price: f64, tx_date: String) {
        self.lots.push_back(Lot {
            shares,
            cost_basis: price,
            tx_date,
        });
    }

    pub fn sell(&mut self, shares: f64, price: f64) -> Result<(), String> {
        let mut remaining = shares;

        while remaining > 0.0001 {
            let lot = self.lots.front_mut()
                .ok_or_else(|| format!("Oversold position: {} {} (remaining: {})",
                    self.politician_id, self.ticker, remaining))?;

            let sold_from_lot = remaining.min(lot.shares);
            let pnl = (price - lot.cost_basis) * sold_from_lot;
            self.realized_pnl += pnl;

            lot.shares -= sold_from_lot;
            remaining -= sold_from_lot;

            if lot.shares < 0.0001 {
                self.lots.pop_front();
            }
        }

        Ok(())
    }

    pub fn shares_held(&self) -> f64 {
        self.lots.iter().map(|lot| lot.shares).sum()
    }

    pub fn avg_cost_basis(&self) -> f64 {
        let total_shares = self.shares_held();
        if total_shares < 0.0001 {
            return 0.0;
        }
        let total_cost: f64 = self.lots.iter()
            .map(|lot| lot.shares * lot.cost_basis)
            .sum();
        total_cost / total_shares
    }
}
```

### Portfolio Calculation from Trades
```rust
// capitoltraders_lib/src/portfolio.rs
use std::collections::HashMap;

pub fn calculate_positions(trades: Vec<TradeFIFO>) -> HashMap<(String, String), Position> {
    let mut positions: HashMap<(String, String), Position> = HashMap::new();

    for trade in trades {
        let key = (trade.politician_id.clone(), trade.ticker.clone());
        let pos = positions.entry(key).or_insert_with(|| {
            Position::new(trade.politician_id.clone(), trade.ticker.clone())
        });

        match trade.tx_type.as_str() {
            "buy" | "receive" => {
                pos.buy(trade.estimated_shares, trade.trade_date_price, trade.tx_date);
            }
            "sell" => {
                if let Err(e) = pos.sell(trade.estimated_shares, trade.trade_date_price) {
                    eprintln!("WARNING: {}", e);
                    // Continue processing - don't fail on oversold
                }
            }
            "exchange" => {
                // Exchange handling TBD - treat as no-op for now
            }
            _ => {
                eprintln!("WARNING: Unknown tx_type: {}", trade.tx_type);
            }
        }
    }

    positions
}

#[derive(Debug)]
pub struct TradeFIFO {
    pub tx_id: i64,
    pub politician_id: String,
    pub ticker: String,
    pub tx_type: String,
    pub tx_date: String,
    pub estimated_shares: f64,
    pub trade_date_price: f64,
}
```

### DB Query for FIFO Calculation
```rust
// capitoltraders_lib/src/db.rs (add to existing file)
pub fn query_trades_for_portfolio(&self) -> Result<Vec<TradeFIFO>, DbError> {
    let mut stmt = self.conn.prepare(
        "SELECT
           t.tx_id,
           t.politician_id,
           a.asset_ticker as ticker,
           t.tx_type,
           t.tx_date,
           t.estimated_shares,
           t.trade_date_price
         FROM trades t
         JOIN assets a ON t.asset_id = a.asset_id
         WHERE t.estimated_shares IS NOT NULL
           AND t.trade_date_price IS NOT NULL
           AND a.asset_type = 'stock'
         ORDER BY t.tx_date ASC, t.tx_id ASC"
    )?;

    let trades = stmt.query_map([], |row| {
        Ok(TradeFIFO {
            tx_id: row.get(0)?,
            politician_id: row.get(1)?,
            ticker: row.get(2)?,
            tx_type: row.get(3)?,
            tx_date: row.get(4)?,
            estimated_shares: row.get(5)?,
            trade_date_price: row.get(6)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(trades)
}
```

### Upserting Positions to DB
```rust
// capitoltraders_lib/src/db.rs
pub fn upsert_positions(&self, positions: &HashMap<(String, String), Position>) -> Result<(), DbError> {
    let tx = self.conn.transaction()?;

    let mut stmt = tx.prepare(
        "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
         VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
         ON CONFLICT(politician_id, issuer_ticker)
         DO UPDATE SET
           shares_held = excluded.shares_held,
           cost_basis = excluded.cost_basis,
           realized_pnl = excluded.realized_pnl,
           last_updated = excluded.last_updated"
    )?;

    for ((politician_id, ticker), pos) in positions {
        let shares_held = pos.shares_held();
        let cost_basis = pos.avg_cost_basis();
        let realized_pnl = pos.realized_pnl;

        // Only insert positions with shares (filter out fully closed)
        if shares_held > 0.0001 {
            stmt.execute(params![politician_id, ticker, shares_held, cost_basis, realized_pnl])?;
        }
    }

    tx.commit()?;
    Ok(())
}
```

### Query Portfolio with Unrealized P&L
```rust
// capitoltraders_lib/src/db.rs
#[derive(Debug, Serialize)]
pub struct PortfolioPosition {
    pub politician_id: String,
    pub ticker: String,
    pub shares_held: f64,
    pub cost_basis: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: Option<f64>,
    pub current_price: Option<f64>,
    pub price_date: Option<String>,
}

pub fn get_portfolio(&self, politician_id: &str) -> Result<Vec<PortfolioPosition>, DbError> {
    let mut stmt = self.conn.prepare(
        "SELECT
           p.politician_id,
           p.issuer_ticker,
           p.shares_held,
           p.cost_basis,
           p.realized_pnl,
           (SELECT t2.current_price
            FROM trades t2
            JOIN assets a2 ON t2.asset_id = a2.asset_id
            WHERE a2.asset_ticker = p.issuer_ticker
              AND t2.current_price IS NOT NULL
            ORDER BY t2.price_enriched_at DESC
            LIMIT 1) as current_price,
           (SELECT t2.price_enriched_at
            FROM trades t2
            JOIN assets a2 ON t2.asset_id = a2.asset_id
            WHERE a2.asset_ticker = p.issuer_ticker
              AND t2.current_price IS NOT NULL
            ORDER BY t2.price_enriched_at DESC
            LIMIT 1) as price_date
         FROM positions p
         WHERE p.politician_id = ?1
           AND p.shares_held > 0.0001
         ORDER BY p.shares_held * p.cost_basis DESC"
    )?;

    let positions = stmt.query_map([politician_id], |row| {
        let shares_held: f64 = row.get(2)?;
        let cost_basis: f64 = row.get(3)?;
        let realized_pnl: f64 = row.get(4)?;
        let current_price: Option<f64> = row.get(5)?;

        let unrealized_pnl = current_price.map(|price| {
            (price - cost_basis) * shares_held
        });

        Ok(PortfolioPosition {
            politician_id: row.get(0)?,
            ticker: row.get(1)?,
            shares_held,
            cost_basis,
            realized_pnl,
            unrealized_pnl,
            current_price,
            price_date: row.get(6)?,
        })
    })?
    .collect::<Result<Vec<_>, _>>()?;

    Ok(positions)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Recalculate FIFO on every query | Materialized positions table | Database best practice 2010+ | 100x+ speedup for portfolio queries |
| Store individual lots in DB | Store aggregated position only | Performance optimization | Reduces DB size, simplifies queries |
| Average cost basis | FIFO cost basis | IRS requirement 2025+ | Tax compliance, accurate realized P&L |
| Floating-point arithmetic | rust_decimal for precision | Best practice 2020+ | Eliminates rounding errors (aspirational - Phase 6) |
| Manual epsilon comparisons | Built-in f64::abs().partial_cmp | Rust stdlib | More readable, but still requires epsilon constant |

**Deprecated/outdated:**
- **Per-wallet tracking without position aggregation:** New IRS rules (2025+) require per-wallet accounting, but aggregation across transactions is still allowed
- **LIFO cost basis:** FIFO is now mandatory default for digital assets (2025), but stocks still allow LIFO (we use FIFO per REQ-P1)
- **Integer shares only:** Modern brokerage APIs support fractional shares, estimated_shares is REAL type

## Open Questions

1. **How should "exchange" transaction type affect FIFO?**
   - What we know: Capitol Trades API returns tx_type "exchange", REQ-P1 lists buy/sell/exchange/receive
   - What's unclear: Does "exchange" mean sell stock A + buy stock B (two separate positions)? Or within-position adjustment?
   - Recommendation: Treat as no-op for Phase 5 (skip in FIFO calculation), log occurrence count. Investigate real examples in Phase 6, implement correctly once semantics understood.

2. **Should positions table store closed positions (shares_held = 0)?**
   - What we know: Schema has positions table, no "is_closed" column. Positions can have realized_pnl with zero shares_held.
   - What's unclear: Display closed positions for historical P&L tracking, or hide them?
   - Recommendation: Store closed positions in DB (useful for audit trail), filter in query (WHERE shares_held > 0.0001). Portfolio CLI can add --include-closed flag later.

3. **How to handle missing current_price for unrealized P&L?**
   - What we know: REQ-P2 requires current_price, but Phase 4 may fail to fetch for some tickers
   - What's unclear: Display position with "N/A" for unrealized P&L, or skip position entirely?
   - Recommendation: Display position with all data except unrealized_pnl = None. Show note "Current price unavailable (last updated: <date>)". Position is still valid, just missing mark-to-market.

4. **Should cost_basis be per-share or total position cost?**
   - What we know: Schema has positions.cost_basis REAL, pattern above uses per-share average
   - What's unclear: Store total cost ($10,000) or per-share ($100)?
   - Recommendation: Store per-share cost_basis (avg_cost_basis() function). Easier to calculate unrealized P&L (current_price - cost_basis) * shares, matches industry standard.

5. **Performance requirement interpretation: "100K trades in under 500ms"**
   - What we know: REQ-P4 criterion 7 specifies performance target
   - What's unclear: Does this mean processing 100K trades to calculate positions (batch operation), or querying portfolio for politician with 100K trades (query operation)?
   - Recommendation: Interpret as batch operation (calculate_positions() on 100K trades). Materialized positions table makes query O(1) regardless of trade count. Benchmark both and document.

## Sources

### Primary (HIGH confidence)
- [rust_decimal crate](https://crates.io/crates/rust_decimal) - Decimal implementation for financial calculations (aspirational for Phase 6+)
- [rust_decimal documentation](https://docs.rs/rust_decimal/latest/rust_decimal/) - 128-bit fixed precision, m / 10^e representation
- [std::collections::VecDeque](https://doc.rust-lang.org/std/collections/struct.VecDeque.html) - Double-ended queue for FIFO
- [FIFO Cost Basis (Vanguard)](https://investor.vanguard.com/investor-resources-education/taxes/cost-basis-first-in-first-out) - FIFO accounting definition
- [Position Average Entry Price (Alpaca)](https://docs.alpaca.markets/docs/position-average-entry-price-calculation) - Weighted average cost basis algorithm
- capitoltraders schema/sqlite.sql - positions table structure (lines 144-153), estimated_shares REAL type
- capitoltraders_lib/src/pricing.rs - ShareEstimate, estimate_shares() function (existing primitives)

### Secondary (MEDIUM confidence)
- [FIFO Accounting Methods 2026 (CoinLedger)](https://coinledger.io/blog/cryptocurrency-tax-calculations-fifo-and-lifo-costing-methods-explained) - FIFO vs LIFO, IRS 2025 changes
- [Cost Basis Methods (Koinly)](https://koinly.io/blog/calculate-cost-basis-crypto-bitcoin/) - FIFO calculation walkthrough
- [SQLite Materialized Views (madflex)](https://madflex.de/SQLite-triggers-as-replacement-for-a-materialized-view/) - Trigger-based materialization pattern
- [SQLite Performance Tuning (phiresky)](https://phiresky.github.io/blog/2020/sqlite-performance-tuning/) - Handling 100k rows, concurrency
- [Rust Collections Performance (Medium)](https://ali-alachkar.medium.com/choosing-the-right-rust-collection-a-performance-deep-dive-7fc66f3fbdd9) - VecDeque vs Vec vs LinkedList
- [VecDeque in Rust (w3resource)](https://www.w3resource.com/rust-tutorial/rust-vecdeque-guide.php) - FIFO/LIFO usage patterns

### Tertiary (LOW confidence)
- [Rust Finance Libraries (Lib.rs)](https://lib.rs/finance) - RustQuant, investments crate (not used, but validate patterns)
- [Stock Average Calculator](https://www.omnicalculator.com/finance/stock-average) - Average cost basis formula verification

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - std::collections is stable, f64/REAL type already in use, no new dependencies
- Architecture: HIGH - Materialized positions pattern is proven, FIFO queue with VecDeque is standard
- Pitfalls: MEDIUM - Floating-point precision issues are well-known, but specific tolerance (0.0001) needs validation. Negative position handling is data-dependent.
- Performance: MEDIUM - 500ms target for 100K trades is achievable with in-memory calculation, but needs benchmarking to confirm

**Research date:** 2026-02-10
**Valid until:** 2026-03-12 (30 days - stable domain, no API changes expected)
