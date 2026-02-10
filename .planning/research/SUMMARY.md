# Research Summary: Yahoo Finance Price Enrichment

**Domain:** Stock price enrichment for congressional trade tracking CLI tool
**Researched:** 2026-02-09
**Overall confidence:** HIGH

## Executive Summary

Adding Yahoo Finance price data to Capitol Traders is straightforward and low-risk. The yahoo_finance_api crate (v4.1.0) integrates seamlessly with our existing Rust stack (reqwest 0.12, tokio 1.x, rusqlite 0.31) and provides exactly the functionality needed: fetch historical closing prices for trade dates and current prices for ongoing tracking. The library is mature (4+ years, v4.x), actively maintained (last update 2024), and has minimal dependencies.

The primary risk is not technical but environmental: Yahoo Finance shut down its official API in 2017, and current endpoints are unofficial/reverse-engineered. They could break at any time. However, this is mitigated by (1) the yahoo_finance_api maintainers tracking endpoint changes, (2) our existing circuit breaker pattern for graceful degradation, and (3) treating price enrichment as an enhancement rather than core functionality.

Rate limiting is a known issue (Python yfinance users report 429 errors after ~950 tickers), but our use case is inherently batched and can tolerate delays. We already have rate limiting infrastructure from the Capitol Trades enrichment pipeline that can be reused. For 200 unique tickers across 1000 trades, sequential fetching with 300ms delays takes ~60 seconds total, which is acceptable for a sync operation.

Data quality is excellent for our use case: Yahoo Finance provides split-adjusted and dividend-adjusted closing prices, which are the standard for calculating gain/loss percentages on congressional trades. We'll fetch both trade-date price and current price, store in SQLite, and calculate unrealized P&L in the CLI output layer.

## Key Findings

**Stack:** Use yahoo_finance_api 4.1.0 (mature, compatible, focused) instead of yfinance-rs 0.7.2 (feature bloat, heavier dependencies) or building a custom client (unnecessary complexity, maintenance burden).

**Architecture:** Extend existing enrichment pipeline pattern (Semaphore + JoinSet + mpsc + circuit breaker) with a new YahooEnrichment phase. Deduplicate tickers before fetching (1000 trades -> ~200 tickers), cache in-memory during run (DashMap), persist to SQLite. Add three columns to trades table: trade_date_price, current_price, price_enriched_at.

