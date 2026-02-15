# Project Research Summary

**Project:** Capitol Traders v1.3 Analytics & Scoring
**Domain:** Congressional Stock Trade Performance Analysis
**Researched:** 2026-02-14
**Confidence:** HIGH

## Executive Summary

Capitol Traders v1.3 adds performance analytics and anomaly detection to congressional stock trade tracking. The research synthesizes findings across two distinct research domains: the v1.2 FEC donation integration (completed) provides context for correlation features, while v1.3 analytics (current milestone) requires no new external dependencies. The recommended approach leverages existing price enrichment infrastructure (v1.1) with pure Rust stdlib calculations for scoring, static YAML files for sector classification, and schema v6 extensions for storing analytical metrics.

The critical architectural insight is that analytics builds entirely on existing enriched data: trade date prices and current prices (v1.1) plus benchmark prices (new Phase 3 of enrich-prices) provide all inputs for alpha scoring, win rate calculation, and anomaly detection. No external stats libraries needed for v1.3 table stakes - percentile ranking, batting average, and HHI calculations are trivial stdlib operations. The differentiator features (sector ETF benchmarking, committee-sector overlap, donation-trade correlation) rely on mappings rather than complex computation, keeping the implementation lightweight.

The seven critical pitfalls identified center on data integrity challenges unique to congressional trading: 30-45 day disclosure delays create false "predictive" signals, imprecise share estimation from dollar ranges introduces 40-70% variance, survivorship bias from delisted stocks inflates returns by 14%, look-ahead bias using future prices corrupts comparisons, small sample sizes (10-50 trades/politician/year) generate 60-80% false positive rates in anomaly detection, benchmark mismatch across sector ETFs changes rankings by 20%, and correlation-causation confusion in committee overlap requires confounding variable controls. Prevention requires strict date discipline, confidence interval storage, statistical rigor (p<0.01 thresholds, minimum N=30), and transparent methodology disclosure.

## Key Findings

### Recommended Stack (v1.3 Analytics)

**No new dependencies required.** The v1.3 analytics milestone leverages existing workspace infrastructure: yahoo_finance_api 4.1.0 for benchmark ETF prices (SPY, XLK, XLF, etc.), rusqlite 0.32 for schema v6 extensions, chrono 0.4 for date range filtering, serde_yml 0.0.12 (from v1.2) for sector/committee mapping YAML files. External statistics libraries (statrs, linregress) deferred to v2.x as they're not needed for table stakes features.

**Core technologies:**
- **Rust stdlib** for analytics calculations - percentile ranking, win rate, HHI, rolling returns all implemented as pure functions without external dependencies
- **Static YAML mappings** for sector classification - top 200 traded tickers hardcoded (ticker -> GICS sector -> benchmark ETF), Unknown sector defaults to SPY benchmark, no runtime API dependency
- **YahooClient extension** (existing v1.1) - add get_benchmark_price_on_date() method reusing DashMap cache, fetch 12 benchmark tickers (SPY + 11 sector ETFs), integrate as Phase 3 of enrich-prices pipeline
- **Schema v6 migration** - add 8 analytics columns to trades table (alpha_score, abnormal_return, benchmark_price, sector_id, etc.), create sector_benchmarks reference table with 11 GICS sectors

**Key architectural decision:** Extend enrich-prices command with benchmark enrichment (Phase 3) rather than separate sync-benchmarks command. This keeps all Yahoo Finance interaction in one place, reuses existing Semaphore+JoinSet+mpsc concurrency pattern, and preserves circuit breaker logic.

### Expected Features (v1.3)

Research identified clear table stakes vs differentiators based on competitive analysis of InsiderFinance, Quiver Quantitative, Unusual Whales, and Capitol Trades platforms.

**Must have (table stakes):**
- Absolute return calculation per trade and per politician - industry standard, all trackers show this
- S&P 500 benchmark comparison (outperformance/underperformance) - expected baseline comparison
- Politician leaderboard with percentile ranks - core ranking feature for all congressional trade platforms
- Win rate (batting average) - percentage of profitable trades, standard portfolio metric
- Time period filters (YTD, 1Y, 3Y, all-time) - temporal analysis is expected for "who did best in 2025?" queries
- Trade count and volume aggregation - basic transparency metrics

