# Domain Pitfalls: Congressional Trade Analytics

**Domain:** Performance scoring and anomaly detection
**Researched:** 2026-02-14

## Critical Pitfalls

Mistakes that cause rewrites or major issues.

### Pitfall 1: Comparing Returns Without Time Normalization
**What goes wrong:** "Rep A has 50% return, Rep B has 20% return" but A traded for 3 years, B traded for 6 months. Raw comparison is misleading.

**Why it happens:** Simple aggregation of all-time P&L ignores time dimension.

**Consequences:**
- Users trust leaderboards that don't adjust for holding period
- Short-term traders penalized vs long-term holders
- Complaints about "unfair rankings"
- Need to rebuild scoring system with annualized returns

**Prevention:**
- Calculate annualized return: `(total_return / days_held) * 365`
- Require date range filter (YTD, 1Y, 3Y) for leaderboards
- Don't show "all-time" rankings without context
- Document methodology in CLI help text

**Detection:**
- Test with synthetic data: 50% return in 6 months vs 50% in 3 years
- If same score, annualization is missing

### Pitfall 2: Benchmark Survivorship Bias
**What goes wrong:** Only storing benchmark prices from today backward. If a politician traded 5 years ago, but benchmark data only goes back 2 years, comparison is impossible.

**Why it happens:** Fetching benchmark prices only when needed, not maintaining historical backfill.

**Consequences:**
- Cannot calculate relative returns for old trades
- Leaderboards show "N/A" for long-term politicians
- Need to backfill years of daily SPY prices (expensive, rate-limited)
- Incomplete analytics for active traders with old positions

**Prevention:**
- First `sync-benchmarks` run fetches full history (e.g., 10 years back)
- Incremental updates only fetch since last sync
- Store `last_sync_date` in metadata table
- Validate benchmark coverage before calculating scores

**Detection:**
- Check MIN(price_date) in benchmark_prices table
- If later than MIN(tx_date) in trades table, coverage is insufficient

### Pitfall 3: Sector Mapping Staleness
**What goes wrong:** Static YAML file has `AAPL: XLK` but Apple changes sector classification (rare but happens). Sector benchmarks become inaccurate.

**Why it happens:** GICS sector classifications are updated periodically (e.g., Facebook moved from Tech to Communication Services in 2018).

**Consequences:**
- Incorrect sector ETF benchmark for affected tickers
- Committee-sector overlap flags miss conflicts
- Users notice "Rep traded tech stocks but no XLK conflict flagged"
- Manual YAML updates required for each reclassification

**Prevention:**
- Add `last_updated` field to sector_mappings.yaml entries
- Log warnings for mappings older than 1 year
- Provide CLI command to validate mappings against Yahoo Finance sector API
- Document manual update process in README

**Detection:**
- Compare sector_mappings.yaml against Yahoo Finance API spot-check
- If mismatch, log warning during CLI execution

### Pitfall 4: Option Trade Inclusion in Win Rate
**What goes wrong:** Option trades (excluded from FIFO per v1.1) accidentally counted in win rate calculation, inflating success metrics.

**Why it happens:** DB query fetches all trades, doesn't filter by asset_type.

**Consequences:**
- Win rate artificially high (or low, depending on option performance)
- Inconsistent with portfolio P&L (which excludes options)
- Users notice "win rate 80% but portfolio return 5%"
- Need to recalculate all historical scores

**Prevention:**
- Filter `WHERE asset_type != 'stock-option'` in all analytics queries
- Document exclusion in CLI help text
- Add test: verify option trades excluded from win rate calculation
- Centralize filter logic in shared helper function

**Detection:**
- Compare trade count in portfolio vs win rate calculation
- If mismatch, options are leaking into analytics

### Pitfall 5: Percentile Rank Division by Zero
**What goes wrong:** Only one politician has trades. `percentile_rank()` divides by `values.len()` which is 1, gives 0% for the only trader.

**Why it happens:** Edge case not tested during development.

**Consequences:**
- Panic on unwrap or NaN propagation through calculations
- Composite score becomes NaN
- CLI crashes or outputs garbage

**Prevention:**
- Guard `percentile_rank()` with `if values.len() <= 1 { return 50.0; }`
- Test with 0, 1, 2, 3 politicians
- Return sensible default (50th percentile) for single value

**Detection:**
- Unit test: `percentile_rank(5.0, &[5.0])` should return 50.0, not NaN

## Moderate Pitfalls

