# Pitfalls Research

**Domain:** Yahoo Finance Market Data Integration
**Researched:** 2026-02-09
**Confidence:** MEDIUM

## Critical Pitfalls

### Pitfall 1: Unofficial API Fragility and Breaking Changes

**What goes wrong:**
Yahoo Finance's unofficial API endpoints change without notice, causing library breakage until maintainers patch the wrappers. This is an ongoing fragility issue - yfinance scrapes Yahoo Finance web endpoints and HTML pages, and because this is unofficial and fragile, any change on Yahoo's site can break yfinance. Unofficial endpoints can change with front-end updates, leading to library breakage.

**Why it happens:**
Yahoo shut down its official API in 2017, so all current access relies on reverse-engineered endpoints or web scraping. Yahoo makes no guarantees about endpoint stability since they're not officially supported.

**How to avoid:**
1. Don't build critical functionality that assumes Yahoo Finance will always work
2. Design with fallback mechanisms (cached data, manual entry, alternative data sources)
3. Implement circuit breakers to detect widespread failures
4. Monitor vendor changelogs actively - keep an eye on the repositories since endpoints and features can change
5. Add comprehensive error handling that degrades gracefully
6. Consider wrapping Yahoo Finance calls in an abstraction layer for easier provider swapping

**Warning signs:**
- Multiple consecutive 404 errors on previously working endpoints
- Changes in response schema structure
- New required headers or authentication challenges
- Library maintainers reporting issues on GitHub
- Sudden increases in error rates across multiple tickers

**Phase to address:**
Phase 1 (Foundation) - build abstraction layer and error handling from the start. Don't couple the entire application to Yahoo Finance specifics.

---

### Pitfall 2: Ticker Symbol Mismatch and Normalization

**What goes wrong:**
Ticker symbols from Capitol Trades may not match Yahoo Finance's expected format. Yahoo Finance uses exchange suffixes for international stocks (e.g., .TO for Toronto, .L for London) and ticker symbols can change due to mergers, acquisitions, rebranding, or exchange moves. Covered ticker symbols may not show up on Yahoo Finance because they are not static and can change.

**Why it happens:**
Capitol Trades uses the ticker symbol as reported by Congress, which may be outdated, exchange-specific, or use different conventions than Yahoo Finance. Around 30-40 ticker changes happen on average per month, and as companies merge or get acquired, tickers change, get delisted, and are sometimes re-used to list an entirely different company.

**How to avoid:**
1. Implement ticker validation before enrichment (check if ticker exists on Yahoo Finance)
2. Build a ticker normalization layer that handles common variations
3. Store original ticker + normalized ticker + Yahoo ticker separately in schema
4. Implement fuzzy matching for company names as fallback
5. Track enrichment failures by failure reason (ticker not found vs. API error)
6. Consider using third-party ticker symbol change history API (EODHD, Polygon.io, Financial Modeling Prep)
7. For unresolved tickers, mark as "needs_manual_review" rather than failing silently

**Warning signs:**
- High percentage of 404 errors on ticker lookups
- Enrichment working for major stocks but failing for smaller companies
- Discrepancies between expected company name and returned company name
- Options trades failing enrichment at higher rates than stock trades

**Phase to address:**
Phase 2 (Ticker Resolution) - must happen before bulk enrichment. Build ticker validation and normalization infrastructure early.

---

### Pitfall 3: Stock Splits and Dividend Adjustments

**What goes wrong:**
Historical prices must be adjusted for splits and dividends to be meaningful. Stock splits can create large historical price changes even though they do not change the value of the company, so you must adjust all pre-split prices in order to calculate historical returns correctly. Without proper adjustments, a $200 stock that had a 2-for-1 split will show a false 50% drop in price.

**Why it happens:**
Developers use raw "close" prices instead of "adjusted close" prices when enriching historical trades. While everyone adjusts for splits (because they have to), dividend adjustments are optional - split adjustments are mandatory for meaningful analysis, while dividend adjustments show total return rather than price return.

