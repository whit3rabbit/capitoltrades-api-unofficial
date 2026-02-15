# Feature Landscape: Congressional Trade Analytics & Scoring

**Domain:** Congressional trade performance analysis and anomaly detection
**Researched:** 2026-02-14

## Table Stakes

Features users expect from congressional trade analytics. Missing these makes the tool feel incomplete.

| Feature | Why Expected | Complexity | Dependencies |
|---------|--------------|------------|--------------|
| **Absolute Return** | Industry standard: "did they beat the market?" | Low | Existing price enrichment data |
| **Benchmark Comparison (S&P 500)** | All platforms show S&P 500 outperformance/underperformance | Medium | Need to fetch/store SPY or ^GSPC daily closes |
| **Politician Leaderboard** | Ranking politicians by return is core feature for all trackers (InsiderFinance, Unusual Whales, Quiver) | Medium | Requires aggregating all trades per politician into portfolio return |
| **Win Rate (Batting Average)** | Percentage of trades that were profitable vs losses. Standard portfolio metric. | Low | Depends on realized P&L from FIFO (already exists) |
| **Time Period Filters** | YTD, 1-year, 3-year, all-time returns expected. Users compare "who did best in 2025?" | Medium | Need to aggregate returns by date range |
| **Percentile Ranking** | "Top 10%", "Bottom 25%" more intuitive than raw scores. Morningstar standard for fund comparison. | Low | Sort and calculate rank/total |
| **Trade Count & Volume** | How many trades, total dollar volume traded. Basic transparency metric. | Low | Simple aggregation of existing data |

## Differentiators

Features that set the product apart. Not expected, but valued.

| Feature | Value Proposition | Complexity | Dependencies |
|---------|-------------------|------------|--------------|
| **Sector ETF Benchmarking** | "Rep X bought tech stocks, did they beat XLK?" More precise than S&P 500 for sector-heavy traders. | High | Need to map tickers to sectors, fetch sector ETF data (XLF, XLK, XLE, etc.) |
| **Committee-Sector Overlap Detection** | "Rep on Energy Committee trades oil stocks." Clear conflict-of-interest signal unavailable elsewhere. | Medium | Committee data (exists), sector mapping (new), correlation scoring |
| **Anomaly Detection: Pre-Move Trades** | "Bought 3 days before +20% move." Insider-timing signal more valuable than raw returns. | High | Requires price volatility analysis, event detection, statistical thresholding |
| **Donation-Trade Correlation Flags** | "Top donor is defense contractor, Rep trades defense stocks." Links campaign finance to trading. | Medium | Employer-to-issuer mapping (exists), contribution aggregation |
| **Composite Score** | Single number combining returns, timing, conflicts, concentration. Makes comparison trivial. | High | Weighting multiple sub-scores, normalization, calibration |
| **Sector Concentration Risk** | Herfindahl-Hirschman Index showing over-exposure to single sector. Portfolio risk metric. | Medium | Sector mapping, HHI calculation |
| **Rolling Returns** | 30-day, 90-day rolling windows smooth out volatile periods. More robust than point-to-point. | Medium | Need date-windowed aggregation, re-calculation on each query |
| **Unusual Volume Detection** | "Traded 10x normal volume in past week." Behavioral change signal. | High | Baseline calculation, standard deviation thresholds, time-series analysis |

## Anti-Features

Features to explicitly NOT build (for v1.3).

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Real-Time Alerts** | CLI is batch-oriented, not event-driven. No infrastructure for push notifications. | Defer to v2.x if web API layer added |
| **Option Strategy Analysis** | Strikes, expiries, Greeks require complex pricing models. Out of scope per PROJECT.md. | Note option trades in output but exclude from analytics |
| **Interactive Charting** | CLI output is text/CSV/JSON. Charts belong in web UI or external visualization tools. | Provide data for external charting (CSV/JSON export) |
| **Market-Wide Anomaly Detection** | "Whole market moved +5%." Scope is politician behavior, not market structure. | Focus on individual politician patterns |
| **Backtesting Trades** | "What if I copied their trades?" Veers into financial advice territory. | Show returns but don't suggest users copy trades |
| **Social Sentiment Integration** | Twitter/Reddit sentiment adds noise and API complexity. Not core to congressional data. | Stick to disclosed trades and donations |
| **Machine Learning Predictions** | "Will Rep X buy next week?" High complexity, low reliability, liability concerns. | Descriptive analytics only (what happened, not what will happen) |

