# Technology Stack - Yahoo Finance Integration

**Project:** Capitol Traders - Yahoo Finance Price Enrichment
**Researched:** 2026-02-09

## Recommended Stack

### Yahoo Finance Client Library
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| yahoo_finance_api | 4.1.0 | Fetch historical and current stock prices from Yahoo Finance | Battle-tested (since 2018), minimal dependencies, actively maintained (latest update 2024), uses our existing reqwest 0.12 and tokio stack, supports async/await natively, no authentication required |

**Rationale:** Use yahoo_finance_api instead of building a custom client or using yfinance-rs because:
1. **Proven compatibility** - Already uses reqwest 0.12.19 (we have 0.12) and tokio 1.45.1 (we have 1.x)
2. **Minimal surface area** - Focused on price data only, not a kitchen-sink library
3. **No heavy dependencies** - Doesn't pull in polars or other data analysis libraries we don't need
4. **Simple API** - `get_quote_history()` and `get_latest_quotes()` match our use case exactly
5. **Mature** - 4+ years in production, 199 commits, version 4.x indicates stable API

**Confidence:** HIGH (verified via official docs.rs and Cargo.toml inspection)

### Supporting Dependencies (Already in Workspace)
| Technology | Current Version | Purpose | Notes |
|------------|----------------|---------|-------|
| reqwest | 0.12 | HTTP client (used by yahoo_finance_api) | Already workspace dependency, compatible |
| tokio | 1.x | Async runtime | Already workspace dependency, compatible |
| serde / serde_json | 1.x | JSON deserialization | Already workspace dependency, used for Yahoo API responses |
| chrono | 0.4 | Date/time handling | Already workspace dependency, yahoo_finance_api uses time crate (compatible) |
| rusqlite | 0.31 | Store enriched price data | Already workspace dependency |

