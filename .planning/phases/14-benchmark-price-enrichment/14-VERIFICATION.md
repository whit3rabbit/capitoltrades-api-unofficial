---
phase: 14-benchmark-price-enrichment
verified: 2026-02-15T15:30:00Z
status: passed
score: 18/18 must-haves verified
re_verification: false
---

# Phase 14: Benchmark Price Enrichment Verification Report

**Phase Goal:** Users can enrich trades with S&P 500 and sector ETF benchmark prices
**Verified:** 2026-02-15T15:30:00Z
**Status:** passed
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can run enrich-prices command which fetches benchmark prices in Phase 3 | ✓ VERIFIED | Phase 3 loop exists at enrich_prices.rs:373-474, calls get_benchmark_unenriched_trades, spawns concurrent fetch tasks, writes via update_benchmark_price |
| 2 | User can see benchmark_price column populated for trades with valid trade dates | ✓ VERIFIED | benchmark_price REAL column in schema/sqlite.sql:69, update_benchmark_price writes Some(f64) values at enrich_prices.rs:443 |
| 3 | User can see 12 benchmark tickers cached (SPY + 11 sector ETFs) | ✓ VERIFIED | get_benchmark_ticker maps 11 GICS sectors to ETF tickers (XLC, XLY, XLP, XLE, XLF, XLV, XLI, XLK, XLB, XLRE, XLU) + SPY fallback, YahooClient DashMap cache reused |
| 4 | Weekend/holiday dates fall back to previous trading day for benchmark prices | ✓ VERIFIED | Phase 3 calls yahoo.get_price_on_date_with_fallback (enrich_prices.rs:424) which implements 7-day lookback |
| 5 | Circuit breaker stops enrichment if 10+ consecutive benchmark price failures | ✓ VERIFIED | breaker3.is_tripped() check at enrich_prices.rs:460, CIRCUIT_BREAKER_THRESHOLD=10, abort_all on trip |

**Score:** 5/5 truths verified

### Required Artifacts (Plan 14-01)

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `capitoltraders_lib/src/db.rs` | migrate_v7, BenchmarkEnrichmentRow, get_benchmark_unenriched_trades, update_benchmark_price | ✓ VERIFIED | migrate_v7 at line 333 (ALTER TABLE + CREATE INDEX), BenchmarkEnrichmentRow at line 3569 (tx_id, issuer_ticker, tx_date, gics_sector), get_benchmark_unenriched_trades at line 1536 (JOIN issuers for gics_sector, WHERE benchmark_price IS NULL), update_benchmark_price at line 1583 (UPDATE trades SET benchmark_price) |
| `schema/sqlite.sql` | benchmark_price column, idx_trades_benchmark_price index | ✓ VERIFIED | benchmark_price REAL at line 69, idx_trades_benchmark_price at line 246 |

### Required Artifacts (Plan 14-02)

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `capitoltraders_cli/src/commands/enrich_prices.rs` | Phase 3 benchmark enrichment loop, get_benchmark_ticker mapping, BenchmarkPriceResult struct | ✓ VERIFIED | BenchmarkPriceResult at line 55 (Vec<i64> tx_ids), get_benchmark_ticker at line 91 (11 GICS sectors to ETFs), Phase 3 loop at lines 373-474 (dedup by (ETF, date), concurrent fetch, circuit breaker) |

### Key Link Verification (Plan 14-01)

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| db.rs::init() | db.rs::migrate_v7() | version < 7 guard | ✓ WIRED | Line 92-94: `if version < 7 { self.migrate_v7()?; self.conn.pragma_update(None, "user_version", 7)?; }` |
| db.rs::get_benchmark_unenriched_trades | issuers table | JOIN issuers for gics_sector | ✓ WIRED | Lines 1542-1544 and 1553-1555: `SELECT ... i.gics_sector FROM trades t JOIN issuers i ON t.issuer_id = i.issuer_id` |