## Feature Dependencies

```
Absolute Return
  └── Price Enrichment (exists v1.1)

Benchmark Comparison
  └── S&P 500 Price Storage (new)
      └── Yahoo Finance Client (exists v1.1)

Sector ETF Benchmarking
  └── Sector Mapping (new)
  └── Sector ETF Price Storage (new)
      └── Yahoo Finance Client (exists v1.1)

Win Rate (Batting Average)
  └── Realized P&L (exists v1.1 FIFO)

Committee-Sector Overlap
  └── Committee Data (exists v1.2)
  └── Sector Mapping (new)
  └── Overlap Scoring Logic (new)

Donation-Trade Correlation
  └── Donation Data (exists v1.2)
  └── Employer-Issuer Mapping (exists v1.2)
  └── Correlation Scoring Logic (new)

Anomaly Detection: Pre-Move Trades
  └── Price Enrichment (exists v1.1)
  └── Volatility Calculation (new)
  └── Event Detection Logic (new)

Composite Score
  └── All Sub-Scores (returns, batting avg, anomaly flags, overlap flags)
  └── Weighting/Normalization Logic (new)

Sector Concentration (HHI)
  └── Sector Mapping (new)
  └── Position Data (exists v1.1 portfolio)

Rolling Returns
  └── Date-Windowed Aggregation (new)
  └── Price Enrichment (exists v1.1)

Unusual Volume Detection
  └── Historical Trade Volume Baseline (new)
  └── Statistical Thresholds (new)
```

## MVP Recommendation

Prioritize table stakes for first deliverable, add 2-3 differentiators in subsequent phases:

### Phase 1: Core Metrics (Table Stakes)
1. Absolute return calculation (per trade, per politician)
2. S&P 500 benchmark comparison
3. Win rate (batting average)
4. Politician leaderboard with percentile ranks
5. Trade count and volume aggregation
6. Time period filtering (YTD, 1Y, 3Y, all-time)

### Phase 2: Benchmarking Infrastructure
1. S&P 500 daily price storage
2. Sector mapping (ticker to GICS sector)
3. Sector ETF price storage (11 Select Sector SPDRs)
4. Sector ETF benchmark returns

### Phase 3: Conflict Detection (Differentiator)
1. Committee-sector overlap scoring
2. Donation-trade correlation flags
3. Per-trade conflict flags
4. Per-politician conflict score

### Phase 4: Anomaly Detection (Differentiator)
1. Pre-move trade detection (bought before +X% move)
2. Unusual volume detection (vs rolling baseline)
3. Sector concentration risk (HHI)
4. Rolling returns (30d, 90d smoothing)

### Phase 5: Composite Scoring (Differentiator)
1. Normalize all sub-scores to 0-100 scale
2. Weighted composite formula
3. Integrate into leaderboard
4. CLI flag for score components display

**Defer to Later:**
- Machine learning-based predictions
- Interactive charts (export data for external tools)
- Real-time alerts (batch CLI scope)
- Option strategy analysis (out of scope)

## Domain-Specific Notes

### 30-Day Disclosure Delay Impact

Per PROJECT.md, there's a 30-45 day disclosure delay (STOCK Act filing requirement). This means:

- **Timing analysis is retrospective only.** Can't detect "bought 3 days before earnings" in real-time.
- **Pre-move detection** still valuable: "Bought NVDA on April 15, filed May 10, NVDA +20% by filing date."
- **Unusual volume detection** looks at clustering of filed trades (multiple disclosures same week), not real-time behavior.

