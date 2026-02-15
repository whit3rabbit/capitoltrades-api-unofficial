---
phase: 14-benchmark-price-enrichment
plan: 02
subsystem: enrichment
tags: [enrichment, benchmark, yahoo-finance, pipeline]
dependency_graph:
  requires: [14-01-schema-v7-migration]
  provides: [benchmark-enrichment-pipeline]
  affects: [enrich-prices-command]
tech_stack:
  added: []
  patterns: [phase-3-enrichment, gics-to-etf-mapping, concurrent-fetch]
key_files:
  created: []
  modified:
    - capitoltraders_cli/src/commands/enrich_prices.rs
decisions:
  - title: Phase 3 uses separate semaphore from Phase 1
    rationale: Phase 1 permits may not be fully released if circuit breaker tripped. Independent semaphore ensures Phase 3 has full concurrency budget.
    alternatives: [reuse Phase 1 semaphore]
  - title: BenchmarkPriceResult uses Vec<i64> (tx_ids) not Vec<usize> (indices)
    rationale: Phase 3 uses separate query (benchmark_trades), so indices would reference wrong vec. tx_ids are database identifiers.
    alternatives: [Vec<usize> with separate index mapping]
metrics:
  duration_minutes: 2.1
  tasks_completed: 1
  tests_added: 0
  files_modified: 1
  commits: 1
  completed_at: "2026-02-15T15:00:33Z"
---

# Phase 14 Plan 02: Benchmark Price Enrichment Pipeline Summary

Phase 3 benchmark price enrichment added to enrich-prices command, fetching SPY and sector ETF prices for alpha calculation.

## What Was Built

### Module Documentation
- Updated module doc comment to reflect three-phase enrichment (historical, current, benchmark)

### Data Structures
- `BenchmarkPriceResult`: Message struct for concurrent benchmark fetch results with Vec<i64> tx_ids

### Functions
- `get_benchmark_ticker(gics_sector: Option<&str>) -> &'static str`: Maps 11 GICS sectors to SPDR ETF tickers with SPY fallback
  - Communication Services -> XLC
  - Consumer Discretionary -> XLY
  - Consumer Staples -> XLP
  - Energy -> XLE
  - Financials -> XLF
  - Health Care -> XLV
  - Industrials -> XLI
  - Information Technology -> XLK
  - Materials -> XLB
  - Real Estate -> XLRE
  - Utilities -> XLU
  - Default (no sector) -> SPY

### Pipeline Changes
- Phase 3 added after Phase 2, before final summary
- Independent query via `db.get_benchmark_unenriched_trades(args.batch_size)`
- Deduplication by `(benchmark_ticker, date)` to prevent redundant API calls
- Concurrent fetch pattern: Semaphore + JoinSet + mpsc (same as Phase 1)
- Jittered delay 200-500ms per request (same as Phase 1)
- Circuit breaker with threshold 10 (same as Phase 1)
- Progress bar with same style as Phase 1/2
- Weekend/holiday fallback via `get_price_on_date_with_fallback` (reuses YahooClient logic)

### Summary Output
- Updated final summary to include Phase 3 stats
- Circuit breaker check now considers both Phase 1 and Phase 3 breakers

## Technical Implementation

**get_benchmark_ticker Pattern:**
```rust
fn get_benchmark_ticker(gics_sector: Option<&str>) -> &'static str {
    match gics_sector {
        Some("Communication Services") => "XLC",
        Some("Consumer Discretionary") => "XLY",
        // ... 9 more sectors
        _ => "SPY",
    }
}
```

**Deduplication Map:**
```rust
let mut benchmark_date_map: HashMap<(String, NaiveDate), Vec<i64>> = HashMap::new();
for trade in &benchmark_trades {
    let date = NaiveDate::parse_from_str(&trade.tx_date, "%Y-%m-%d")?;
    let benchmark_ticker = get_benchmark_ticker(trade.gics_sector.as_deref());
    benchmark_date_map
        .entry((benchmark_ticker.to_string(), date))
        .or_default()
        .push(trade.tx_id);
}
```

**Concurrent Fetch:**
```rust
join_set3.spawn(async move {
    let _permit = sem.acquire().await.expect("semaphore closed");
    let delay_ms = rand::thread_rng().gen_range(200..500);
    sleep(Duration::from_millis(delay_ms)).await;

    let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;
    let _ = sender.send(BenchmarkPriceResult { trade_indices: tx_ids, result }).await;
});
```

**DB Update:**
```rust
match fetch.result {
    Ok(Some(price)) => {
        for tx_id in &fetch.trade_indices {
            db.update_benchmark_price(*tx_id, Some(price))?;
            benchmark_enriched += 1;
        }
        breaker3.record_success();
    }
    Ok(None) | Err(_) => {
        for tx_id in &fetch.trade_indices {
            db.update_benchmark_price(*tx_id, None)?;
            benchmark_skipped += 1;
        }
        breaker3.record_failure();
    }
}
```

## Deviations from Plan

None - plan executed exactly as written.

## Verification Results

- cargo check passes: VERIFIED
- cargo clippy --workspace passes with no warnings: VERIFIED
- cargo test --workspace passes (519 total tests): VERIFIED
- "Phase 3" found in enrich_prices.rs: VERIFIED
- "get_benchmark_ticker" found in enrich_prices.rs: VERIFIED
- "BenchmarkPriceResult" found in enrich_prices.rs: VERIFIED
- "get_benchmark_unenriched_trades" found in enrich_prices.rs: VERIFIED
- "update_benchmark_price" found in enrich_prices.rs: VERIFIED

## Files Changed

### capitoltraders_cli/src/commands/enrich_prices.rs
- Updated module doc comment (lines 3-6)
- Added BenchmarkPriceResult struct (lines 54-58)
- Added get_benchmark_ticker function (lines 84-101)
- Added Phase 3 enrichment loop (lines 376-472)
- Updated final summary to include Phase 3 stats (lines 475-496)
- Updated circuit breaker check to consider Phase 3 (lines 498-503)

## Commits

- a1c04b4: feat(14-02): add Phase 3 benchmark price enrichment to enrich-prices

## Self-Check: PASSED

All files and commits verified:
- FOUND: capitoltraders_cli/src/commands/enrich_prices.rs
- FOUND: a1c04b4 (feat commit)

## Next Steps

Phase 14 is complete. User can now run `capitoltraders enrich-prices --db path/to/db.sqlite` and see three phases:
1. Historical trade prices by (ticker, date)
2. Current prices by ticker
3. Benchmark prices by (ETF, date) based on GICS sector mapping

This provides all data needed for Phase 15 to calculate alpha (trade return minus benchmark return).