### Key Link Verification (Plan 14-02)

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| enrich_prices.rs::run() | db.get_benchmark_unenriched_trades() | separate query for benchmark-unenriched trades | ✓ WIRED | Line 374: `let benchmark_trades = db.get_benchmark_unenriched_trades(args.batch_size)?;` |
| enrich_prices.rs::get_benchmark_ticker() | GICS sector names | const match on sector string | ✓ WIRED | Lines 92-104: match statement with 11 GICS sectors matching GICS_SECTORS constant exactly |
| enrich_prices.rs Phase 3 loop | yahoo.get_price_on_date_with_fallback() | same YahooClient used in Phase 1 | ✓ WIRED | Line 424: `let result = yahoo_clone.get_price_on_date_with_fallback(&ticker, date).await;` |
| enrich_prices.rs Phase 3 receiver | db.update_benchmark_price() | single-threaded DB writes from channel | ✓ WIRED | Lines 443 and 451: `db.update_benchmark_price(*tx_id, Some(price))?;` and `db.update_benchmark_price(*tx_id, None)?;` |

### Requirements Coverage

| Requirement | Status | Blocking Issue |
|-------------|--------|---------------|
| FOUND-03: User can enrich benchmark prices via Yahoo Finance during enrich-prices run | ✓ SATISFIED | All supporting truths verified. Phase 3 fetches benchmark prices from Yahoo Finance using sector-to-ETF mapping, writes to benchmark_price column |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| enrich_prices.rs | 188, 318, 406 | unwrap() on ProgressStyle template | ℹ️ Info | Acceptable - hardcoded strings that never fail |
| db.rs | 2060, 2068, 2076, 2084, 2092, 2100 | "placeholders" in variable names | ℹ️ Info | Not anti-pattern - legitimate SQL placeholder usage |

**No blocker or warning anti-patterns found.**

### Human Verification Required

None - all verification criteria are programmatically verifiable and passed automated checks.

## Detailed Verification

### Plan 14-01: Schema v7 Migration

**Truth 1:** Schema v7 migration adds benchmark_price REAL column to trades table on existing databases
- ✓ migrate_v7 method exists (db.rs:333-358)
- ✓ ALTER TABLE trades ADD COLUMN benchmark_price REAL with duplicate column error handling
- ✓ CREATE INDEX IF NOT EXISTS idx_trades_benchmark_price with no-such-table error handling
- ✓ Version guard in init() at lines 92-94
- ✓ Tests: test_migrate_v7_from_v6, test_migrate_v7_idempotent passed

**Truth 2:** Fresh databases include benchmark_price column in base schema
- ✓ benchmark_price REAL in schema/sqlite.sql:69
- ✓ idx_trades_benchmark_price index in schema/sqlite.sql:246
- ✓ Test: test_v7_version_check passed (fresh DB has user_version=7)

**Truth 3:** get_benchmark_unenriched_trades returns trades where benchmark_price IS NULL with gics_sector from issuers JOIN
- ✓ Method exists at db.rs:1536-1577
- ✓ SQL query JOINs issuers table for gics_sector (lines 1542-1544, 1553-1555)
- ✓ WHERE clause filters benchmark_price IS NULL (lines 1548, 1559)
- ✓ Returns BenchmarkEnrichmentRow with gics_sector field
- ✓ Tests: test_get_benchmark_unenriched_trades, test_get_benchmark_unenriched_trades_with_limit passed

**Truth 4:** update_benchmark_price writes benchmark_price for a single trade by tx_id
- ✓ Method exists at db.rs:1583-1595
- ✓ SQL: UPDATE trades SET benchmark_price = ?1 WHERE tx_id = ?2
- ✓ Accepts Option<f64> (handles both Some and None)
- ✓ Does NOT touch price_enriched_at (independent enrichment)
- ✓ Test: test_update_benchmark_price passed

### Plan 14-02: Phase 3 Benchmark Enrichment

**Truth 1:** Running enrich-prices fetches benchmark prices in a third phase after historical and current price enrichment
- ✓ Phase 3 code at enrich_prices.rs:373-474 (after Phase 2 at line 372)
- ✓ Independent query: db.get_benchmark_unenriched_trades(args.batch_size)
- ✓ Progress bar with Phase 3 labeling
- ✓ Summary includes Phase 3 stats (lines 489-491)

**Truth 2:** Benchmark prices are deduplicated by (ETF ticker, date) so each unique pair is fetched only once
- ✓ HashMap<(String, NaiveDate), Vec<i64>> benchmark_date_map (lines 379-392)
- ✓ Dedup loop iterates benchmark_trades, groups by (benchmark_ticker, date)
- ✓ Single fetch per unique pair, multiple tx_ids updated from same result
- ✓ Progress bar shows unique_pairs count (line 395)