### CLI Output Constraints

CLI is text-based (table/CSV/JSON/markdown/XML). Analytics must be:
- **Tabular** (leaderboards, top-N rankings)
- **Summary stats** (aggregate numbers, percentiles)
- **Flags/indicators** (conflict: yes/no, anomaly: detected/none)

Complex visualizations (charts, graphs) belong in external tools consuming CSV/JSON exports.

### Existing Data Richness

The codebase already has:
- **Price enrichment** (trade date price, current price, estimated shares, estimated value)
- **FIFO P&L** (realized gains/losses, unrealized P&L on open positions)
- **Donation data** (Schedule A contributions, employer-to-issuer mapping)
- **Committee assignments** (from OpenFEC, mapped to politicians)
- **Sector data** (on issuers table from scraping)

This means many analytics are data transformations rather than new data ingestion.

### Performance Metrics Clarity

**Alpha** (excess return vs benchmark) requires risk-free rate assumption. For CLI simplicity, use:
- **Relative Return** = politician return - benchmark return (no risk-free rate adjustment)
- **Outperformance %** = (politician return / benchmark return - 1) * 100

Avoid terms like "Alpha" unless implementing full CAPM calculations (complexity overkill for v1.3).

**Beta** (volatility relative to market) requires regression analysis. For CLI:
- Use **sector concentration** as risk proxy (higher concentration = higher implied risk)
- Defer true Beta calculation to later (requires daily price correlation, window parameters, statistical lib)

**Sharpe Ratio** (risk-adjusted return) requires return standard deviation. For CLI:
- Use **win rate** + **avg gain vs avg loss** as simpler risk-adjustment proxy
- Defer true Sharpe to later (requires daily return variance calculations)

### Sector Mapping Considerations

From research, Select Sector SPDR ETFs cover 11 sectors:
- XLB (Materials), XLC (Communication Services), XLE (Energy)
- XLF (Financials), XLI (Industrials), XLK (Technology)
- XLP (Consumer Staples), XLRE (Real Estate), XLU (Utilities)
- XLV (Health Care), XLY (Consumer Discretionary)

These map to GICS (Global Industry Classification Standard) sectors. Yahoo Finance provides sector via company profile API. For v1.3:
- **Option 1:** Scrape sector from existing issuer detail pages (already have market_cap, sector likely available)
- **Option 2:** Use Static mapping file (ticker -> GICS code -> sector ETF)
- **Option 3:** Yahoo Finance API lookup (adds API dependency)

Recommend Option 1 (check if sector already scraped) or Option 2 (static file for known tickers, graceful fallback).

### Anomaly Detection Thresholds

From research on anomaly detection:
- **Pre-move threshold:** +/- 10% price move within 30 days of trade (standard deviation-based too complex for CLI v1)
- **Unusual volume:** 2x or 3x rolling 90-day average trade count per politician
- **Sector concentration:** HHI > 2500 (per DOJ merger guidelines) indicates high concentration

These are heuristic thresholds. For v1.3, use fixed thresholds with CLI flags for customization (--pre-move-threshold, --volume-multiplier).

### Committee-Sector Mapping

Committees have oversight over sectors:
- Energy & Commerce Committee -> Energy (XLE), Utilities (XLU)
- Financial Services Committee -> Financials (XLF)
- Armed Services Committee -> Industrials (XLI, Aerospace/Defense subset)
- Agriculture Committee -> Materials (XLB, Agri-chemicals)

Need static mapping file (committee name -> relevant sector ETFs). For v1.3, start with high-signal mappings (Energy/Financial Services/Armed Services) and expand.

### Composite Score Weighting

From research, percentile-based weighting is standard. For v1.3 composite score:
1. Calculate sub-scores (return %, win rate, anomaly count, conflict count)
2. Convert to percentiles (rank among all politicians)
3. Weight: 40% returns, 20% win rate, 20% anomaly flags, 20% conflict flags
4. Sum to 0-100 final score