### Pitfall 6: Ignoring Partial Fills in Trade Volume
**What goes wrong:** Trade shows "$50K-$100K" range, estimated at $75K. But politician later discloses only bought $15K. Volume metrics are inflated.

**Why it happens:** Estimation uses midpoint of range, actual amounts not disclosed in STOCK Act filings.

**Consequences:**
- Overstated total volume traded
- Sector concentration HHI is skewed
- Users notice "Rep traded $10M" but filing shows far less

**Prevention:**
- Flag estimated values in output (e.g., "~$75K")
- Document estimation methodology
- If actual amounts become available (future API), allow override
- Use range bounds for sensitivity analysis (min/max scenarios)

**Detection:**
- Compare estimated vs actual (if actual becomes available)
- Log wide ranges (>$500K spread) as high-uncertainty

### Pitfall 7: Committee Name Variations
**What goes wrong:** Committee stored as "House Energy and Commerce" in DB, but committee_sectors.yaml uses "House Committee on Energy and Commerce". Overlap detection fails.

**Why it happens:** Inconsistent naming between OpenFEC API and static mapping file.

**Consequences:**
- Zero conflict flags detected
- Users manually verify and find conflicts
- Need to regenerate mappings with name normalization

**Prevention:**
- Normalize committee names (lowercase, remove "Committee on", trim whitespace)
- Use fuzzy matching (Jaro-Winkler) for committee lookups
- Validate committee_sectors.yaml against actual DB committee names on startup
- Log warnings for unmapped committees

**Detection:**
- Check `SELECT DISTINCT committee FROM politicians` vs YAML keys
- If non-empty diff, normalization or coverage issue exists

### Pitfall 8: Benchmark Price Gaps (Weekends/Holidays)
**What goes wrong:** Politician trades on Friday, Yahoo Finance has no price for Saturday/Sunday. Benchmark comparison fails or uses stale Monday price.

**Why it happens:** Stock markets closed on weekends/holidays, but trade disclosure dates may reference non-trading days.

**Consequences:**
- Benchmark return calculation errors
- Users see "N/A" for weekend trades
- Inconsistent comparison methodology

**Prevention:**
- Reuse existing `get_price_on_date_with_fallback()` pattern from v1.1
- Fallback: try date-1, date-2, ..., date-7 until price found
- Log fallback usage for transparency
- Document in CLI help: "weekend trades use previous trading day price"

**Detection:**
- Test with known weekend trade date (e.g., 2024-12-25, Christmas)
- Verify fallback logic returns previous trading day

### Pitfall 9: Composite Score Weight Misconfiguration
**What goes wrong:** User sets custom weights `--weight-returns 0.9 --weight-win-rate 0.1` but forgets other weights. Total != 1.0, percentile scaling breaks.

**Why it happens:** CLI allows partial weight specification without validation.

**Consequences:**
- Scores not normalized to 0-100 scale
- Leaderboard order is nonsensical
- Users lose trust in analytics

**Prevention:**
- Validate weights sum to 1.0 (or normalize to sum if not)
- Default weights always defined (40/20/20/20)
- Document weight behavior in --help
- Return error if weights sum to 0.0

**Detection:**
- Unit test: `assert_eq!(weights.sum(), 1.0);`
- CLI integration test with invalid weights

### Pitfall 10: Anomaly Detection False Positives from Volatility
**What goes wrong:** Entire tech sector moves +15% due to AI hype. Every tech stock trade flagged as "pre-move anomaly."

**Why it happens:** Absolute price threshold (e.g., +10%) doesn't account for sector-wide movements.

**Consequences:**
- Anomaly flags become noise
- Users ignore flagged trades
- True insider timing signals drowned out

**Prevention:**
- Use relative price movement: stock return - sector ETF return
- Only flag if `stock_return - sector_return > threshold` (e.g., +5% outperformance)
- Document methodology: "anomaly = stock outperforms sector by >5% within 30 days"
- Add CLI flag `--anomaly-threshold` for customization

**Detection:**
- Test with 2020 COVID crash (all stocks down 30%): should have zero anomalies
- Test with stock outperforming sector: should have anomalies

## Minor Pitfalls

### Pitfall 11: Hardcoded Benchmark Ticker List
**What goes wrong:** Code has `const BENCHMARKS: &[&str] = &["SPY", "XLK", ...]`. New sector ETF added (e.g., XBI for biotech), requires code change.

**Why it happens:** Convenience vs extensibility tradeoff.

**Consequences:**
- Recompilation needed for new benchmarks
- Users can't add custom benchmarks
- Low friction but not ideal