**Should have (competitive differentiators):**
- Sector ETF benchmarking - "Rep bought tech stocks, did they beat XLK?" More precise than S&P 500, high-value feature
- Committee-sector overlap detection - "Rep on Energy Committee trades oil stocks" conflict signal unavailable elsewhere
- Anomaly detection for pre-move trades - "Bought 3 days before +20% move" insider-timing signal
- Donation-trade correlation flags - "Top donor is defense contractor, Rep trades defense stocks" links v1.2 donation data to trades
- Composite score - single 0-100 number combining returns, timing, conflicts, concentration for trivial comparison

**Defer to v2.x (anti-features for v1.3):**
- Real-time alerts - CLI is batch-oriented, no event-driven infrastructure
- Option strategy analysis - strikes, expiries, Greeks require complex pricing models beyond PROJECT.md scope
- Interactive charting - CLI output is text/CSV/JSON, charts belong in external visualization tools
- Machine learning predictions - "Will Rep X buy next week?" high complexity, low reliability, liability concerns
- Backtesting trades - "What if I copied?" veers into financial advice territory

### Architecture Approach (v1.3)

The architecture extends the established 3-crate Rust workspace pattern: domain logic in capitoltraders_lib (new sector.rs and scoring.rs modules), CLI dispatch in capitoltraders_cli (extend enrich_prices.rs, new analytics.rs command), DB operations in db.rs (schema v6 migration, new analytics query methods). Pure calculation functions follow portfolio.rs pattern (ScoreComponents struct return, no DB/network dependencies, 100% unit testable). Enrichment pipeline reuses enrich_prices.rs Semaphore+JoinSet+mpsc concurrency with benchmark price fetching as Phase 3. Output formatting extends existing output.rs with print_analytics_* functions for all 5 formats.

**Major components:**
1. **sector.rs module** (new) - GicsSector enum, static SECTOR_MAP HashMap for common tickers, classify_ticker() pure function, benchmark_for_sector() mapping (sector -> XLK/XLF/etc.), parallels committee.rs structure
2. **scoring.rs module** (new) - calculate_alpha_score() pure function (trade_return - benchmark_return), calculate_abnormal_return() (dollar P&L vs benchmark expectation), ScoreComponents struct with all analytical metrics, mirrors portfolio.rs pure function pattern
3. **YahooClient extension** - get_benchmark_price_on_date() method added, reuses existing DashMap cache with (ticker, date) key, fetches SPY/sector ETF prices using same weekend fallback logic, no architectural changes to yahoo.rs
4. **Schema v6 migration** - adds benchmark_price, sector_benchmark_price, alpha_score, abnormal_return, sector_id, analytics_enriched_at columns to trades table, creates sector_benchmarks table with 11 GICS sectors + benchmark tickers, composite indexes for analytics queries
5. **enrich-prices Phase 3** - extends existing command with benchmark enrichment after Phase 2 current prices, deduplicates by (benchmark_ticker, date), spawns concurrent tasks for 12 benchmarks (SPY + 11 sectors), updates trades.benchmark_price column
6. **analytics CLI command** (new) - AnalyticsArgs with filters (politician, party, min-alpha, sector), query_analytics_trades() DB method with dynamic WHERE clause, output dispatch following portfolio.rs model

**Integration pattern:** Analytics is entirely downstream of existing enrichment. Dependency chain: trades synced -> prices enriched (Phases 1+2) -> benchmarks enriched (Phase 3) -> analytics calculated (new enrich-analytics command) -> analytics queried (analytics command). No circular dependencies, clean separation of concerns.

### Critical Pitfalls (Top 7)

Research across both v1.2 FEC integration and v1.3 analytics identified distinct pitfall categories. FEC pitfalls (employer fuzzy matching, committee name variations, disclosure delay artifacts) inform correlation features but don't block core analytics. Analytics-specific pitfalls are critical for v1.3:

1. **Look-Ahead Bias in Benchmark Comparison** (CRITICAL) - Using stock prices or financial data not available at trade notification date. Most common: comparing to "next day's opening price" when trade reported after market close, using dividend-adjusted returns including dividends paid after analysis date, applying retroactive GICS sector reclassifications. **Prevention:** Use disclosure_date (when trade became public) as benchmark comparison anchor NOT trade_date, store price_as_of_date timestamp alongside every price field, never compare using data timestamped after disclosure_date + 1 day. Walk-forward validation should show Sharpe ratio <1.5 (higher indicates look-ahead bias).