### New Dependencies Required
| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| time | 0.3.41 | Date/time conversion | Required by yahoo_finance_api for timestamps; compatible with our chrono 0.4 usage (we'll convert between them) |

**Confidence:** HIGH (verified via Cargo.toml analysis)

## Alternatives Considered

| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| Yahoo Finance Client | yahoo_finance_api 4.1.0 | yfinance-rs 0.7.2 | Heavy dependencies (polars 0.51 optional but adds complexity), feature bloat (WebSocket streaming, ESG data, fundamentals we don't need), less mature (v0.x vs v4.x), updated Aug 2025 but newer means less battle-tested |
| Yahoo Finance Client | yahoo_finance_api 4.1.0 | Custom reqwest client | Reinventing the wheel, Yahoo Finance API has quirks (cookie handling, crumb tokens for some endpoints), yahoo_finance_api handles these, maintenance burden of tracking Yahoo API changes |
| Yahoo Finance Client | yahoo_finance_api 4.1.0 | yahoo-finance 0.3.0 (patchfx) | Last updated 2020, unmaintained, uses older reqwest patterns |
| Data Source | Yahoo Finance (free, unofficial) | Alpha Vantage | Requires API key, free tier limited to 25 requests/day (we may have 1000s of tickers to enrich), rate limits too restrictive for bulk enrichment |
| Data Source | Yahoo Finance (free, unofficial) | Finnhub | Requires API key, 60 calls/minute free tier insufficient for bulk enrichment, introduces authentication complexity |
| Data Source | Yahoo Finance (free, unofficial) | IEX Cloud | Shut down August 2024, no longer viable |
| Data Source | Yahoo Finance (free, unofficial) | Polygon.io | Premium only, no free tier, overkill for congressional trade tracking |

## Installation

```toml
# Add to capitoltraders_lib/Cargo.toml
[dependencies]
yahoo_finance_api = "4.1.0"
time = "0.3"  # Required by yahoo_finance_api
```

No changes needed to workspace dependencies - reqwest 0.12, tokio 1.x, serde, and chrono already compatible.

## Integration Pattern

```rust
// Minimal example matching our existing enrichment pipeline pattern
use yahoo_finance_api::{YahooConnector, Quote};
use time::OffsetDateTime;

// Initialize once, reuse (similar to our ScrapeClient pattern)
let yahoo_client = YahooConnector::new()?;

// Fetch historical price for a specific date (trade date enrichment)
let start = OffsetDateTime::from_unix_timestamp(trade_date_as_unix)?;
let end = start + Duration::days(1);
let quotes = yahoo_client
    .get_quote_history("AAPL", start, end)
    .await?
    .quotes()?;

// quotes[0].close is the closing price for that date

// Fetch current price (latest enrichment)
let latest = yahoo_client
    .get_latest_quotes("AAPL", "1d")
    .await?
    .last_quote()?;

// latest.close is the current closing price
```

## Known Limitations & Mitigation Strategies

### Yahoo Finance API Stability (CRITICAL)
**Issue:** Yahoo Finance shut down official API in 2017. Current endpoints are unofficial, reverse-engineered, and can break without notice.

**Risk:** Medium-High. Yahoo can change endpoints, add rate limiting, or block scraping at any time.

**Mitigation:**
1. **Graceful degradation** - Store "last successful enrichment timestamp" in DB, don't fail CLI commands if Yahoo is down
2. **Circuit breaker** - Already have circuit breaker pattern from existing enrichment pipeline, reuse for Yahoo failures
3. **Fallback messaging** - If Yahoo unavailable, display last-enriched prices with timestamp caveat
4. **No critical dependencies** - Price enrichment is enhancement, not core feature
5. **Monitoring** - Log Yahoo API failures to detect breaking changes early

**Confidence:** HIGH (verified via multiple sources documenting Yahoo API history)

### Rate Limiting
**Issue:** Yahoo Finance implements rate limits. Python yfinance users report 429 errors after ~950 tickers (Nov 2024 - April 2025).

**Risk:** Medium. Bulk enrichment of 1000s of trades could hit rate limits.

**Mitigation:**
1. **Respect existing rate limiter** - Reuse our existing rate limiting infrastructure (already in enrichment pipeline)
2. **Batch intelligently** - Group requests by ticker (fetch AAPL once, use for all AAPL trades)
3. **Jittered delays** - Add 200-500ms random delay between ticker requests
4. **Resume on 429** - Circuit breaker trips on consecutive 429s, resume after exponential backoff
5. **Cache aggressively** - Store prices in DB, never re-fetch same ticker-date combination

**Confidence:** MEDIUM (rate limiting documented in Python yfinance issues, but limits not officially published)

### Date/Time Library Mismatch
**Issue:** yahoo_finance_api uses `time` crate v0.3, we use `chrono` v0.4 throughout codebase.

**Risk:** Low. Conversion overhead, potential timezone bugs.

**Mitigation:**
1. **Conversion layer** - Create thin adapter functions `chrono::NaiveDate -> time::OffsetDateTime` and back
2. **Explicit UTC** - Always convert to UTC, never rely on local time
3. **Test edge cases** - Unit test conversions for leap years, DST transitions, epoch boundaries
4. **Centralized conversion** - Put all time/chrono conversions in one module, don't scatter through codebase

**Confidence:** HIGH (both crates widely used, conversion patterns well-documented)

### Ticker Symbol Mismatches
**Issue:** Capitol Trades data may have ticker symbols that don't exist in Yahoo Finance (delisted stocks, typos, non-US exchanges).

**Risk:** Medium. Failed lookups for valid trades.

**Mitigation:**
1. **404 is not an error** - Log missing tickers, don't fail enrichment pipeline
2. **Manual override table** - SQLite table for "known bad tickers" to skip retries
3. **Fuzzy matching fallback** - For failed lookups, try `search_ticker()` to find close matches, log for manual review
4. **Preserve original data** - Never modify trade.issuer_ticker, only add enriched price columns

**Confidence:** MEDIUM (based on general data quality patterns, not Yahoo-specific verification)

## API Endpoints Used (Unofficial)

Based on yahoo_finance_api source code analysis and community documentation:

- **Chart API (historical data):** `https://query2.finance.yahoo.com/v8/finance/chart/{ticker}`
  - Parameters: `period1` (Unix timestamp start), `period2` (Unix timestamp end), `interval` (1d, 1wk, 1mo)
  - Returns: OHLCV data (open, high, low, close, volume, adjusted close)

- **Quote API (latest data):** Via chart endpoint with recent period

**Authentication:** None required. yahoo_finance_api handles cookie/crumb token extraction internally.

**Confidence:** MEDIUM (endpoint URLs verified in yahoo_finance_api source, but unofficial so subject to change)

## Performance Characteristics

Based on yahoo_finance_api implementation and community reports:

| Operation | Latency | Notes |
|-----------|---------|-------|
| Single ticker historical lookup | 200-500ms | Depends on date range, network latency |
| Single ticker latest quote | 100-300ms | Faster than historical (smaller response) |
| Bulk ticker requests (sequential) | ~300ms per ticker | With rate limiting delays |
| Bulk ticker requests (concurrent) | Risk 429 errors | Don't recommend >5 concurrent |

**Recommendation:** For 1000 trades across 200 unique tickers:
- Deduplicate by ticker first (1000 trades -> ~200 unique tickers)
- Fetch sequentially with 300ms delay = ~60 seconds total
- Use existing Semaphore-based concurrency limiter (set to 3-5 permits)
- Cache results in memory during enrichment run (DashMap), persist to SQLite

**Confidence:** MEDIUM (based on reqwest defaults and community reports, not official benchmarks)

## Data Quality Considerations

### What Yahoo Finance Provides
- **Adjusted close prices** - Split-adjusted, dividend-adjusted (use this for historical accuracy)
- **Unadjusted close prices** - Raw closing price on that date
- **Volume data** - Trading volume (useful for trade size context)
- **Timestamp precision** - Daily granularity for historical, 1-minute for latest quotes

### What Yahoo Finance Does NOT Provide
- **Intraday historical prices** (free tier) - Can't get exact trade execution time price
- **Pre-market/after-hours** - Only regular trading hours
- **Options pricing** - Stock prices only (our use case doesn't need options)

### Recommendation for Capitol Traders
Use **adjusted close price** for:
- **Trade date price** - What the stock was worth on the date the politician traded it
- **Current price** - What the stock is worth now

Calculate:
- **Gain/Loss %** = ((current_price - trade_date_price) / trade_date_price) * 100
- **Unrealized P&L** = (current_price - trade_date_price) * estimated_shares

Store in SQLite:
```sql
ALTER TABLE trades ADD COLUMN trade_date_price REAL;
ALTER TABLE trades ADD COLUMN current_price REAL;
ALTER TABLE trades ADD COLUMN price_enriched_at TEXT;  -- ISO 8601 timestamp
```

**Confidence:** HIGH (standard financial data practice, verified via yahoo_finance_api API documentation)

## Security Considerations

### No Sensitive Data Exposure
- Yahoo Finance API requires no API keys or authentication
- All data fetched is public market data
- No risk of leaking credentials

### HTTP Security
- yahoo_finance_api uses rustls-tls (no OpenSSL dependency)
- Our workspace already configures reqwest with rustls-tls
- All Yahoo Finance endpoints use HTTPS

### Data Validation
- Validate price data is non-negative, non-null before storing
- Reject prices outside reasonable bounds (e.g., > $100M per share likely data error)
- Log anomalies for manual review (e.g., 1000x price jump in one day)

**Confidence:** HIGH (standard security practices, verified reqwest configuration)

## Maintenance & Monitoring Plan

### Ongoing Maintenance
1. **Watch yahoo_finance_api releases** - Check crates.io monthly for updates (especially if Yahoo changes APIs)
2. **Monitor circuit breaker trips** - Alert if >10% of enrichment requests fail consecutively
3. **Track rate limit errors** - Log 429 responses, adjust delays if pattern emerges
4. **Version pin** - Pin to 4.1.x in Cargo.toml, test minor upgrades in CI before deploying

### Failure Detection
1. **Enrichment success rate metric** - Log % of tickers successfully enriched per run
2. **Last successful enrichment timestamp** - Store per ticker, alert if >7 days stale for active trades
3. **Price data sanity checks** - Alert if >5% of prices are null after enrichment run

### Rollback Plan
If Yahoo Finance API breaks:
1. Circuit breaker trips automatically (existing pattern)
2. CLI continues to work with last-enriched prices (graceful degradation)
3. Users see "(prices as of YYYY-MM-DD)" disclaimer
4. Option to disable price enrichment via CLI flag: `--skip-price-enrichment`

**Confidence:** HIGH (leverages existing patterns, adds minimal new risk)

## Sources

**Crate Documentation & Analysis:**
- [yahoo_finance_api on crates.io](https://crates.io/crates/yahoo_finance_api)
- [yahoo_finance_api documentation on docs.rs](https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/)
- [yahoo_finance_api Cargo.toml](https://docs.rs/crate/yahoo_finance_api/latest/source/Cargo.toml.orig)
- [yahoo_finance_api GitHub repository](https://github.com/xemwebe/yahoo_finance_api)
- [yfinance-rs on crates.io](https://crates.io/crates/yfinance-rs)
- [yfinance-rs documentation on docs.rs](https://docs.rs/yfinance-rs)
- [yfinance-rs GitHub repository](https://github.com/gramistella/yfinance-rs)

**Yahoo Finance API Status & History:**
- [Yahoo Finance API Guide - ScrapFly](https://scrapfly.io/blog/posts/guide-to-yahoo-finance-api)
- [AlgoTrading101 Yahoo Finance API Guide](https://algotrading101.com/learn/yahoo-finance-api-guide/)
- [Why yfinance Keeps Getting Blocked - Medium](https://medium.com/@trading.dude/why-yfinance-keeps-getting-blocked-and-what-to-use-instead-92d84bb2cc01)
- [What Happened to Yahoo Finance API - Medium](https://medium.com/@dineshjoshi/what-happened-to-the-yahoo-finance-api-857c2a6abb6d)

**Rate Limiting Issues:**
- [yfinance Issue #2422 - Rate Limit Errors](https://github.com/ranaroussi/yfinance/issues/2422)
- [yfinance Issue #2128 - New Rate Limiting](https://github.com/ranaroussi/yfinance/issues/2128)

**Alternative APIs:**
- [Financial Data APIs 2025 Guide - KSRed](https://www.ksred.com/the-complete-guide-to-financial-data-apis-building-your-own-stock-market-data-pipeline-in-2025/)
- [Comparing Live Market Data APIs - DEV Community](https://dev.to/williamsmithh/comparing-live-market-data-apis-which-one-is-right-for-your-project-4f71)
- [Alpha Vantage vs Finnhub vs IEX Cloud Comparison - SourceForge](https://sourceforge.net/software/compare/Alpha-Vantage-vs-Finnhub-vs-IEX-Cloud/)

**Rust Async Ecosystem:**
- [reqwest GitHub repository](https://github.com/seanmonstar/reqwest)
- [tokio-rusqlite on lib.rs](https://lib.rs/crates/tokio-rusqlite)
- [reqwest 0.12.26 documentation](https://docs.rs/crate/reqwest/latest)

**Rust Yahoo Finance Implementation Examples:**
- [Building Currency Exchange Tracker with Rust - Medium](https://medium.com/@ekfqlwcjswl/building-a-currency-exchange-rate-tracker-with-rust-and-yahoo-finance-0d93ee9516f1)
- [Processing Financial Data in Rust - Bernardo de Lemos](http://bernardo.shippedbrain.com/rust_process_and_download_stock_data/)