**Prevention:**
- Move to YAML config file (e.g., `data/benchmarks.yaml`)
- Allow CLI flag `--benchmarks path/to/custom.yaml`
- Default to built-in list if no custom file

**Detection:**
- Feature request for custom benchmark support

### Pitfall 12: Date Range Filter Inclusivity Ambiguity
**What goes wrong:** User runs `--start-date 2024-01-01 --end-date 2024-12-31`. Are endpoints included or excluded? Confusing if not documented.

**Why it happens:** SQL BETWEEN is inclusive, but users may expect exclusive end.

**Consequences:**
- Off-by-one errors in date filtering
- Users manually verify and find extra/missing trades

**Prevention:**
- Document in CLI help: "start and end dates are inclusive"
- Use SQL `WHERE tx_date >= ? AND tx_date <= ?`
- Test boundary conditions (trade on exact start/end date)

**Detection:**
- Integration test with trade on 2024-01-01, query `--start-date 2024-01-01`
- Verify trade is included

### Pitfall 13: Null Prices Breaking Calculations
**What goes wrong:** Trade has `trade_date_price = NULL` (enrichment failed). Return calculation does `(sell_price - buy_price) / buy_price` and panics.

**Why it happens:** Enrichment pipeline circuit breaker, Yahoo Finance API errors.

**Consequences:**
- CLI crash on score calculation
- Partial leaderboard results
- Users frustrated

**Prevention:**
- Skip trades with NULL prices in analytics
- Log warning: "X trades excluded due to missing prices"
- Provide `enrich-prices --retry-failed` to backfill
- Test with NULL prices in dataset

**Detection:**
- Unit test: calculate_return() with NULL buy_price returns None, not panic

### Pitfall 14: Committee Assignment Churn
**What goes wrong:** Politician switches committees mid-term. Old trades flagged with new committee, incorrect conflict detection.

**Why it happens:** Committee data is point-in-time (current assignment), not historical.

**Consequences:**
- False positives: "Rep traded energy stocks before joining Energy Committee"
- Users notice incorrect conflict flags

**Prevention:**
- Use trade date to lookup committee assignment (if historical data available)
- If not available, flag as "current committee only (may not reflect assignment at trade time)"
- Document limitation in CLI help

**Detection:**
- Manually verify politician who switched committees
- Check if old trades flagged with new committee

### Pitfall 15: Floating Point Precision in HHI
**What goes wrong:** Sector weights are 0.333333... (repeating). HHI calculation accumulates rounding errors, results in 2499.98 instead of 2500.00.

**Why it happens:** IEEE 754 float representation.

**Consequences:**
- HHI threshold check (`> 2500`) misses edge case
- Inconsistent output across platforms (x86 vs ARM)

**Prevention:**
- Round HHI to 2 decimal places before comparison
- Use integer threshold comparison: `(hhi * 100.0) as i64 > 250000`
- Document precision in tests

**Detection:**
- Unit test: HHI for equal 4-sector portfolio should be exactly 2500.0

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| **Benchmark Sync** | Survivorship bias (Pitfall 2) | Backfill full history first run |
| **Score Calculation** | Time normalization (Pitfall 1) | Require date range, annualize returns |
| **Win Rate** | Option trade inclusion (Pitfall 4) | Filter asset_type in queries |
| **Sector Mapping** | Staleness (Pitfall 3) | Log warnings for old mappings |
| **Anomaly Detection** | False positives (Pitfall 10) | Use sector-relative thresholds |
| **Composite Scoring** | Weight misconfiguration (Pitfall 9) | Validate weights sum to 1.0 |
| **Committee Conflicts** | Name variations (Pitfall 7) | Normalize names, fuzzy matching |
| **CLI Output** | Null prices (Pitfall 13) | Skip nulls, log warnings |

## Sources

- Existing codebase pitfalls (enrichment pipeline, FIFO edge cases, committee resolver) reviewed
- [Unusual Whales Congress Trading Report 2025](https://unusualwhales.com/congress-trading-report-2025) - "Only 32.2% beat market in 2025" (survivorship bias context)
- [Stock Market Volume Anomaly Detection | SliceMatrix](https://slicematrix.github.io/stock_market_anomalies.html) - False positive reduction techniques
- [How To Analyze Portfolio For Concentration Risk | Financial Samurai](https://www.financialsamurai.com/how-to-analyze-investment-portfolio-for-concentration-risk-sector-exposure-style/) - HHI calculation precision issues