2. **Survivorship Bias Inflating Returns** (CRITICAL) - Excluding delisted, bankrupt, or acquired companies from performance calculations. Yahoo Finance only provides data for currently trading stocks, delisted price history disappears after 6-12 months. **Impact:** Studies show survivorship bias causes 14 percentage point underestimation of drawdowns and inflates annual returns by 8-14%. **Prevention:** Add delisting_date, final_price (settlement for acquisitions, 0.0 for bankruptcies), delisting_reason columns to issuers table, check SEC EDGAR Form 25 for delisting notices if Yahoo returns 404, include delisted stocks in portfolio return calculation using final_price.

3. **Imprecise Share Estimation Variance** (HIGH) - STOCK Act requires transaction value as range ($15,001-$50,000), not exact amount. Midpoint estimation creates 40-70% variance in calculated returns. For $15K-$50K range at $100/share, could be 150 shares (low end) or 500 shares (high end), P&L ranges from +$15K to +$85K. **Prevention:** Store amount_min, amount_max, estimated_shares_min, estimated_shares_max (not just midpoint), display ranges prominently ("Est. Shares: 150-500"), show P&L with error bars ("Profit: $25,000 ± $15,000"), require minimum 30+ trades before showing point estimates.

4. **Disclosure Delay Attribution Artifacts** (HIGH) - 30-45 day disclosure delay creates false predictive signals. Politician buying Moderna Jan 15 disclosed Feb 14, vaccine breakthrough announcement Feb 1 (stock jumps 30%), analysis shows "bought 2 weeks before public knew" when actually bought 2 weeks AFTER. **Prevention:** Three-date schema (trade_date, notification_date, disclosure_date), compare trade_date to news date for predictive analysis, compare disclosure_date for copyable analysis, flag only if trade_date precedes news AND disclosure_date is timely (<45 days).

5. **False Positive Clustering from Small Samples** (HIGH) - Congressional trades have 10-50 trades/politician/year (small sample). Standard anomaly detection produces 60-80% false positive rates. With 20 trades, probability of 5+ in same sector by chance alone is 35% (binomial distribution). **Prevention:** Require minimum N=30 before enabling anomaly detection, use p<0.01 threshold (not p<0.05) with Bonferroni correction for multiple comparisons, use Fisher's Exact Test (works with small samples) not Chi-Squared, ensemble methods with majority vote (3+/5 detectors) reduces false positives by 70%.

6. **Benchmark Mismatch and Dividend Adjustment** (MEDIUM) - Sector classification systems vary (GICS vs SIC), sector ETF methodologies differ (XLK excludes GOOGL/META, QQQ includes non-tech stocks), dividend-adjusted returns favor dividend sectors (financials, utilities) while price returns favor growth sectors (tech). **Prevention:** Store multiple benchmarks (S&P 500 + sector ETF + custom sector-weighted), label all metrics as "Price Return" vs "Total Return", use sector classification as of trade_date not current, document GICS version (2016/2018 restructurings), allow user benchmark selection in UI.

7. **Correlation Presented as Causation** (MEDIUM) - Committee membership correlates with sector trading (r=0.6-0.8) but confounding variables explain it: selection bias (senators interested in finance join Banking Committee AND prefer financial stocks), state economy (Texas senator on Energy Committee trades oil due to constituent interests), prior occupation. **Prevention:** Multi-variate analysis controlling for state industry, display multiple hypotheses ("Possible explanations: committee membership, prior career in banking, home state economy"), avoid causal language ("correlates with" not "explains"), propensity score matching to compare committee members to similar non-members.

## Implications for Roadmap

Based on research, v1.3 Analytics & Scoring should follow a 5-phase structure prioritizing table stakes features first, then layering differentiators that depend on enriched data. The v1.2 FEC integration provides donation data for Phase 4 correlation features but doesn't block earlier phases.

### Phase 1: Data Foundation & Sector Classification
**Rationale:** Sector mapping and schema extensions are prerequisites for all analytics. Static YAML approach avoids external API dependencies, enables parallel work on enrichment and scoring modules. Schema v6 must exist before benchmark enrichment can write data.