**Truth 3:** Trades with GICS sector get sector-specific ETF benchmark; trades without get SPY fallback
- ✓ get_benchmark_ticker function at lines 91-106
- ✓ Match statement handles 11 GICS sectors (exact match to GICS_SECTORS constant)
- ✓ Default case returns "SPY" for None or unrecognized sectors
- ✓ Sector-to-ETF mapping:
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

**Truth 4:** Circuit breaker stops Phase 3 after 10 consecutive benchmark price failures
- ✓ breaker3 initialized with CIRCUIT_BREAKER_THRESHOLD (10) at line 437
- ✓ record_success on Ok(Some(price)) at line 446
- ✓ record_failure on Ok(None) or Err at line 454
- ✓ is_tripped check at line 460, abort_all on trip at line 465

**Truth 5:** Weekend/holiday dates fall back to previous trading day for benchmark prices
- ✓ Phase 3 calls yahoo_clone.get_price_on_date_with_fallback(&ticker, date) at line 424
- ✓ Same fallback logic as Phase 1 (7-day lookback in YahooClient)
- ✓ DashMap cache shared across all phases (Arc<YahooClient> reused)

**Truth 6:** Phase 3 runs independently -- works even if user already ran Phases 1 and 2 previously
- ✓ Separate query: get_benchmark_unenriched_trades returns only trades with NULL benchmark_price
- ✓ Not dependent on Phase 1/2 `trades` vec (which may be empty if already enriched)
- ✓ Independent semaphore (semaphore3), circuit breaker (breaker3), progress bar (pb3)

## Test Results

**Test suite:** 538 tests passed, 0 failed

**Phase 14 specific tests (6 new):**
- test_migrate_v7_from_v6: PASSED
- test_migrate_v7_idempotent: PASSED
- test_v7_version_check: PASSED
- test_get_benchmark_unenriched_trades: PASSED
- test_get_benchmark_unenriched_trades_with_limit: PASSED
- test_update_benchmark_price: PASSED

**Regression tests:** All 532 existing tests still pass (version assertions updated from 6 to 7)

## Files Changed

### Plan 14-01
- schema/sqlite.sql: Added benchmark_price column (line 69), added index (line 246)
- capitoltraders_lib/src/db.rs: Added migrate_v7 (line 333), BenchmarkEnrichmentRow (line 3569), get_benchmark_unenriched_trades (line 1536), update_benchmark_price (line 1583), 6 tests (lines 8652-8849), updated 15 version assertions

### Plan 14-02
- capitoltraders_cli/src/commands/enrich_prices.rs: Updated module doc (line 6), added BenchmarkPriceResult (line 55), added get_benchmark_ticker (line 91), added Phase 3 loop (lines 373-474), updated summary (lines 489-491, 498-503)

## Commits

**Plan 14-01:**
- 12e8d51: feat(14-01): add schema v7 migration and benchmark DB methods
- c96ac7a: test(14-01): add schema v7 migration and benchmark DB method tests

**Plan 14-02:**
- a1c04b4: feat(14-02): add Phase 3 benchmark price enrichment to enrich-prices

All commits verified in git log.

## Gaps Summary

No gaps found. All must-haves verified:
- **Plan 14-01:** 4/4 truths verified, 2/2 artifacts verified, 2/2 key links wired
- **Plan 14-02:** 6/6 truths verified, 1/1 artifact verified, 4/4 key links wired
- **Requirements:** FOUND-03 satisfied
- **Tests:** 6 new tests pass, 0 regressions
- **Commits:** 3 commits verified

Phase 14 goal achieved. Users can now enrich trades with S&P 500 and sector ETF benchmark prices via Phase 3 of the enrich-prices command. The implementation follows established patterns (Semaphore + JoinSet + mpsc concurrency, circuit breaker, weekend fallback), reuses the YahooClient cache, and properly maps GICS sectors to SPDR sector ETFs.

---

_Verified: 2026-02-15T15:30:00Z_
_Verifier: Claude (gsd-verifier)_