**How to avoid:**
1. ALWAYS use adjusted_close from Yahoo Finance, never raw close
2. Store both raw close and adjusted close in database for audit trail
3. Document clearly in schema which price field is used for calculations
4. When comparing trade price to historical price, account for splits between trade date and today
5. Test enrichment specifically with companies that had recent splits (e.g., NVDA 2024 10-for-1 split)
6. Be aware that different data providers may handle edge cases differently (special dividends, rounding)

**Warning signs:**
- Calculated returns showing impossible gains/losses (e.g., 1000% in one day)
- Price discrepancies when comparing to other data sources
- Historical prices not matching Yahoo Finance charts visually
- Test failures on split-adjusted stock comparisons

**Phase to address:**
Phase 1 (Foundation) - correct price field selection is critical from day one. Using wrong price field contaminates all downstream analysis.

---

### Pitfall 4: Options Contract Data Complexity

**What goes wrong:**
Congressional trades include stock options (calls/puts), not just stocks. Options data is far more complex than stock data - requires strike price, expiration date, option type (call/put), and uses OCC symbology (21-byte format: Root+YYMMDD+C/P+Strike). Options data from Yahoo Finance is practically useless for traders because the delay can be different for each option contract - some contracts are delayed by hours and others by 15-30 minutes.

**Why it happens:**
Developers treat options the same as stocks, but options have expiration dates, strike prices, and time decay. Capitol Trades reports options in various formats (sometimes just as the underlying ticker with notes in description).

**How to avoid:**
1. Design schema to handle both stocks and options with different fields
2. Parse option contract strings into components (underlying, expiration, strike, type)
3. For options, store: underlying_ticker, strike_price, expiration_date, option_type, contract_symbol
4. Accept that options pricing data may be stale or unavailable
5. For expired options, mark as enriched but with historical data limitations
6. Consider focusing MVP on stocks only, defer options to later phase
7. Use option chain data structure from Yahoo Finance: get_options_chain() returns dictionary with "calls" and "puts" keys

**Warning signs:**
- Options trades failing enrichment at 100% rate
- Option data showing prices that don't match market realities
- Confusion between stock and option when displaying to users
- Error messages about invalid ticker format for option contracts

**Phase to address:**
Phase 3 (Options Support) - explicitly defer to separate phase after stock enrichment is solid. Options are not MVP.

---

### Pitfall 5: Rate Limiting and IP Blocking

**What goes wrong:**
Yahoo Finance implements sophisticated rate limiting and bot detection. Yahoo sees many rapid requests from the same IP or pattern and starts rate-limiting or even temporarily banning those requests. If Yahoo starts enforcing rate limits per IP or per key, those calls quickly fail.

**Why it happens:**
Bulk enrichment of thousands of historical trades triggers anti-bot protections. Congressional trading data contains ~40,000+ trades, and enriching all of them in a tight loop will get blocked.

**How to avoid:**
1. Implement request pacing with delays between requests (2-5 seconds recommended)
2. Add random jitter to delays to mimic human browsing patterns
3. Use exponential backoff on 429 (Too Many Requests) errors
4. Respect Retry-After header if present
5. Limit concurrent requests (semaphore with max 3-5 concurrent)
6. Batch enrichment operations with progress tracking
7. Implement per-ticker caching to avoid re-fetching same ticker multiple times
8. Consider proxy rotation for large-scale operations (though adds complexity)
9. Monitor for 429 errors and automatically slow down when detected
10. Use circuit breaker pattern - after N consecutive failures, pause enrichment

**Warning signs:**
- 429 Too Many Requests errors
- 403 Forbidden errors (IP banned)
- Empty responses or truncated data
- Increasing error rates mid-batch
- Requests timing out more frequently

**Phase to address:**
Phase 1 (Foundation) - rate limiting must be built into the enrichment pipeline from the start. Capitol Traders already has semaphore-based concurrency control for trade detail scraping - extend this pattern.

---

### Pitfall 6: Trade Value Range Estimation Inaccuracy

**What goes wrong:**
The STOCK Act requires disclosure using predetermined dollar ranges ($1,000-$15,000, $15,001-$50,000, etc.) that provide general magnitude while obscuring precise values. Different tracking websites may show different estimated values for the same trade because they use different estimation methods. This creates inherent limitations when trying to calculate portfolio values or returns.