Weights should be CLI-configurable or documented for transparency (avoid "black box" scores).

## Sources

Based on web research (MEDIUM-HIGH confidence, cross-referenced across multiple platforms):

**Congressional Trade Tracking Platforms:**
- [Congress Stock Trades Tracker | InsiderFinance](https://www.insiderfinance.io/congress-trades) - Performance metrics, leaderboards
- [Congress Trading - Quiver Quantitative](https://www.quiverquant.com/congresstrading/) - Strategy backtesting, cumulative returns
- [US Politics: Track Congressional & Senate Stock Trades | Unusual Whales](https://unusualwhales.com/politics) - Anomaly detection, STOCK Act violations
- [Capitol Trades: When Politicians Got It Right in 2025](https://www.capitoltrades.com/articles/when-politicians-got-it-right-in-2025-2026-01-02) - Timing analysis, success stories
- [US politician trade tracker | Trendlyne](https://us.trendlyne.com/us/politicians/recent-trades/) - Latest disclosures, filtering

**Performance Metrics:**
- [Risk Metrics Explained: Sharpe Ratio, Alpha, and Beta | Financial Regulation Courses](https://www.financialregulationcourses.com/risk-metrics-explained-sharpe-ratio-alpha-beta) - Standard performance metrics
- [Mastering Performance Metrics: Alpha vs Beta | Allio](https://www.alliocapital.com/macroscope/alpha-beta-risk-adjusted-returns) - Risk-adjusted returns
- [Batting Average and Win-Loss Ratio | Novus](https://www.novus.com/articles/batting-average-and-win-loss-ratio) - Portfolio win rate metrics
- [Percentile Rank in Category | Morningstar](https://awgmain.morningstar.com/webhelp/glossary_definitions/mutual_fund/Percentile_Rank_in_Category.htm) - Ranking methodology

**Sector & Concentration Analysis:**
- [Select Sector SPDR ETFs | State Street](https://www.ssga.com/us/en/intermediary/capabilities/equities/sector-investing/select-sector-etfs) - 11 sector ETFs, structure
- [My S&P 500 Prediction On Sector Outperformers | Seeking Alpha](https://seekingalpha.com/article/4854947-my-s-and-p-500-prediction-on-sector-out-performers-and-laggards-in-2026) - 2026 sector performance
- [Analyzing concentration risk in credit portfolios | Moody's](https://www.moodys.com/web/en/us/insights/portfolio-management/analyzing-concentration-risk-in-credit-portfolios.html) - HHI, Gini coefficient
- [How To Analyze Portfolio For Concentration Risk | Financial Samurai](https://www.financialsamurai.com/how-to-analyze-investment-portfolio-for-concentration-risk-sector-exposure-style/) - Sector exposure metrics

**Anomaly Detection:**
- [Anomaly Detection in Finance | Intrinio](https://intrinio.com/blog/anomaly-detection-in-finance-identifying-market-irregularities-with-real-time-data) - Unusual volume, price movements
- [Stock Market Volume Anomaly Detection | SliceMatrix](https://slicematrix.github.io/stock_market_anomalies.html) - Statistical techniques

**Composite Scoring:**
- [Weighted Scoring Model: Step-by-Step Guide | Product School](https://productschool.com/blog/product-fundamentals/weighted-scoring-model) - Weighting methodology
- [Discovering optimal weights in stock-picking models | Springer](https://jfin-swufe.springeropen.com/articles/10.1186/s40854-020-00209-x) - Mixture design approach

**Rolling Returns:**
- [How to Calculate Rolling Returns | SmartAsset](https://smartasset.com/investing/rolling-returns) - Period-over-period calculations
- [Rolling Returns Calculator | MF Online](https://www.mfonline.co.in/mutual-funds-research/rolling-returns) - Trailing returns methodology
