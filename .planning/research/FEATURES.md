# Feature Landscape

**Domain:** Stock Price Enrichment and Portfolio Tracking for Congressional Trade Analysis
**Researched:** 2026-02-09
**Confidence:** MEDIUM

## Table Stakes

Features users expect. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Historical price at trade date | Without this, enrichment is meaningless - need to know what price they actually traded at | Medium | Requires Yahoo Finance historical API lookup per ticker per trade date. Handle weekends/holidays with nearest trading day. |
| Current price per ticker | Users expect to see "what is it worth now?" - table stakes for any portfolio tool | Low | Single lookup per ticker, can be cached briefly (e.g., 5 min TTL for batch CLI). |
| Net position per politician per ticker | Portfolio = current holdings after all buys/sells - users can't calculate P&L without this | High | Requires FIFO/LIFO accounting across all trades. Must handle partial fills, exchanges, receives (non-purchase acquisitions). |
| Realized P&L (closed positions) | Show profit/loss on positions that were fully sold - core financial metric | Medium | Track cost basis, match buys to sells using FIFO, calculate (sell price - buy price) * shares. |
| Unrealized P&L (open positions) | Show profit/loss on current holdings using current price - expected for any portfolio | Medium | (current price - average cost basis) * remaining shares. Requires current price lookup. |
| Trade date validation | Congressional trades have 45-day disclosure lag - need to validate trade_date exists and is historical | Low | Check trade_date is not null, not in future, not before politician's term start. |
| Ticker symbol validation | Invalid/delisted tickers will break enrichment - must validate before API calls | Medium | Check ticker exists in Yahoo Finance, handle delisted stocks gracefully (mark as unenrichable, don't fail batch). |
| Batch processing with resumability | CLI enrichment may process thousands of trades - must be resumable after failures | Medium | Track enrichment status per trade (unenriched/enriched/failed), skip already-enriched on re-run. Pattern already exists from Phase 5 trade detail enrichment. |
| Graceful failure handling | Some tickers will fail (delisted, data gaps, API errors) - don't fail entire batch | Low | Circuit breaker pattern from Phase 5, mark individual trades as failed, continue processing. Log failures for review. |
| Option classification | Options (calls/puts) are ~20-30% of congressional trades - must distinguish from stock | Low | Already have asset_type field from Phase 5. Classify stock-option separately, defer valuation (complex). |

## Differentiators

Features that set product apart. Not expected, but valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Trade value estimation using historical price | Congressional disclosures use dollar ranges ($1K-15K, etc.) not exact amounts - historical price lets you estimate actual share count | High | Reverse-calculate shares = midpoint of range / historical price. Increases accuracy of position tracking. Requires range parsing. |
| Cost basis per position (average weighted) | Show average purchase price for remaining holdings - helps understand if position is profitable | Medium | Track cumulative cost across all buys, divide by remaining shares. More sophisticated than just FIFO cost basis. |
| Aggregate portfolio P&L per politician | "How much has Nancy Pelosi made overall?" - compelling analysis feature | Medium | Sum realized + unrealized P&L across all positions. Requires all above calculations to be complete. |
| Sector-level portfolio analysis | "What sectors is this politician betting on?" - strategic insight beyond ticker-level | Medium | Leverage Yahoo Finance sector metadata (already available in their API). Group positions by sector, show exposure. |
| Time-weighted return calculation | Show portfolio performance over time, not just absolute P&L - fairer comparison | High | Complex calculation requiring position values at multiple time points. Likely defer to v2. |
| Wash sale detection | Identify potential wash sales (sell + rebuy within 30 days) - tax/compliance insight | Medium | Flag trades where same ticker sold then bought within 30 days. Informational only (we're not doing their taxes). |
| Data staleness indicators | Show when prices were last updated - transparency about data freshness | Low | Store enriched_at timestamp per ticker price, display age in output. Already have pattern from Phase 5. |
| Performance vs S&P 500 benchmark | "Did they beat the market?" - compelling narrative feature | Medium | Fetch S&P 500 historical returns, compare politician portfolio return. Requires time-series data. Defer to v2. |
| Historical portfolio snapshots | Track portfolio composition changes over time - "they sold all their tech stocks in March 2024" | High | Requires storing position snapshots at intervals or reconstructing from trade history. Significant complexity, defer. |

## Anti-Features

Features to explicitly NOT build.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Real-time price updates | This is a batch CLI tool for analysis, not a trading platform. Real-time adds complexity (websockets, polling) with no user value. | Refresh prices on-demand when user runs enrichment command. Cache current prices for 5-15 min to avoid redundant API calls within same session. |
| Full options valuation (Greeks, Black-Scholes) | Options pricing is extremely complex (strike, expiry, IV, Greeks). Congressional disclosures often lack strike/expiry details. Adds massive complexity for limited value. | Classify options separately, calculate simple P&L if we have strike/expiry, otherwise mark as "option (valuation N/A)". Focus on stock positions. |
| Tax reporting features | We don't have enough data (exact purchase amounts, tax lots, account types). IRS reporting requires precision we can't guarantee. Liability risk. | Provide informational P&L only with clear disclaimer: "Not for tax reporting purposes". Flag potential wash sales for awareness, but don't calculate tax liability. |
| Multi-currency conversion | Congressional trades are overwhelmingly USD stocks. Foreign stocks are rare edge cases. Currency conversion adds API dependencies and complexity. | Assume USD. If foreign ticker detected, mark position as "foreign security (P&L not calculated)" and skip enrichment. Document limitation. |
| Intraday price tracking | Congressional trades disclose date only, not time. Intraday prices irrelevant and add API cost. | Use daily close price for historical lookups. Sufficient accuracy given 45-day disclosure lag and date-only precision. |
| Portfolio rebalancing recommendations | This crosses from analysis into investment advice. Out of scope for a transparency/research tool. | Provide data (positions, P&L, sectors) and let users draw their own conclusions. We're a data tool, not a robo-advisor. |
| Social features (following politicians, alerts) | Adds backend infrastructure (user accounts, notifications, storage). This is a local CLI tool, not a SaaS platform. | Output data to CSV/JSON/DB. Users can build their own alerting using standard tools (cron + grep, etc.). Keep tool stateless. |

## Feature Dependencies

```
Historical Price Lookup
    └──requires──> Ticker Symbol Validation
                       └──requires──> Trade Date Validation

Net Position Calculation (FIFO)
    └──requires──> Historical Price Lookup (for cost basis)
    └──requires──> Trade Type Classification (buy/sell/exchange)

Unrealized P&L
    └──requires──> Net Position Calculation
    └──requires──> Current Price Lookup

Realized P&L
    └──requires──> Net Position Calculation (closed positions)
    └──requires──> Historical Price Lookup (both buy and sell prices)

Aggregate Portfolio P&L
    └──requires──> Unrealized P&L
    └──requires──> Realized P&L

Trade Value Estimation ──enhances──> Net Position Calculation (better share count)
Cost Basis (Avg Weighted) ──enhances──> Unrealized P&L (alternative cost basis method)
Sector Analysis ──enhances──> Aggregate Portfolio P&L (grouping dimension)

Wash Sale Detection ──conflicts──> Clean P&L Calculation (creates exceptions/asterisks)
```

### Dependency Notes

- **Historical Price Lookup requires Ticker Validation:** Must verify ticker exists before making expensive API calls. Invalid tickers fail gracefully.
- **Historical Price Lookup requires Trade Date Validation:** Can't look up price for invalid/future dates. Weekends need nearest trading day logic.
- **Net Position requires Historical Price Lookup:** FIFO accounting needs purchase price per lot to calculate cost basis. Can't calculate positions without knowing what was paid.
- **P&L calculations require Net Position:** Can't calculate profit/loss without knowing current holdings and cost basis. Position calculation is foundation.
- **Trade Value Estimation enhances Position Calculation:** Reverse-engineering share count from dollar ranges + historical price improves accuracy. Congressional disclosures give ranges like "$15K-50K" - historical price lets us estimate shares = midpoint / price.
- **Wash Sale Detection conflicts with Clean P&L:** Marking wash sales adds complexity to P&L reporting (need asterisks, footnotes). Defer to v2 if at all.

## MVP Recommendation

Prioritize core enrichment and position tracking. Defer analysis features to validate foundation first.

### Launch With (v1)

Minimum viable enrichment - prove the data pipeline works:

1. **Historical price at trade date** - Core enrichment value
2. **Current price per ticker** - Required for unrealized P&L
3. **Ticker symbol validation** - Prevent batch failures
4. **Trade date validation** - Data quality gate
5. **Batch processing with resumability** - Handle scale (1000s of trades)
6. **Graceful failure handling** - Resilience for production use
7. **Option classification** - Distinguish options from stock (no valuation yet)
8. **Net position per politician per ticker (FIFO)** - Portfolio foundation
9. **Unrealized P&L per position** - "What are current holdings worth?"
10. **Data staleness indicators** - Transparency about price freshness

### Add After Validation (v1.x)

Once core enrichment is proven stable:

- **Realized P&L (closed positions)** - Requires robust position tracking, add when FIFO accounting is validated
- **Trade value estimation** - Enhance position accuracy using historical prices
- **Cost basis (average weighted)** - Alternative to FIFO, useful comparison
- **Aggregate portfolio P&L per politician** - Headline feature, requires all P&L components working

### Future Consideration (v2+)

Defer until v1 is in production use:

- **Sector-level portfolio analysis** - Requires sector metadata integration
- **Wash sale detection** - Edge case, adds complexity to P&L reporting
- **Performance vs S&P 500 benchmark** - Requires time-series infrastructure
- **Historical portfolio snapshots** - Major feature, requires storage architecture redesign

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Historical price at trade date | HIGH | MEDIUM | P1 |
| Current price per ticker | HIGH | LOW | P1 |
| Ticker symbol validation | HIGH | MEDIUM | P1 |
| Trade date validation | HIGH | LOW | P1 |
| Batch processing with resumability | HIGH | MEDIUM | P1 |
| Graceful failure handling | HIGH | LOW | P1 |
| Option classification | HIGH | LOW | P1 |
| Net position (FIFO) | HIGH | HIGH | P1 |
| Unrealized P&L | HIGH | MEDIUM | P1 |
| Data staleness indicators | MEDIUM | LOW | P1 |
| Realized P&L | HIGH | MEDIUM | P2 |
| Trade value estimation | MEDIUM | HIGH | P2 |
| Cost basis (avg weighted) | MEDIUM | MEDIUM | P2 |
| Aggregate portfolio P&L | HIGH | MEDIUM | P2 |
| Sector analysis | MEDIUM | MEDIUM | P3 |
| Wash sale detection | LOW | MEDIUM | P3 |
| Performance vs benchmark | MEDIUM | HIGH | P3 |
| Historical snapshots | HIGH | HIGH | P3 |

**Priority key:**
- P1: Must have for launch (v1.0)
- P2: Should have, add when possible (v1.x)
- P3: Nice to have, future consideration (v2+)

## Implementation Complexity Notes

### Historical Price Lookup (MEDIUM Complexity)
- Yahoo Finance unofficial APIs (yfinance library most popular in Rust ecosystem)
- Handle weekends/holidays - need "nearest trading day" logic
- Rate limiting - batch lookups to avoid API throttling
- Delisted stocks - some tickers won't have data, need fallback
- Date range validation - can't look up future dates
- **Estimate:** 2-3 days implementation + 1 day testing edge cases

### Net Position Calculation (HIGH Complexity)
- FIFO accounting across all trades per ticker per politician
- Handle multiple transaction types: buy, sell, exchange, receive
- Partial position tracking - some positions never fully closed
- Edge cases: stock splits, mergers (out of scope for v1, document limitation)
- Congressional trade quirks: dollar ranges not exact shares, need estimation
- Validation: positions should never go negative (unless short selling, which politicians rarely do)
- **Estimate:** 4-5 days implementation + 2 days testing with real trade data

### Trade Value Estimation (HIGH Complexity)
- Parse dollar range strings ("$15,001 - $50,000") into numeric ranges
- Calculate midpoint of range
- Divide by historical price to estimate shares
- Round to reasonable share counts (no fractional shares for stocks)
- Handle edge cases: very small trades (under $1K), very large trades (over $50M)
- Accuracy validation - compare estimated vs reported ranges
- **Estimate:** 3-4 days implementation + 1 day validation

### Realized P&L (MEDIUM Complexity)
- Match sell transactions to buy transactions using FIFO
- Calculate gain/loss per matched pair: (sell_price - buy_price) * shares
- Sum across all closed positions
- Handle partial sells - only realize P&L on portion sold
- Edge case: sells before buys in trade history (data quality issue)
- **Estimate:** 2-3 days implementation + 1 day testing

### Sector Analysis (MEDIUM Complexity)
- Yahoo Finance API provides sector metadata per ticker
- Map tickers to sectors during price enrichment
- Store sector in database (new field in tickers table)
- Group positions by sector for reporting
- Handle unknown sectors gracefully (ETFs, bonds, etc.)
- **Estimate:** 2 days implementation + 0.5 day testing

## Complexity Factors Specific to Congressional Trades

### Dollar Range Ambiguity
Congressional disclosures use ranges ($1K-15K, $15K-50K, $50K-100K, etc.) not exact amounts. This creates uncertainty:
- **Impact on position tracking:** Can't calculate exact share counts without price data
- **Mitigation:** Use historical price to estimate shares = range_midpoint / historical_price
- **Accuracy:** ±50% error possible (if trade was at range boundary), but better than no estimate
- **Validation:** Calculate value = estimated_shares * historical_price, check if within original range

### 45-Day Disclosure Lag
Trades disclosed 30-45 days after execution:
- **Impact on analysis:** Prices may have moved significantly by time of disclosure
- **Impact on enrichment:** Historical lookup always needed, can't use "current" price
- **User expectation:** Show both trade-date price and current price for context
- **Note:** This is why real-time features are anti-features - data is inherently stale

### Incomplete Transaction Details
Many trades lack key details:
- Missing transaction type (buy/sell sometimes unclear)
- Missing ticker symbols (only company name given)
- Options trades often lack strike/expiry
- **Mitigation:** Classify as "incomplete data" and skip P&L calculation, show in separate report
- **Data quality flag:** Track completeness metrics, surface in enrichment summary

### Asset Type Diversity
Congressional trades include:
- Common stock (bulk of trades, easy)
- Stock options (calls/puts, complex valuation)
- Municipal bonds (limited price data)
- Cryptocurrency (newer, volatile)
- Real estate/funds (not publicly traded)
- **Mitigation:** Focus v1 on stocks only (80%+ of volume), classify others separately

## Sources

### Portfolio Tracking Features
- [11 Best Portfolio Analysis Software For Investors in 2026 - MarketDash](https://www.marketdash.io/blog/best-portfolio-analysis-software)
- [15 Best Stock Portfolio Trackers in February 2026 - Benzinga](https://www.benzinga.com/money/best-portfolio-tracker)
- [11 Best Stock Portfolio Trackers for 2026 Reviewed - ValueWalk](https://www.valuewalk.com/investing/best-stock-portfolio-tracker/)
- [Stock Portfolio Management & Tracker - Yahoo Finance](https://finance.yahoo.com/portfolios/)

### Trade Enrichment Patterns
- [Trade data enrichment for Transaction Cost Analysis - LSEG Devportal](https://developers.lseg.com/en/article-catalog/article/trade-data-enrichment-for-transaction-cost-analysis)
- [Trade Enrichment | How is it achieved | Components - Fintelligents](https://fintelligents.com/trade-enrichment/)
- [The TRADE predictions series 2026: Key insights on data](https://www.thetradenews.com/the-trade-predictions-series-2026-key-insights-on-data/)

### Congressional Trading Tools
- [Congress Trading - Quiver Quantitative](https://www.quiverquant.com/congresstrading/)
- [What's Trading on Capitol Hill? - Capitol Trades](https://www.capitoltrades.com/)
- [US politician trade tracker - Trendlyne](https://us.trendlyne.com/us/politicians/recent-trades/)
- [Congress Stock Trades Tracker - InsiderFinance](https://www.insiderfinance.io/congress-trades)
- [Using a Congress Stock Tracker to Guide Your Trades - Intellectia](https://intellectia.ai/blog/best-congress-stock-tracker)

### P&L Calculation
- [How to Calculate Your Trading Profit & Loss (P&L) with Ease - SoftFX](https://www.soft-fx.com/blog/how-to-calculate-your-profit-and-loss-for-your-trading-positions/)
- [What is PnL and How to Calculate it? - QuadCode](https://quadcode.com/glossary/what-is-pl-and-how-to-calculate-it)
- [How to Calculate Stock Profit - Charles Schwab](https://www.schwab.com/learn/story/how-to-calculate-stock-profit)
- [Position and P&L - IBKR Guides](https://www.ibkrguides.com/traderworkstation/position-and-pnl.htm)
- [How to manually compute the P&L of your stocks, options and futures trades - TradMetria](https://trademetria.com/blog/how-to-manually-compute-the-pl-of-your-stocks-options-and-futures-trades/)

### Options Tracking
- [Basic Call and Put Options Strategies - Charles Schwab](https://www.schwab.com/learn/story/basic-call-and-put-options-strategies)
- [Options profit calculator](https://www.optionsprofitcalculator.com/)
- [Long Put Calculator - Options Profit Calculator](https://www.optionsprofitcalculator.com/calculator/long-put.html)

### Position Tracking (FIFO/LIFO)
- [LIFO vs FIFO: Which is Better for Day Traders? - Warrior Trading](https://www.warriortrading.com/lifo-vs-fifo-which-is-better-for-day-traders/)
- [FIFO Method Explained (2025): Guide for Traders - The Trading Analyst](https://thetradinganalyst.com/fifo-method/)
- [How to Sell Stock With FIFO or LIFO - Nasdaq](https://www.nasdaq.com/articles/how-sell-stock-fifo-or-lifo-2016-03-19)
- [How to Determine Which Shares to Sell, FIFO or LIFO - Zacks](https://finance.zacks.com/determine-shares-sell-fifo-lifo-9766.html)
- [Understanding FIFO / FILO on stocks for tax reporting - Claimyr](https://claimyr.com/government-services/irs/Understanding-FIFO-FILO-on-stocks-for-tax-reporting-which-method-is-better/2025-04-11)

### Congressional Trading Analysis Pitfalls
- [Estimating Congressional Trade Values: Range Analysis Guide - Nancy Pelosi Stock Tracker](https://nancypelosistocktracker.org/articles/estimating-trade-values)
- [Congress Trading Report 2024 - Unusual Whales](https://unusualwhales.com/congress-trading-report-2024)
- [Capitol Losses: The Mediocre Performance of Congressional Stock Portfolios](https://j-hai.github.io/assets/pdf/capitol.pdf)
- [Do senators and house members beat the stock market? - ScienceDirect](https://www.sciencedirect.com/science/article/abs/pii/S0047272722000044)

### Yahoo Finance API
- [Download historical data in Yahoo Finance - Yahoo Help](https://help.yahoo.com/kb/SLN2311.html)
- [Yahoo Finance API - A Complete Guide - AlgoTrading101](https://algotrading101.com/learn/yahoo-finance-api-guide/)
- [GitHub - ranaroussi/yfinance: Download market data from Yahoo! Finance's API](https://github.com/ranaroussi/yfinance)
- [How To Use The Yahoo Finance API - Market Data](https://www.marketdata.app/how-to-use-the-yahoo-finance-api/)

### Data Quality and Missing Data
- [A Better Way for Finance (and Others) to Handle Missing Data - Chicago Booth Review](https://www.chicagobooth.edu/review/better-way-finance-others-handle-missing-data)
- [How to identify and Handle Missing Trading Data to clean up - Medium](https://medium.com/@malarraju14/how-to-identify-and-handle-missing-trading-data-to-clean-up-7fcbca224157)
- [Handling Missing Data in Trading Datasets - BlueChip Algos](https://bluechipalgos.com/blog/handling-missing-data-in-trading-datasets/)

### Portfolio Data Validation
- [How to Streamline Your Investment Tracking with a Portfolio Tracker Using Data Validation in Google Sheets - FileDrop](https://getfiledrop.com/how-to-streamline-your-investment-tracking-with-a-portfolio-tracker-using-data-validation-in-google-sheets/)
- [Portfolio-performance - Help Documentation](https://help.portfolio-performance.info/en/reference/view/securities/all-securities/)

---
*Feature research for: Stock Price Enrichment and Portfolio Tracking for Congressional Trade Analysis*
*Researched: 2026-02-09*
*Confidence: MEDIUM (verified via multiple web sources, no official library documentation available for Context7 verification)*