**Why it happens:**
Congressional disclosures are legally required to be ranges, not exact amounts. No precise transaction data exists. Developers try to enrich with exact historical prices but can't determine exact share quantities.

**How to avoid:**
1. Store the original range in database, don't convert to single value
2. If calculating estimated value, use range midpoint but document assumption
3. Add confidence bounds to any dollar amount displays
4. Never claim exact portfolio values - always show as estimates
5. Consider storing range_low, range_high, estimated_midpoint separately
6. Document estimation methodology clearly in output
7. Accept that ROI calculations will be approximate, not precise

**Warning signs:**
- Users questioning why values differ from other trackers
- Legal concerns about representing estimates as actuals
- Calculations that require exact values producing misleading results
- Comparisons between trades in different range brackets being unfair

**Phase to address:**
Phase 1 (Foundation) - schema design must accommodate ranges, not force single values. Set expectations correctly from the start.

---

### Pitfall 7: Delisted and Historical Company Data

**What goes wrong:**
Some companies in congressional trades may be delisted, merged, acquired, or renamed since the trade date. Yahoo Finance's policy now requires a premium subscription (e.g., Gold plan) to download historical data, and delisted companies are available only as part of the Premium Plus membership. For active tickers, errors like "$TSLA: possibly delisted; no price data found" are being thrown even for listed symbols.

**Why it happens:**
Yahoo Finance changed data access policies in 2025-2026, restricting historical data to paid tiers. Old trades (2-10 years ago) reference companies that no longer exist under the same ticker or are entirely delisted.

**How to avoid:**
1. Accept that some trades cannot be enriched and mark them explicitly
2. Store enrichment_status field: success, ticker_not_found, delisted, api_error, rate_limited
3. Build reporting that shows enrichment coverage percentage
4. Consider snapshot approach: enrich with price at trade date only, don't try to track current prices for old tickers
5. For critical old tickers, consider one-time data purchase from premium providers
6. Document data limitations transparently to users
7. Implement fallback: if Yahoo Finance fails, mark for manual research

**Warning signs:**
- Older trades (pre-2020) failing enrichment at higher rates
- "possibly delisted" errors for legitimate active stocks
- Missing data for companies known to have been acquired
- Enrichment success rate declining over time as more companies delist

**Phase to address:**
Phase 2 (Enrichment) - handle explicitly during bulk enrichment phase. Don't assume all tickers will resolve.

---

### Pitfall 8: Congressional Reporting Delay Impact

**What goes wrong:**
The STOCK Act requires officials to publicly disclose trades within 30 days of receiving notice, and within 45 days of the transaction date. This 30-45 day reporting delay means by the time data is available for enrichment, stock prices may have moved significantly. Enriching at disclosure date rather than transaction date produces misleading timing signals.

**Why it happens:**
Developers enrich trades when they're discovered (disclosure date) rather than when they occurred (transaction date). This makes it appear Congress is reacting to price movements when actually the price movement happened after their trade.

**How to avoid:**
1. ALWAYS use transaction_date (tx_date) for price lookups, never disclosure_date (pub_date)
2. Store both dates clearly in schema: trade_date, disclosure_date, enrichment_date
3. When displaying "days since trade" calculations, use trade_date not disclosure_date
4. Accept that enrichment is retrospective - we're looking back at historical prices
5. Test specifically: enrich a known trade and verify price matches Yahoo Finance historical data on trade_date
6. Document timing in UI: "Trade occurred on [date], disclosed on [date], enriched on [date]"

**Warning signs:**
- Price enrichment showing values that don't match trade timing narratives
- "Buy low, sell high" patterns reversed when examining actual trade dates
- User confusion about timing of trades vs. timing of disclosures
- Incorrect date range queries returning no data

**Phase to address:**
Phase 1 (Foundation) - schema design and query logic must use correct date field. This is a conceptual error that's easy to make.

---

### Pitfall 9: Market Closure Data Gaps

**What goes wrong:**
Stock markets close on weekends (Saturday/Sunday) and holidays (New Year's, Martin Luther King Jr. Day, Presidents' Day, Good Friday, Memorial Day, Independence Day, Labor Day, Thanksgiving, Christmas). When markets are closed, Yahoo Finance has no data for those dates. If a congressional trade occurs on a Friday but is disclosed as happening on Saturday, enrichment queries for Saturday return no data.