**Delivers:**
- sector.rs module with GicsSector enum and SECTOR_MAP (top 200 tickers hardcoded)
- Schema v6 migration (analytics columns on trades table, sector_benchmarks reference table)
- Static YAML files (data/sector_mappings.yaml, data/committee_sectors.yaml)
- Validation: sector classifications don't require API calls, migrations are idempotent

**Addresses features:**
- Sector ETF benchmarking foundation (sector classification)
- Committee-sector overlap foundation (committee mapping)
- HHI concentration analysis foundation (sector weights)

**Avoids pitfalls:**
- Pitfall 6 (Benchmark Mismatch) - GICS sector classification with effective date tracking prevents historical mismatches
- Sector Mapping Staleness - YAML last_updated field enables staleness warnings

**Research needed:** No additional research (sector mapping well-documented in GICS specification)

### Phase 2: Benchmark Price Enrichment
**Rationale:** Benchmark prices are enrichment data (like trade date prices in v1.1), not analytical output. Extending enrich-prices with Phase 3 reuses existing concurrency infrastructure and circuit breaker logic. Must complete before scoring because alpha calculations require benchmark prices.

**Delivers:**
- enrich-prices Phase 3 (benchmark price fetching after Phase 2 current prices)
- YahooClient::get_benchmark_price_on_date() method
- Benchmark price storage in trades.benchmark_price column
- 12 benchmark tickers supported (SPY + 11 sector ETFs)

**Uses stack:**
- yahoo_finance_api 4.1.0 (existing) for ETF price data
- Existing Semaphore+JoinSet+mpsc concurrency pattern
- Existing DashMap cache with weekend fallback logic

**Implements architecture:**
- Two-Phase Enrichment Extension pattern (Phase 3 adds benchmark prices)
- Deduplication by (benchmark_ticker, date) - high cache hit rate since benchmarks reused across trades

**Avoids pitfalls:**
- Pitfall 2 (Benchmark Survivorship Bias) - backfill full history on first sync, incremental updates after
- Benchmark Price Gaps - reuse get_price_on_date_with_fallback() for weekend/holiday handling
- Null Prices Breaking Calculations - circuit breaker prevents partial enrichment, NULL benchmarks logged as warnings

**Research needed:** No additional research (Yahoo Finance ETF prices use same API as stock prices)

### Phase 3: Core Performance Scoring
**Rationale:** Table stakes features (absolute return, S&P 500 benchmark, win rate, leaderboard) provide immediate user value and validate enrichment pipeline before adding complexity. Pure calculation functions with comprehensive unit tests reduce integration risk.

**Delivers:**
- scoring.rs module with calculate_analytics() pure function
- ScoreComponents struct (alpha_score, abnormal_return, trade_return_pct, benchmark_return_pct, holding_period_days)
- enrich-analytics CLI command (calculates and stores scores)
- DB methods: get_unenriched_analytics_trades(), update_trade_analytics()
- Win rate calculation (percentage of trades with realized_pnl > 0)
- Politician leaderboard with percentile ranks

**Addresses features:**
- Absolute Return (table stakes)
- S&P 500 Benchmark Comparison (table stakes)
- Win Rate / Batting Average (table stakes)
- Politician Leaderboard (table stakes)
- Percentile Ranking (table stakes)

**Avoids pitfalls:**
- Pitfall 1 (Time Normalization) - require date range filter for leaderboards, document annualized return methodology
- Option Trade Inclusion - filter WHERE asset_type != 'stock-option' in all analytics queries
- Percentile Division by Zero - guard percentile_rank() with if values.len() <= 1 { return 50.0; }
- Null Prices - skip trades with NULL prices, log warning count

**Research needed:** No additional research (alpha/abnormal return calculations are standard finance formulas)

### Phase 4: Conflict Detection Features
**Rationale:** Committee-sector overlap and donation-trade correlation are high-value differentiators that leverage v1.2 FEC data. Dependencies are satisfied (committee data exists, employer-issuer mapping exists from v1.2), implementation is primarily mapping logic not complex computation.

**Delivers:**
- Committee-sector overlap scoring (committee jurisdiction -> GICS sectors -> trade sector)
- Donation-trade correlation flags (employer-to-issuer mapping -> contribution aggregation -> conflict score)
- Per-trade conflict flags (committee_overlap: yes/no, donation_conflict: yes/no)
- Per-politician conflict score (aggregate across all trades)