**Critical pitfall:** Yahoo Finance API is unofficial and can break without notice. Must implement graceful degradation (display last-enriched prices with timestamp caveat, don't fail CLI if Yahoo is down) and monitoring (log enrichment success rate, alert if >7 days stale).

## Implications for Roadmap

Based on research, suggested phase structure for Yahoo Finance milestone:

1. **Phase 1: Data Model & Schema Migration** - Add price columns to trades table, write migration, update DbTrade struct
   - Addresses: Foundation for storing enriched price data
   - Avoids: Schema changes mid-development (backwards compatibility pain)
   - Estimated complexity: Low (standard SQLite migration pattern)

2. **Phase 2: Yahoo Client Integration** - Add yahoo_finance_api dependency, create YahooClient wrapper, write conversion helpers (chrono <-> time crate)
   - Addresses: Core integration, date/time library mismatch
   - Avoids: Scattering time/chrono conversions across codebase
   - Estimated complexity: Low (thin wrapper, well-documented API)

3. **Phase 3: Enrichment Pipeline** - Implement ticker deduplication, batch fetching with rate limiting, in-memory caching, SQLite persistence
   - Addresses: Bulk enrichment efficiency, rate limit mitigation
   - Avoids: Fetching same ticker multiple times, overwhelming Yahoo API
   - Estimated complexity: Medium (reuse existing enrichment patterns, but new domain)

4. **Phase 4: CLI Integration & Display** - Add price columns to output formatters (table, CSV, JSON, markdown, XML), calculate gain/loss percentages
   - Addresses: User-facing value (price data visible in CLI)
   - Avoids: Breaking existing output formats
   - Estimated complexity: Low (extend existing output.rs patterns)

5. **Phase 5: Error Handling & Monitoring** - Implement circuit breaker for Yahoo failures, graceful degradation messaging, enrichment success rate logging
   - Addresses: Production reliability, Yahoo API instability risk
   - Avoids: Silent failures, user confusion when prices unavailable
   - Estimated complexity: Low (reuse existing circuit breaker, add logging)

6. **Phase 6: Testing & Validation** - Fixture-based tests for Yahoo responses, edge case testing (delisted tickers, date conversions), integration tests with wiremock
   - Addresses: Data quality, correctness
   - Avoids: Production surprises, silent data corruption
   - Estimated complexity: Medium (need realistic Yahoo API fixtures)

**Phase ordering rationale:**
- Schema first prevents mid-development migrations
- Client integration before enrichment pipeline (dependency)
- Enrichment before CLI display (data must exist before displaying)
- Error handling after core functionality (can test failures once happy path works)
- Testing throughout, but dedicated phase at end for edge cases

**Research flags for phases:**
- Phase 3: Likely needs deeper research into rate limiting (no official documentation, community reports vary, may need empirical testing)
- Phase 6: Likely needs deeper research into Yahoo Finance data edge cases (delisted stocks, ticker symbol changes, splits/dividends timing)
- Phases 1, 2, 4, 5: Standard patterns, unlikely to need additional research

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | yahoo_finance_api v4.1.0 verified via docs.rs, Cargo.toml inspection, compatibility with our stack confirmed |
| Features | HIGH | Yahoo Finance API capabilities well-documented, matches our use case (historical + current prices) |
| Architecture | HIGH | Reuses existing enrichment pipeline patterns, extends proven DB migration approach |
| Pitfalls | MEDIUM-HIGH | Yahoo API instability documented via Python yfinance issues, rate limiting anecdotal but consistent across sources, mitigation strategies standard |

## Gaps to Address

**Rate Limiting Specifics:** No official documentation on Yahoo Finance rate limits. Python yfinance community reports 429 errors after ~950 tickers (Nov 2024 - April 2025), but limits may vary by region, IP reputation, or request pattern. Recommendation: Start conservative (300ms delays, max 5 concurrent), monitor 429 responses, adjust based on empirical data.

**Delisted Ticker Handling:** Research did not cover how Yahoo Finance responds to delisted stocks or invalid ticker symbols. May return 404, empty data, or error response. Needs empirical testing in Phase 6 or flagged as "needs investigation" in roadmap.

**Date/Time Edge Cases:** Conversion between chrono::NaiveDate (our codebase) and time::OffsetDateTime (yahoo_finance_api) is straightforward for typical dates, but edge cases (leap seconds, timezone transitions, historical dates pre-1970) not verified. Low risk given our use case (recent congressional trades, always UTC), but should have unit tests.

**Yahoo Finance Data Quality:** Research focused on API availability/structure, not data quality. Assumptions: (1) adjusted close prices are accurate, (2) historical data is backfilled correctly, (3) splits/dividends are reflected properly. These are standard financial data practices, but not independently verified. Consider spot-checking against alternative source (e.g., manually verify 10 random ticker-date combinations against Google Finance) in Phase 6.

**Alternative Fallback:** If Yahoo Finance becomes permanently unavailable, no fallback researched. Free alternatives (Alpha Vantage, Finnhub) have rate limits too restrictive for bulk enrichment. Paid alternatives (Polygon.io, EODHD) add complexity and cost. Recommendation: Accept risk, document workaround (manual price entry table, CSV import), revisit if Yahoo breaks.

## Sources

See STACK.md for comprehensive source list. Key sources:
- [yahoo_finance_api on docs.rs](https://docs.rs/yahoo_finance_api/latest/yahoo_finance_api/) - PRIMARY
- [yahoo_finance_api Cargo.toml](https://docs.rs/crate/yahoo_finance_api/latest/source/Cargo.toml.orig) - PRIMARY
- [What Happened to Yahoo Finance API - Medium](https://medium.com/@dineshjoshi/what-happened-to-the-yahoo-finance-api-857c2a6abb6d) - SECONDARY (API history)
- [yfinance Rate Limit Issues](https://github.com/ranaroussi/yfinance/issues/2422) - SECONDARY (rate limiting evidence)