**Why it happens:**
Developers query Yahoo Finance for exact date match without checking if market was open that day. Historical data doesn't include non-trading days, creating gaps in date continuity.

**How to avoid:**
1. Implement market calendar checking (US stock market holidays)
2. If trade date falls on weekend/holiday, use previous trading day's close price
3. Store effective_price_date separately from trade_date to show which date's price was used
4. Use Yahoo Finance's date snapping behavior intentionally: "start/end dates don't match exactly - the returned data snaps to some date within a week's distance"
5. Test enrichment with trades on known holidays and weekends
6. Document price date logic in enrichment status notes
7. Consider using exchange calendar libraries (Rust equivalent of pandas_market_calendars)

**Warning signs:**
- Enrichment failures clustered around weekends
- Missing data for holiday periods
- Price data showing gaps when visualized on charts
- Inconsistent behavior for trades reported on different days of week

**Phase to address:**
Phase 2 (Enrichment) - must handle during enrichment implementation. Test cases should include weekend trades.

---

### Pitfall 10: Data Quality and Inconsistency Issues

**What goes wrong:**
Yahoo Finance data has quality issues: small issues including missing dates, inconsistent adjusted prices, sudden access limits, and datasets that quietly change without explanation. The adjusted close of Yahoo data is currently incomplete and doesn't account for dividends. Yahoo Finance was never designed to be a reliable data source for programmatic or long-term use - it's a website first, not a data infrastructure.

**Why it happens:**
Yahoo Finance is optimized for web UI consumption, not API access. Data corrections, restatements, and backfills happen without notification. Free platforms do not offer reliability commitments, and if something breaks, users typically don't get clear documentation or support.

**How to avoid:**
1. Never assume Yahoo Finance data is canonical truth
2. Implement data validation: sanity check prices (not negative, not impossibly high)
3. Flag suspicious data for manual review (e.g., 10x price jumps in one day without split)
4. Store data source and enrichment timestamp for audit trail
5. Consider periodic re-enrichment to catch data corrections (monthly/quarterly)
6. Log data quality issues separately from API errors
7. Accept that some data will be wrong - build mechanisms to detect and handle it
8. For critical analysis, cross-reference with second data source when available

**Warning signs:**
- Price data that contradicts known market events
- Adjusted close values changing on re-fetch for same date
- Missing data for dates that should exist
- Prices that don't match Yahoo Finance website UI
- Inconsistencies when comparing against other data sources

**Phase to address:**
Phase 2 (Enrichment) and Phase 4 (Monitoring) - validation during enrichment, ongoing monitoring for data quality drift.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Using raw close price instead of adjusted close | Simpler API call, less data | All calculations are wrong, splits cause false signals | Never - always use adjusted |
| Enriching at disclosure date instead of trade date | Easier query logic | Completely wrong timing analysis, misleading signals | Never - breaks fundamental analysis |
| No ticker validation before enrichment | Faster initial implementation | High failure rates, wasted API calls, polluted error logs | Never - validate first |
| Single-value storage for trade ranges | Simpler schema, easier calculations | Misrepresents data accuracy, legal concerns | Never - ranges are legally required format |
| Treating options same as stocks | Unified code path, less complexity | Options fail silently or show wrong data | Acceptable for MVP if options explicitly unsupported |
| No rate limiting on bulk operations | Faster enrichment | IP bans, API blocks, complete failure | Never - rate limiting is mandatory |
| Hardcoding Yahoo Finance specifics throughout codebase | Faster initial development | Impossible to switch providers when Yahoo breaks | Never - use abstraction layer |
| No caching of ticker → company lookups | Simpler architecture | Redundant API calls for same ticker | Acceptable for small datasets (<1000 trades) |

## Integration Gotchas