**Uses v1.2 data:**
- fec_committees table (committee assignments)
- donations table (Schedule A contributions with employer field)
- employer_issuer_links table (fuzzy matching results from v1.2 Phase 12)

**Implements architecture:**
- Static YAML committee mapping (committee_sectors.yaml)
- Fuzzy committee name matching (normalize, Jaro-Winkler for variations)
- Employer aggregation by politician + issuer pair

**Avoids pitfalls:**
- Pitfall 7 (Correlation as Causation) - multi-variate analysis with state_industry confounder, display multiple hypotheses not causal claims, avoid causal language in output
- Committee Name Variations - normalize committee names (lowercase, remove "Committee on"), fuzzy matching threshold 0.85+
- Committee Assignment Churn - flag as "current committee only (may not reflect assignment at trade time)" when historical data unavailable

**Research needed:** No additional research (committee jurisdiction mapping documented in Congressional rules, employer-issuer fuzzy matching already implemented in v1.2)

### Phase 5: Anomaly Detection & Composite Scoring
**Rationale:** Advanced analytics features layer on top of enriched scores and conflict flags. Anomaly detection has high false positive risk, requiring statistical rigor (minimum sample sizes, sector-relative thresholds, p<0.01 significance). Composite scoring depends on all sub-scores being calculated.

**Delivers:**
- Pre-move trade detection (bought before +X% price move, sector-relative)
- Unusual volume detection (vs rolling 90-day baseline per politician)
- Sector concentration risk (HHI calculation with >2500 high concentration threshold)
- Rolling returns (30-day, 90-day smoothing windows)
- Composite score (weighted combination: 40% returns, 20% win rate, 20% anomaly flags, 20% conflict flags)
- analytics CLI command with all filters (min-alpha, sector, top-N, conflict flags)
- All output formats (table/CSV/markdown/XML/JSON) for analytics queries

**Addresses features:**
- Anomaly Detection: Pre-Move Trades (differentiator)
- Unusual Volume Detection (differentiator)
- Sector Concentration Risk (differentiator)
- Rolling Returns (differentiator)
- Composite Score (differentiator)

**Avoids pitfalls:**
- Pitfall 5 (False Positive Clustering) - require minimum N=30 trades before flagging clusters, p<0.01 threshold with Bonferroni correction, ensemble voting (3+/5 detectors)
- Anomaly False Positives from Volatility - use sector-relative thresholds (stock_return - sector_return > 5% not absolute 10% move), document methodology prominently
- Composite Weight Misconfiguration - validate weights sum to 1.0, default weights always defined (40/20/20/20), normalize if user provides partial weights
- Floating Point HHI Precision - round HHI to 2 decimal places before threshold comparison, use integer comparison (hhi * 100.0) as i64 > 250000