Common mistakes when connecting to Yahoo Finance API.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Historical price fetch | Fetching one day at a time | Fetch date range, extract needed date - reduces API calls |
| Options data | Using stock endpoints for options | Use options chain endpoints, parse OCC symbology |
| Date handling | Querying exact date without checking market open | Use market calendar, snap to previous trading day |
| Error handling | Treating all errors as transient | Distinguish: 404 (bad ticker), 429 (rate limit), 5xx (retry), network (retry) |
| Ticker format | Using Capitol Trades ticker as-is | Normalize: uppercase, trim whitespace, validate format |
| Response parsing | Assuming fields always exist | Check for null/missing, have defaults, validate schema |
| Price field selection | Using first price field found | Explicitly use adjusted_close for calculations |
| Bulk operations | Sequential enrichment of all trades | Batch by ticker (multiple trades same ticker), pace requests |

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| No request pacing | Works fine for 10 trades, fails at 100 | Implement delays and concurrent limits from start | >50 requests in short time |
| Synchronous enrichment | Blocks CLI, appears frozen | Use async with progress indicators | >100 trades |
| No query result caching | Every ticker fetched multiple times | Cache ticker data in-memory during batch run | >500 trades with repeated tickers |
| Re-enriching already enriched trades | Wasted API calls on every sync | Check enrichment_status before enriching | Every re-run |
| Fetching full options chain for single contract | Huge payload, slow response | Parse OCC symbol, use targeted query if possible | Every options trade |
| No database indexing on trade_date, ticker | Slow queries as trades table grows | Index on (ticker, trade_date) and enrichment_status | >10,000 trades |
| Loading all trades into memory | Works on laptop, fails in production | Stream/paginate large queries | >50,000 trades |
| No progress tracking for long enrichment runs | User doesn't know if it's working | Log progress every N tickers, show percentage | >500 trades (>30 min) |

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Logging full API responses | Yahoo Finance responses may contain PII or sensitive data | Log only status codes and error messages, not bodies |
| Storing API keys in code | If Capitol Traders adds paid Yahoo Finance API | Use environment variables, never commit credentials |
| Exposing rate limit bypass techniques | Could enable abuse if API keys used | Keep rate limit implementation internal |
| Displaying exact trade values from ranges | Misrepresenting data accuracy, potential legal issues | Always show ranges, label estimates clearly |
| No input sanitization on ticker symbols | SQL injection if ticker used in raw SQL | Use parameterized queries (already done in Capitol Traders) |
| Caching sensitive data without TTL | Stale data could misrepresent current state | Implement cache expiration (Capitol Traders uses 300s) |

## UX Pitfalls

Common user experience mistakes in this domain.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Showing enrichment failures as errors | Users think something is broken | "X trades could not be enriched (ticker not found, delisted, etc.)" with stats |
| No progress indication during bulk enrichment | CLI appears frozen, users kill process | Progress bar: "Enriching... 45/200 trades (22%)" |
| Mixing enriched and unenriched data without indication | Confusing comparisons, incomplete analysis | Visual indicators: enriched trades marked with icon/badge |
| Claiming exact portfolio values | Users make financial decisions based on false precision | "Estimated value (midpoint of range): ~$45,000" |
| Not explaining date delays | Users think Congress has insider info on recent events | "Trade occurred [45 days ago], disclosed [today]" |
| Showing failed enrichment attempts as missing data | Users don't know if enrichment was attempted | Enrichment status field: pending, success, failed_ticker_not_found, failed_api_error |
| No way to retry failed enrichments | Transient failures permanent | `--retry-failed` flag to re-attempt previously failed enrichments |
| Displaying raw adjusted prices without context | "$1.23" - is this post-split? adjusted? | "Adjusted close: $1.23 (accounts for splits/dividends)" |

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Price Enrichment:** Often missing split/dividend adjustments - verify using adjusted_close field, test with recently split stock
- [ ] **Ticker Validation:** Often missing exchange suffix handling - verify .TO, .L, ^GSPC special characters work
- [ ] **Date Handling:** Often missing market calendar - verify trades on weekends/holidays use previous trading day
- [ ] **Error Handling:** Often missing 429 rate limit exponential backoff - verify batch enrichment slows down on rate limit errors
- [ ] **Options Support:** Often missing OCC symbol parsing - verify options trades identified and enriched separately from stocks
- [ ] **Range Storage:** Often missing range_high/range_low fields - verify schema stores original ranges not just midpoint estimates
- [ ] **Enrichment Status:** Often missing detailed failure reasons - verify can distinguish ticker_not_found from api_error from rate_limited
- [ ] **Progress Tracking:** Often missing for long operations - verify enrichment shows progress for >100 trades
- [ ] **Caching:** Often missing per-ticker cache - verify same ticker requested multiple times uses cache
- [ ] **Delisting Handling:** Often missing explicit delisted status - verify old trades for delisted companies marked appropriately
- [ ] **Transaction Date Usage:** Often missing correct date field - verify using tx_date not pub_date for price lookups
- [ ] **Data Validation:** Often missing sanity checks - verify negative prices, impossibly high prices flagged
- [ ] **Retry Logic:** Often missing idempotency - verify re-running enrichment doesn't duplicate or corrupt data
- [ ] **Abstraction Layer:** Often missing provider abstraction - verify Yahoo Finance calls isolated to single module for easy swapping

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Used raw close instead of adjusted close | HIGH | Re-enrich all trades with adjusted_close; update schema to enforce field; add tests to prevent regression |
| Enriched at disclosure date instead of trade date | HIGH | Re-enrich all trades using correct date; fix queries; add validation test comparing to known trade |
| No ticker validation | MEDIUM | Add validation layer; re-enrich failed trades with validation; track failure reasons separately |
| Hit rate limits / IP banned | LOW | Wait 24 hours; implement rate limiting; add delays; use circuit breaker for future |
| Treated options as stocks | MEDIUM | Add option detection; create separate enrichment path; re-enrich option trades; mark unsupported for MVP |
| Stored single value for ranges | LOW-MEDIUM | Add range fields to schema; migrate data; update display logic; keep original ranges |
| No split adjustment awareness | HIGH | Re-enrich all trades; add split detection tests; document adjusted_close usage |
| Hardcoded Yahoo Finance throughout | HIGH | Create abstraction layer; refactor all calls to use abstraction; add provider interface |
| No caching of repeated tickers | LOW | Add in-memory cache for batch operations; measure API call reduction |
| Delisted tickers failing silently | MEDIUM | Add explicit delisted status; re-attempt enrichment with new error handling; report statistics |
| Market closure dates not handled | MEDIUM | Add market calendar checking; re-enrich weekend/holiday trades; document effective dates |
| Yahoo Finance endpoint changed | MEDIUM-HIGH | Update to latest library version; implement alternative provider; use cached data temporarily |

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Unofficial API fragility | Phase 1: Abstraction layer + error handling | Can swap providers by changing config, not code |
| Ticker symbol mismatch | Phase 2: Ticker validation + normalization | Enrichment success rate >90% for major tickers |
| Split/dividend adjustments | Phase 1: Use adjusted_close from start | Test case: NVDA 2024 split shows correct historical prices |
| Options complexity | Phase 3: Options support OR explicitly defer | Schema accommodates options, or options marked unsupported |
| Rate limiting | Phase 1: Pacing + semaphore + backoff | Can enrich 1000+ trades without IP ban |
| Trade value ranges | Phase 1: Schema with range fields | Database stores range_low + range_high, not single value |
| Delisted companies | Phase 2: Enrichment status tracking | Delisted trades marked explicitly, stats reported |
| Reporting delay | Phase 1: Use tx_date for enrichment | Test: enrichment uses trade date not disclosure date |
| Market closures | Phase 2: Market calendar integration | Weekend trades use Friday close price |
| Data quality | Phase 2: Validation + Phase 4: Monitoring | Sanity checks flag outliers, re-enrichment capability exists |

## Sources