**Research needed:** Potentially deep-dive on statistical significance testing for small samples (Fisher's Exact Test, permutation tests) if false positive rate exceeds 20% in validation.

### Phase Ordering Rationale

**Why this order:**
1. Sector classification and schema are zero-dependency foundations that unblock all subsequent phases
2. Benchmark enrichment must precede scoring (scores require benchmark prices)
3. Core scoring before conflicts/anomalies validates enrichment pipeline with simple features before complexity
4. Conflict detection after core scoring leverages v1.2 donation data once basic analytics proven
5. Anomaly detection last because it has highest false positive risk, needs all other scores for composite calculation

**Dependency chain enforces ordering:**
- Phase 2 depends on Phase 1 (schema v6 must exist for benchmark_price column)
- Phase 3 depends on Phase 2 (scoring requires benchmark prices)
- Phase 4 depends on Phase 3 (conflict scores integrate with performance scores)
- Phase 5 depends on Phases 3+4 (composite score combines all sub-scores)

**Risk mitigation:**
- Pure calculation modules (sector.rs, scoring.rs) in Phases 1+3 are unit testable in isolation before integration
- Incremental feature delivery allows validation at each phase (benchmark enrichment -> scoring -> conflicts -> anomalies)
- Table stakes features (Phases 1-3) provide user value even if differentiators (Phases 4-5) are deferred
- Static YAML approach (Phase 1) avoids external API dependencies that could block later phases

### Research Flags

**Phases with standard patterns (no additional research needed):**
- **Phase 1 (Sector Classification):** GICS sectors well-documented, YAML mapping straightforward, parallels existing committee_resolver.rs pattern
- **Phase 2 (Benchmark Enrichment):** Yahoo Finance ETF prices identical to stock prices, extends existing enrich_prices.rs Phase 1+2 pattern
- **Phase 3 (Core Scoring):** Alpha and abnormal return are standard finance calculations, pure function pattern mirrors portfolio.rs FIFO calculator

**Phases likely needing validation during implementation:**
- **Phase 4 (Conflict Detection):** Committee jurisdiction mapping may need manual verification against Congressional rules for edge cases (joint committees, select committees). Employer-to-issuer fuzzy matching false positive rate needs validation with real donation data.
- **Phase 5 (Anomaly Detection):** Statistical significance testing with small samples (N=10-50) requires careful threshold calibration. Fisher's Exact Test and permutation tests may need deeper research if false positive rate exceeds 20% in initial validation. Composite score weighting (40/20/20/20) is initial hypothesis, may need A/B testing or user feedback to refine.

**Research gaps from initial research:**
- Optimal false positive threshold for anomaly detection (research suggests 1-5%, need to validate with congressional trade data characteristics)
- Composite score weighting methodology (40/20/20/20 is standard finance practice but not validated for congressional trade domain)
- Committee assignment historical tracking (current research assumes point-in-time committee data, historical tracking would eliminate committee churn pitfall but requires data source investigation)

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | No new dependencies for v1.3, existing yahoo_finance_api sufficient for benchmark ETFs, static YAML approach proven in v1.2 FEC mappings |
| Features | HIGH | Table stakes validated against 4 congressional trade platforms (InsiderFinance, Quiver, Unusual Whales, Capitol Trades), differentiators align with competitive gaps |
| Architecture | HIGH | Extends existing patterns (enrich_prices pipeline, portfolio FIFO, sector classification parallels committee resolver), no architectural unknowns |
| Pitfalls | HIGH | 7 critical pitfalls sourced from finance literature (survivorship bias, look-ahead bias), domain-specific issues documented in existing codebase edge cases |

**Overall confidence:** HIGH

Research for v1.3 analytics built on solid foundation of v1.1 (price enrichment) and v1.2 (FEC integration) patterns. No external API dependencies beyond existing Yahoo Finance integration reduces risk. Static YAML approach for sector classification avoids rate limiting concerns. Pure calculation modules (sector.rs, scoring.rs) are unit testable without DB/network dependencies, enabling test-driven development.

Lower confidence areas are implementation details (optimal anomaly thresholds, composite score weights) not architectural choices. These can be refined during Phase 4+5 implementation based on validation with real trade data.

### Gaps to Address

**Gap 1: Small sample statistical testing**
- **Issue:** Congressional trades have 10-50 trades/politician/year (small sample). Standard anomaly detection produces 60-80% false positive rates. Research identified Fisher's Exact Test and permutation tests as solutions but implementation details need validation.
- **Handle:** Phase 5 implementation should start with conservative thresholds (p<0.01, minimum N=30) and measure false positive rate on validation set (2023-2024 trades). If FPR >20%, research-phase for statistical testing alternatives (bootstrap confidence intervals, Bayesian approaches).

**Gap 2: Committee assignment historical tracking**
- **Issue:** Current research assumes point-in-time committee data (politician's current assignment). Politicians switch committees mid-term, creating false positives ("Rep traded energy stocks before joining Energy Committee"). Historical committee tracking would eliminate this but requires data source investigation.
- **Handle:** Phase 4 implementation flags conflicts with disclaimer "current committee only (may not reflect assignment at trade time)". If user feedback indicates high false positive rate, research-phase to investigate ProPublica Congress API or GovTrack for historical committee assignments.

**Gap 3: Sector classification coverage validation**
- **Issue:** Static YAML covers top 200 tickers but unknown coverage percentage of congressional trades. If <80% coverage, frequent "Unknown sector" classifications will limit sector ETF benchmarking value.
- **Handle:** Phase 1 implementation counts tickers classified vs Unknown after sync. If Unknown >20%, expand YAML mapping or add fallback to Yahoo Finance sector API (deferred from initial research as optional enhancement).

**Gap 4: Benchmark price historical depth**
- **Issue:** Research identified survivorship bias pitfall but didn't validate how far back Yahoo Finance maintains ETF price history. If SPY/XLK only available 5 years back but trades go back 10 years, partial coverage creates bias.
- **Handle:** Phase 2 implementation validates MIN(price_date) for each benchmark ticker against MIN(tx_date) in trades table. Log warning if coverage gap >6 months. If significant gap, document limitation in CLI help or research SEC filing data for historical ETF NAV.

## Sources

Research drew from multiple domains with varying confidence levels.

### Primary (HIGH confidence)

**v1.3 Analytics Research:**
- [Congress Stock Trades Tracker | InsiderFinance](https://www.insiderfinance.io/congress-trades) - Performance metrics, leaderboards (validated table stakes)
- [Congress Trading - Quiver Quantitative](https://www.quiverquant.com/congresstrading/) - Strategy backtesting, cumulative returns (alpha methodology)
- [US Politics: Track Congressional & Senate Stock Trades | Unusual Whales](https://unusualwhales.com/politics) - Anomaly detection, STOCK Act violations (conflict detection patterns)
- [Select Sector SPDR ETFs | State Street](https://www.ssga.com/us/en/intermediary/capabilities/equities/sector-investing/select-sector-etfs) - 11 GICS sector ETF structure, methodologies
- [Global Industry Classification Standard - MSCI](https://www.msci.com/indexes/index-resources/gics) - Official GICS methodology (sector classification)
- Existing codebase patterns (yahoo.rs, portfolio.rs, enrich_prices.rs) - Architectural patterns validated in v1.1/v1.2

### Secondary (MEDIUM confidence)

**Performance Metrics:**
- [Risk Metrics Explained: Sharpe Ratio, Alpha, and Beta | Financial Regulation Courses](https://www.financialregulationcourses.com/risk-metrics-explained-sharpe-ratio-alpha-beta) - Alpha/abnormal return methodology
- [Batting Average and Win-Loss Ratio | Novus](https://www.novus.com/articles/batting-average-and-win-loss-ratio) - Win rate calculation formulas
- [Percentile Rank in Category | Morningstar](https://awgmain.morningstar.com/webhelp/glossary_definitions/mutual_fund/Percentile_Rank_in_Category.htm) - Ranking methodology
- [How To Analyze Portfolio For Concentration Risk | Financial Samurai](https://www.financialsamurai.com/how-to-analyze-investment-portfolio-for-concentration-risk-sector-exposure-style/) - HHI calculation for sector concentration

**Pitfalls Research:**
- [Survivorship Bias in Trading: Why Most 'Proven' Strategies Are Misleading](https://enlightenedstocktrading.com/survivorship-bias-in-trading/) - 14% return inflation from survivorship bias
- [Understanding Look-Ahead Bias in Trading Strategies - MarketCalls](https://www.marketcalls.in/machine-learning/understanding-look-ahead-bias-and-how-to-avoid-it-in-trading-strategies.html) - Look-ahead bias detection (Sharpe ratio >1.5 indicator)
- [Anomaly Detection: How to Tell Good Performance from Bad - Towards Data Science](https://towardsdatascience.com/anomaly-detection-how-to-tell-good-performance-from-bad-b57116d71a10/) - False positive reduction (precision 0.20 threshold)
- [Correlation vs. Causation: Why It Matters for Investors - Practical AI Investor](https://practicalainvestor.substack.com/p/correlation-vs-causation-why-it-matters) - Confounding variable analysis

### Tertiary (LOW confidence, needs validation)

**Composite Scoring Weighting:**
- [Weighted Scoring Model: Step-by-Step Guide | Product School](https://productschool.com/blog/product-fundamentals/weighted-scoring-model) - General weighting methodology (40/20/20/20 split is inferred, not domain-validated)
- [Discovering optimal weights in stock-picking models | Springer](https://jfin-swufe.springeropen.com/articles/10.1186/s40854-020-00209-x) - Mixture design approach (complex, deferred to v2.x)

**Small Sample Statistical Testing:**
- Fisher's Exact Test and permutation tests mentioned in pitfalls research but implementation details not fully researched (will need deeper dive in Phase 5 if false positive rate exceeds 20%)

---
*Research completed: 2026-02-14*
*Ready for roadmap: yes*