**API Stability & Rate Limiting:**
- [Why yfinance Keeps Getting Blocked](https://medium.com/@trading.dude/why-yfinance-keeps-getting-blocked-and-what-to-use-instead-92d84bb2cc01)
- [Yahoo Finance API Complete Guide](https://algotrading101.com/learn/yahoo-finance-api-guide/)
- [Rate Limiting Best Practices for yfinance](https://www.slingacademy.com/article/rate-limiting-and-api-best-practices-for-yfinance/)
- [Navigating Yahoo Finance API Call Limit](https://apipark.com/technews/RZtyppGC.html)

**Ticker Symbols & Corporate Actions:**
- [Yahoo Finance Ticker Symbol Lookup](https://help.yahoo.com/kb/SLN2257.html)
- [Yahoo Finance Stock Ticker Guide](https://www.bitget.com/wiki/yahoo-finance-stock-ticker)
- [Ticker Mapping Corporate Symbol Map](https://www.tickdata.com/product/corporate-symbol-maps/)
- [Stock Symbol Change History](https://www.nasdaq.com/market-activity/stocks/symbol-change-history)
- [Symbol Change History API](https://eodhd.com/financial-apis-blog/symbol-change-history-api)

**Stock Splits & Adjustments:**
- [What is Adjusted Close?](https://help.yahoo.com/kb/SLN28256.html)
- [Price Data Adjustments](https://help.stockcharts.com/data-and-ticker-symbols/data-availability/price-data-adjustments)
- [Split-Adjusted vs Raw Stock Prices](https://www.stocktitan.net/articles/split-adjusted-price-vs-raw-price)
- [Adjusted vs Unadjusted Prices](https://www.koyfin.com/help/adjusted-vs-unadjusted-prices/)

**Options Data:**
- [How to Read an Option Symbol](https://help.yahoo.com/kb/SLN13884.html)
- [Option Symbology Initiative](https://www.fidelity.com/webcontent/ap102701-quotes-content/18.11/shtml/osi.shtml)
- [Yahoo Finance Options Data Download](https://www.fintut.com/yahoo-finance-options-python/)
- [Options Symbology OCC](https://www.optionstaxguy.com/option-symbols-osi)

**Delisted Stocks & Historical Data:**
- [New Premium Plus Feature: Delisted Companies](https://finance.yahoo.com/news/premium-plus-feature-historical-financial-201155209.html)
- [yfinance Issue #359: Symbol May Be Delisted](https://github.com/ranaroussi/yfinance/issues/359)
- [yfinance Issue #2340: Premium Subscription Requirement](https://github.com/ranaroussi/yfinance/issues/2340)

**Congressional Trading & STOCK Act:**
- [STOCK Act Summary](https://www.capitoltrades.com/articles/what-is-the-stock-act)
- [How to Estimate Congressional Trading Values](https://nancypelosistocktracker.org/articles/estimating-trade-values)
- [Congressional Stock Trading Explained](https://www.brennancenter.org/our-work/research-reports/congressional-stock-trading-explained)
- [Politician Trading Analysis](https://www.ballardspahr.com/insights/alerts-and-articles/2024/10/politician-trading-if-you-cant-stop-them-join-them)

**Data Quality & Reliability:**
- [Where to Get Reliable Historical Stock Data](https://medium.com/predict/where-to-get-reliable-historical-stock-market-data-when-yahoo-finance-isnt-enough-ddf59a66b18b)
- [Fixing Yahoo Finance Download Errors](https://robotwealth.com/solved-errors-downloading-stock-price-data-yahoo-finance/)
- [yfinance Issue #2052: Is Yahoo Finance Broken?](https://github.com/ranaroussi/yfinance/issues/2052)

**Caching & Performance:**
- [yfinance Caching and Performance](https://deepwiki.com/ranaroussi/yfinance/6.2-caching-and-rate-limiting)
- [yfinance-cache PyPI](https://pypi.org/project/yfinance-cache/)
- [Caching Strategies to Know in 2026](https://www.dragonflydb.io/guides/caching-strategies-to-know)

**Rust Libraries:**
- [yahoo_finance_api crate](https://crates.io/crates/yahoo_finance_api)
- [yahoo-finance crate](https://crates.io/crates/yahoo-finance)
- [yfinance_rs docs](https://docs.rs/yfinance-rs)

**Database Design:**
- [Storing Financial Time-Series Data Efficiently](https://ericdraken.com/storing-stock-candle-data-efficiently/)
- [SQLite and Temporal Tables](https://www.sqliteforum.com/p/sqlite-and-temporal-tables)
- [Handling Time Series Data in SQLite](https://moldstud.com/articles/p-handling-time-series-data-in-sqlite-best-practices)

---
*Pitfalls research for: Yahoo Finance Market Data Integration*
*Researched: 2026-02-09*
