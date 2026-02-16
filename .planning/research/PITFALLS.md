# Domain Pitfalls: Congressional Trade Analytics & Scoring

**Domain:** Trade Performance Analytics with Disclosure Delays and Imprecise Data
**Researched:** 2026-02-14
**Confidence:** HIGH

## Executive Summary

Adding analytics and scoring to congressional trade tracking presents unique challenges due to inherent data limitations: 30-45 day disclosure delays (STOCK Act), estimated shares from dollar ranges, value ranges instead of exact amounts, fuzzy employer matching, and incomplete committee data. The seven critical pitfalls are: (1) **Look-Ahead Bias** - using price data not available at trade notification date; (2) **Survivorship Bias** - excluding delisted stocks inflates returns by 14-26%; (3) **Imprecise Share Estimation** - dollar ranges ($15K-$50K) create 40-70% variance in calculated returns; (4) **Disclosure Delay Artifacts** - 30-day lag makes trades appear to predict events they learned about; (5) **False Positive Clustering** - small sample sizes (10-50 trades/politician/year) create spurious patterns; (6) **Benchmark Mismatch** - sector ETF selection and dividend adjustment materially change performance rankings; (7) **Correlation as Causation** - committee membership correlates with sector exposure but doesn't prove insider knowledge.

**Recommended Strategy:** Treat all performance metrics as estimates with confidence intervals. Use trade notification date (not execution date) for benchmark comparison. Include delisted stocks with final settlement prices. Display share estimates as ranges, not point values. Require minimum sample sizes (30+ trades) before showing anomaly scores. Provide multiple benchmark options (S&P 500, sector ETFs, volatility-matched portfolios). Never present correlation coefficients without confounding variable analysis.

---

## Critical Pitfalls

### Pitfall 1: Look-Ahead Bias in Benchmark Comparison

**Severity:** CRITICAL

**What goes wrong:**
Using stock prices or financial data that wasn't available at the time of the trade decision. Most common forms: (1) comparing to "next day's opening price" when trade was reported after market close; (2) using final-year revenue figures released weeks after year-end; (3) applying sector classifications that changed retroactively; (4) using dividend-adjusted returns that include dividends paid after the analysis date.

**Why it happens:**
STOCK Act disclosure timeline creates confusion about "trade date" vs "notification date" vs "analysis date". Trades must be disclosed within 30 days of notification but no later than 45 days after execution. Historical price datasets often include retroactive adjustments (splits, dividends, sector reclassifications) that weren't known at the trade date. Developers naturally use `current_price` fields without checking `price_as_of_date` timestamps.

**Real-world example:**
A momentum strategy backtest showed 26% CAGR, but when look-ahead bias was removed (using only point-in-time data), returns dropped to 12.2%. Look-ahead bias inflates Sharpe ratios above 1.5 and creates unrealistically smooth equity curves.

**Consequences:**
- Performance metrics appear 2-3x better than achievable in real-time
- Anomaly detection flags trades as "suspicious" that simply benefited from public information released after disclosure
- Users attempt to copy trades believing politicians have 26% alpha when actual edge is 5-8%
- Regulatory scrutiny if analysis suggests insider trading based on artifactual data

**Prevention:**

1. **Strict Date Discipline:**
   - Use `disclosure_date` (when trade became public) as benchmark comparison anchor, NOT `trade_date`
   - Store `price_as_of_date` alongside every price field in database
   - Never compare performance using data timestamped after `disclosure_date + 1 day`

2. **Point-in-Time Data Validation:**
   ```sql
   -- CORRECT: Price known at disclosure time
   SELECT
       t.tx_id,
       t.disclosure_date,
       p.price,
       p.price_date
   FROM trades t
   JOIN prices p ON p.ticker = t.ticker
       AND p.price_date <= DATE(t.disclosure_date)
   ORDER BY p.price_date DESC
   LIMIT 1;

   -- WRONG: Using current_price regardless of when it was known
   SELECT t.tx_id, i.current_price FROM trades t JOIN issuers i ON i.ticker = t.ticker;
   ```

3. **Sector Classification Versioning:**
   - Store sector classification with `as_of_date` timestamp
   - Use sector assigned at `disclosure_date`, not current sector
   - Document GICS methodology version (2016 restructuring added Real Estate sector)

4. **Dividend Adjustment Strategy:**
   - For retrospective analysis: Use total return (price + dividends)
   - For "copyable" analysis: Use price return only (dividends not available to copiers until ex-date)
   - Label clearly: "Total Return (includes dividends)" vs "Price Return (replicable)"

5. **Walk-Forward Validation:**
   - Test scoring algorithms on held-out future periods
   - If performance degrades >50% in walk-forward test, look-ahead bias likely present
   - Example: Train scoring model on 2020-2022 trades, validate on 2023-2024 trades

**Warning signs:**
- Sharpe ratio > 1.5 in backtests
- Very smooth equity curve (straight line on log chart)
- Annualized return > 15% consistently
- Anomaly detector flags every trade in a specific date range (likely using future knowledge)
- Performance metrics change dramatically when analysis date shifts by 1-2 days

**Phase to address:**
Phase 1 (Data Foundation) - establish strict date discipline in schema before building any analytics.

---

### Pitfall 2: Survivorship Bias Inflating Returns

**Severity:** CRITICAL

**What goes wrong:**
Excluding delisted, bankrupt, or acquired companies from performance calculations. When calculating politician portfolio returns, only including stocks still trading today makes performance appear 15-40% better than reality. A portfolio that bought Enron, Lehman Brothers, and Theranos shows 0% return if those stocks are excluded (they no longer exist in price databases), but actually lost 100% of invested capital.

**Why it happens:**
Yahoo Finance and most free APIs only provide data for currently trading stocks. When a stock is delisted, its price history often disappears from the API after 6-12 months. SQLite `current_price` enrichment naturally skips delisted stocks (API returns no data). Developers assume "if no price data, skip the trade" rather than "if no price data, assume total loss."

**Real-world impact:**
Studies show survivorship bias causes 14 percentage point underestimation of drawdowns and inflates annual returns by an average of 8-14%. For congressional trades specifically, politicians often trade distressed stocks (Lordstown Motors, Luckin Coffee, FTX-related stocks) that later delist.

**Consequences:**
- Performance rankings unfairly favor politicians who traded surviving companies
- Anomaly detection misses genuine insider trading in bankrupt companies (trades appear neutral when excluded)
- Users copy "high performing" politicians whose actual returns include 3-5 total losses
- Legal exposure if promoting "Senator X achieved 30% returns" when actual is 8% including delisted stocks

**Prevention:**

1. **Delisting Detection Schema:**
   ```sql
   ALTER TABLE issuers ADD COLUMN delisting_date TEXT;
   ALTER TABLE issuers ADD COLUMN delisting_reason TEXT; -- 'bankruptcy', 'acquisition', 'voluntary', 'regulatory'
   ALTER TABLE issuers ADD COLUMN final_price REAL; -- Settlement price for acquisitions, 0.0 for bankruptcies
   ALTER TABLE issuers ADD COLUMN delisting_verified INTEGER DEFAULT 0;
   ```

2. **API Fallback Strategy:**
   - Primary: Yahoo Finance for active stocks
   - Fallback 1: Check SEC EDGAR for Form 25 (delisting notice) if Yahoo returns 404
   - Fallback 2: Manual entry for confirmed bankruptcies (maintain seed list of known delistings)
   - Sentinel: Store `final_price = 0.0` and `delisting_reason = 'bankruptcy'` for confirmed losses

3. **Acquisition Handling:**
   - Acquisitions are NOT losses - use acquisition price as final_price
   - Example: Twitter acquired at $54.20/share - use $54.20 as terminal value
   - Check SEC EDGAR Form 8-K for merger consideration (cash vs stock vs mixed)

4. **Portfolio Return Calculation:**
   ```sql
   -- CORRECT: Includes delisted stocks with final settlement value
   SELECT
       politician_id,
       SUM(estimated_value) as total_cost,
       SUM(CASE
           WHEN i.delisting_date IS NOT NULL THEN estimated_shares * i.final_price
           ELSE estimated_shares * i.current_price
       END) as current_value,
       (current_value - total_cost) / total_cost as total_return
   FROM trades t
   JOIN issuers i ON i.ticker = t.ticker
   GROUP BY politician_id;

   -- WRONG: Only includes surviving stocks
   SELECT politician_id, SUM(estimated_shares * i.current_price) FROM trades t
   JOIN issuers i ON i.ticker = t.ticker
   WHERE i.current_price IS NOT NULL; -- Silently excludes delistings
   ```

5. **Transparency Requirements:**
   - Display delisted stock count prominently: "Portfolio includes 3 delisted stocks"
   - Show survivorship-adjusted vs unadjusted returns side-by-side
   - Flag incomplete data: "Warning: 5 trades missing delisting data, returns may be overstated"

**Warning signs:**
- Portfolio returns consistently beat S&P 500 by >10%/year
- No negative returns in any calculated portfolio
- Missing trades when filtering by date range (deleted from DB because no price data)
- Return calculations change when date range shifts (survivorship filtering artifacts)
- Politicians who traded crypto/SPAC stocks show high returns (likely delisted stocks excluded)

**Phase to address:**
Phase 2 (Price Enrichment Enhancement) - add delisting detection before building performance metrics.

---

### Pitfall 3: Imprecise Share Estimation Variance

**Severity:** HIGH

**What goes wrong:**
STOCK Act requires disclosure of transaction value as a range ($1,000-$15,000, $15,001-$50,000, etc.), not exact amount. Estimating shares using midpoint of range creates 40-70% variance in calculated returns. Example: $15,001-$50,000 range at $100/share could be 150 shares (low end) or 500 shares (high end). If stock goes to $200, return is either $30,000 (100% gain) or $100,000 (100% gain on different base), but P&L ranges from +$15K to +$85K depending on assumption.

**Why it happens:**
Developers naturally use midpoint estimation (simple, deterministic). Users expect single number, not ranges. Display code struggles with "estimated shares: 150-500" formatting. Performance rankings require sortable scalar values, not intervals. Database schemas optimized for point values (REAL type), not ranges (requires custom type or two columns).

**Real-world accuracy:**
For $1K-$15K range, midpoint is $8K, but actual could be $1K (87.5% error) or $15K (87.5% error). For $15K-$50K range, midpoint is $32.5K, but actual could be $15K (53.8% error) or $50K (53.8% error). This variance compounds across portfolio with 50-200 trades.

**Consequences:**
- Politician ranked #1 with "30% return" might actually be #15 with "12% return" if trades were at low end of ranges
- Anomaly detection flags normal trades as outliers due to compounded estimation errors
- P&L displays suggest precision ($43,782.19 profit) when actual uncertainty is ±$20,000
- Users make investment decisions based on false precision

**Prevention:**

1. **Store Ranges, Not Midpoints:**
   ```sql
   ALTER TABLE trades ADD COLUMN amount_min REAL;
   ALTER TABLE trades ADD COLUMN amount_max REAL;
   ALTER TABLE trades ADD COLUMN estimated_shares_min REAL;
   ALTER TABLE trades ADD COLUMN estimated_shares_max REAL;
   ALTER TABLE trades ADD COLUMN estimated_shares_midpoint REAL; -- For sorting/filtering only

   -- Calculate returns as confidence interval
   SELECT
       politician_id,
       SUM(estimated_shares_min * current_price - amount_max) as pessimistic_pl,
       SUM(estimated_shares_midpoint * current_price - (amount_min + amount_max)/2) as estimated_pl,
       SUM(estimated_shares_max * current_price - amount_min) as optimistic_pl
   FROM trades;
   ```

2. **Display Best Practices:**
   - Show ranges prominently: "Est. Shares: 150-500 (midpoint: 325)"
   - P&L with error bars: "Profit: $25,000 (±$15,000)"
   - Rankings with confidence intervals: "#5-#12 depending on actual trade sizes"
   - Color-code precision: High precision (narrow range) in green, low precision (wide range) in yellow

3. **Minimum Sample Size Requirements:**
   - Individual trade analysis: Always show ranges, never hide uncertainty
   - Portfolio aggregation: Require 30+ trades before showing point estimates (Law of Large Numbers reduces variance)
   - Anomaly detection: Require 50+ trades before flagging outliers (avoid false positives from estimation variance)

4. **Conservative Estimation for Comparisons:**
   - When ranking politicians, use pessimistic_pl (assumes worst-case estimation)
   - When showing "best performers", use optimistic_pl with disclosure
   - Never mix estimation assumptions in same comparison (all pessimistic or all optimistic)

5. **Monte Carlo Validation:**
   - For each portfolio, run 1,000 simulations sampling randomly within ranges
   - Display distribution of possible returns
   - Flag portfolios where 5th-95th percentile spans >20 percentage points

**Warning signs:**
- P&L shown to penny precision ($43,782.19) with no disclaimer
- Rankings change dramatically when switching from midpoint to conservative estimation
- Single-trade "returns" calculated and displayed (statistical nonsense with range data)
- No confidence intervals despite data being inherently imprecise
- User complaints: "I copied Senator X's trades but got different returns"

**Phase to address:**
Phase 1 (Data Foundation) - schema must support ranges from day 1, before analytics layer built.

---

### Pitfall 4: Disclosure Delay Attribution Artifacts

**Severity:** HIGH

**What goes wrong:**
STOCK Act allows 30-45 day disclosure delay. A politician buying Moderna on January 15th discloses on February 14th. If Moderna announces vaccine breakthrough on February 1st (stock jumps 30%), analysis shows "politician bought 2 weeks before public knew" when actually they bought 2 weeks AFTER public knew but disclosed later. This creates false appearance of predictive ability.

**Why it happens:**
Database stores `trade_date` (execution date) but analysts compare to `disclosure_date` (public filing date). News events indexed by calendar date. Pattern detection looks for "trades preceding news" without checking if trade was disclosed before or after news. Confusion between "when did they trade" vs "when did we learn about the trade."

**Real-world examples:**
- Senator trades defense stocks on Feb 10, Ukraine invasion Feb 24, disclosure March 15. Appears predictive but news was public before disclosure.
- Representative sells bank stocks March 8, SVB collapses March 10, disclosure April 5. Analysis flags as suspicious insider trading, but trade was disclosed AFTER collapse (just late disclosure).

**Consequences:**
- False insider trading accusations based on disclosure timing artifacts
- Media coverage of "suspicious" trades that are actually benign late disclosures
- Anomaly detection overwhelmed with false positives from disclosure delay
- Legal risk if platform suggests illegal activity based on artifactual analysis

**Prevention:**

1. **Three-Date Schema:**
   ```sql
   ALTER TABLE trades ADD COLUMN trade_date TEXT; -- Actual execution date
   ALTER TABLE trades ADD COLUMN notification_date TEXT; -- When politician was notified (often same as trade_date)
   ALTER TABLE trades ADD COLUMN disclosure_date TEXT; -- When publicly filed
   ALTER TABLE trades ADD COLUMN disclosure_days_late INTEGER; -- Days between notification and disclosure

   -- Calculate using business days, not calendar days (weekends/holidays don't count toward 45-day limit)
   ```

2. **Analysis Rules:**
   - **For "predictive" analysis:** Compare `trade_date` to news date (did they trade before news?)
   - **For "copyable" analysis:** Compare `disclosure_date` to current date (can public act on this now?)
   - **For "suspicious" analysis:** Flag only if `trade_date` precedes news AND `disclosure_date` is timely (<45 days)
   - **Never flag:** Late disclosures (disclosure_days_late > 45) where news occurred during delay

3. **News Event Schema:**
   ```sql
   CREATE TABLE news_events (
       event_id TEXT PRIMARY KEY,
       ticker TEXT,
       event_date TEXT,
       event_type TEXT, -- 'earnings', 'merger', 'fda_approval', 'bankruptcy', 'lawsuit'
       event_description TEXT,
       price_impact REAL -- % change on event_date
   );

   -- Join with trades to find true predictive trades
   SELECT t.*, e.*
   FROM trades t
   JOIN news_events e ON e.ticker = t.ticker
   WHERE t.trade_date < e.event_date
       AND t.disclosure_date < e.event_date -- Disclosed before news, so no public info available
       AND e.price_impact > 0.10; -- Material 10%+ move
   ```

4. **Disclosure Delay Visualization:**
   - Timeline charts showing trade_date, news events, disclosure_date
   - Color coding: Green (timely disclosure), Yellow (disclosed but late), Red (not yet disclosed)
   - Warning banners: "This trade was disclosed 15 days after the news event"

5. **Late Disclosure Penalties:**
   - Exclude trades with disclosure_days_late > 45 from "performance" rankings (STOCK Act violation)
   - Flag with disclaimer: "This politician has 12 late disclosures (STOCK Act violations)"
   - Separate leaderboard: "Timely Disclosers" vs "All Trades"

**Warning signs:**
- High concentration of "predictive" trades in December-January (year-end disclosure rush creates lag artifacts)
- Anomaly scores correlate with disclosure delay rather than trade timing
- "Suspicious" trades cluster around politicians known for late disclosures
- Pattern detection finds "politician trades 30 days before news" (exactly the disclosure window)
- Media inquiries about "insider trading" for trades disclosed after public news

**Phase to address:**
Phase 3 (Performance Scoring) - implement three-date discipline before calculating predictive metrics.

---

### Pitfall 5: False Positive Clustering from Small Samples

**Severity:** HIGH

**What goes wrong:**
Congressional trades have small sample sizes (most politicians make 10-50 trades/year). Anomaly detection on small samples produces 60-80% false positive rates. Example: Politician makes 8 tech trades in a month. Clustering algorithm flags as "suspicious concentration" when actually it's random chance (8 trades is too small to detect statistical significance). Studies show anomaly detection with precision 0.20 or lower renders systems unusable due to false alarm fatigue.

**Why it happens:**
Standard anomaly detection algorithms (Isolation Forest, LOF, One-Class SVM) assume 1,000+ samples. With 10-50 trades, random variation dominates signal. Limited training data means models can't establish robust boundary between normal and abnormal. Rare events (trades are rare compared to non-trades) have insufficient positive samples. Developers test on synthetic data with large N, then deploy on real data with small N.

**Statistical reality:**
With 20 trades, probability of 5+ trades in same sector by chance alone is 35% (binomial distribution). With 50 trades, random clustering produces 3-5 "significant" patterns that are actually noise. Chi-squared test for independence requires expected frequency >5 in each cell, impossible with small N.

**Consequences:**
- 80% of "anomalies" are false positives (random variation)
- Engineers lose trust in monitoring system due to false alarm fatigue
- Users see "SUSPICIOUS CLUSTER: 4 defense trades in Feb" when it's random chance
- Genuine insider trading buried in noise (cry wolf problem)
- Computational waste: Running complex ML models on insufficient data

**Prevention:**

1. **Minimum Sample Size Requirements:**
   ```sql
   -- Require minimum trades before enabling anomaly detection
   SELECT politician_id, COUNT(*) as trade_count
   FROM trades
   GROUP BY politician_id
   HAVING trade_count >= 30; -- CLT threshold for parametric tests

   -- For clustering: Require minimum cluster size
   WITH sector_counts AS (
       SELECT politician_id, sector, COUNT(*) as n
       FROM trades t
       JOIN issuers i ON i.ticker = t.ticker
       GROUP BY politician_id, sector
   )
   SELECT * FROM sector_counts
   WHERE n >= 5; -- Don't flag clusters with <5 trades
   ```

2. **Non-Parametric Statistical Tests:**
   - Use Fisher's Exact Test (works with small samples) instead of Chi-Squared
   - Bootstrap confidence intervals (resampling with replacement)
   - Permutation tests (compare to randomized null distribution)
   - Bayesian approaches with informative priors

3. **Conservative Significance Thresholds:**
   - Standard: p < 0.05 (5% false positive rate)
   - Small samples: p < 0.01 (1% false positive rate) with Bonferroni correction for multiple comparisons
   - Example: Testing 10 sectors requires p < 0.001 threshold (0.01 / 10)

4. **Temporal Aggregation:**
   - Instead of monthly clustering, aggregate to quarterly or yearly
   - Increases sample size: 4 trades/month → 12 trades/quarter → 48 trades/year
   - Trade-off: Reduced temporal resolution but higher statistical power

5. **Display Confidence Intervals:**
   - "Defense trades: 8 observed, expected 3-7 (not statistically significant)"
   - "Tech concentration: 45%, baseline 30% ± 15% (within normal range)"
   - Show p-value prominently: "p=0.18 (not significant)"
   - Color coding: Gray (insufficient data), Yellow (marginal p<0.05), Red (strong p<0.01)

6. **Ensemble Methods with Voting:**
   - Run 3-5 different anomaly detectors
   - Require majority vote (3+/5) before flagging anomaly
   - Reduces false positives by ~70% (Fusing anomaly detection with false positive mitigation, 2024 study)

**Warning signs:**
- 50%+ of politicians flagged as "anomalous" (likely threshold too sensitive)
- Anomaly flags disappear when sample size increases (noise artifacts)
- Clusters found in random shuffled data (permutation test failure)
- No correlation between flagged "anomalies" and actual regulatory violations
- Flags trigger more often for politicians with fewer trades (small sample bias)

**Phase to address:**
Phase 4 (Anomaly Detection) - implement statistical rigor before exposing to users.

---

### Pitfall 6: Benchmark Mismatch and Dividend Adjustment

**Severity:** MEDIUM

**What goes wrong:**
Sector classification systems vary (GICS vs SIC vs NAICS), sector ETF methodologies differ, and dividend adjustment choices materially affect rankings. Example: Comparing politician tech trades to XLK (Technology Select Sector SPDR) vs QQQ (Nasdaq-100) vs VGT (Vanguard Information Technology) produces different results because they have different holdings (XLK excludes GOOGL/META, QQQ includes non-tech stocks, VGT uses different weighting). Dividend-adjusted returns favor dividend-paying sectors (financials, utilities) while price returns favor growth sectors (tech).

**Why it happens:**
No universal "correct" benchmark. GICS reclassified telecom/real estate in 2016 and 2018, creating historical inconsistencies. ETF providers use proprietary methodologies (market-cap weighted vs equal-weighted vs fundamental-weighted). Free APIs often provide only price return, not total return. Developers pick first ETF that matches sector name without checking methodology.

**Real-world variance:**
Tech sector ETFs: XLK (0.09% expense ratio, excludes GOOGL), VGT (0.10%, includes GOOGL), FTEC (0.084%, equal-weighted). Year-over-year returns vary 3-8% despite "same sector". Dividend yield: Utilities 3-4%, Tech 0.5-1% - materially different total returns.

**Consequences:**
- Politician ranked #3 with Tech benchmark XLK, but #15 with QQQ benchmark
- Performance comparisons unfair when mixing total return and price return
- Sector "outperformance" artifactually driven by benchmark choice, not skill
- Backtests show different historical performance depending on GICS version used

**Prevention:**

1. **Multiple Benchmark Display:**
   ```sql
   CREATE TABLE benchmarks (
       benchmark_id TEXT PRIMARY KEY,
       benchmark_name TEXT, -- 'S&P 500', 'XLK', 'QQQ'
       benchmark_type TEXT, -- 'broad_market', 'sector_etf', 'custom'
       sector TEXT, -- NULL for broad market
       methodology TEXT, -- 'market_cap_weighted', 'equal_weighted'
       includes_dividends INTEGER, -- 0 = price return, 1 = total return
       expense_ratio REAL
   );

   -- Show performance against multiple benchmarks
   SELECT
       politician_id,
       (portfolio_return - sp500_return) as sp500_alpha,
       (portfolio_return - sector_etf_return) as sector_alpha,
       (portfolio_return - custom_benchmark_return) as custom_alpha
   FROM performance_metrics;
   ```

2. **Sector Classification Versioning:**
   - Store GICS effective date: `gics_sector TEXT, gics_version TEXT, gics_effective_date TEXT`
   - Use sector classification as of trade date, not current
   - Document 2016 change (Telecom → Communication Services) and 2018 change (Real Estate carved out)

3. **Dividend Adjustment Consistency:**
   - Label all metrics: "Price Return (excludes dividends)" vs "Total Return (includes dividends)"
   - Default to total return for long-term holdings (>1 year)
   - Default to price return for short-term trades (<90 days) - dividends immaterial
   - Never mix: If portfolio uses total return, benchmark must use total return

4. **Benchmark Selection UI:**
   - Allow users to choose benchmark: "Compare to: [S&P 500] [Sector ETF] [Custom]"
   - Default: S&P 500 (most common, least ambiguous)
   - Show multiple: Table with columns for each benchmark (transparent trade-offs)
   - Disclaimer: "Rankings change based on benchmark selection"

5. **Tracking Error Analysis:**
   - Calculate tracking error (std dev of return differences) between politician and benchmark
   - High tracking error (>10%) indicates poor benchmark fit
   - Suggest alternative: "Tech trades have 15% tracking error vs XLK, try QQQ (8% tracking error)"

6. **Custom Sector-Neutral Benchmarks:**
   - Weight benchmarks by politician's sector exposure
   - Example: 40% tech, 30% healthcare, 30% financials → 40% XLK + 30% XLV + 30% XLF
   - Eliminates sector allocation as confounding variable

**Warning signs:**
- Rankings change significantly when benchmark changes from S&P 500 to sector ETF
- Politicians with high dividend stock concentration (utilities, REITs) rank poorly on price return, well on total return
- Historical performance analysis breaks in 2016-2018 (GICS restructuring artifacts)
- Benchmark tracking error >15% (benchmark doesn't match trading style)
- User confusion: "Why does Senator X show 20% return here but 12% on other site?" (benchmark difference)

**Phase to address:**
Phase 3 (Performance Scoring) - establish benchmark methodology before releasing rankings.

---

### Pitfall 7: Correlation Presented as Causation

**Severity:** MEDIUM

**What goes wrong:**
Committee membership correlates with sector trading (Senate Finance members trade financials, Armed Services trade defense), but this doesn't prove insider knowledge. Display like "Senator X sits on Banking Committee and outperformed XLF by 8%" implies causal insider trading when confounding variables explain it: (1) selection bias (senators interested in finance join Banking Committee and independently prefer financial stocks), (2) reverse causation (owning bank stocks motivated joining Banking Committee), (3) confounding variables (state economy - Texas senator on Energy Committee may trade oil stocks due to constituent interests, not insider info).

**Why it happens:**
Humans naturally infer causation from correlation. Committee jurisdiction maps neatly to GICS sectors, creating strong correlations (r=0.6-0.8). Media narratives reinforce "committee assignment = insider knowledge" framing. Statistical tests (chi-squared, t-tests) detect correlation but don't prove causation. Confounding variables (state economy, personal wealth, spouse's industry) are hard to measure and often unavailable.

**Real-world examples:**
Agriculture Committee members trade agriculture stocks 3x baseline rate. Could be: (1) insider knowledge of farm bill subsidies, (2) members from agricultural states have constituent interests, (3) members own farms/agricultural businesses before joining Congress. Can't distinguish without randomized experiment (impossible).

**Consequences:**
- Legal exposure: Implying insider trading without evidence
- False conclusions: "Committee oversight causes trading alpha" when it's selection bias
- Misleading users: "Copy Finance Committee trades for 8% alpha" fails when replicated
- Regulatory scrutiny: SEC/ethics investigations triggered by correlation-based accusations

**Prevention:**

1. **Confounding Variable Analysis:**
   ```sql
   CREATE TABLE confounders (
       politician_id TEXT,
       home_state TEXT,
       state_primary_industries TEXT, -- JSON array: ["oil", "agriculture", "tech"]
       prior_occupation TEXT,
       spouse_employer TEXT,
       personal_business_interests TEXT, -- JSON array of sectors
       wealth_quintile INTEGER -- 1-5, from financial disclosures
   );

   -- Multi-variate analysis: Does committee assignment predict trading after controlling for state industry?
   SELECT
       committee,
       AVG(sector_exposure) as avg_exposure,
       AVG(CASE WHEN state_industry = sector THEN 1 ELSE 0 END) as state_industry_match
   FROM politician_committees pc
   JOIN politician_trades pt ON pc.politician_id = pt.politician_id
   JOIN confounders c ON c.politician_id = pc.politician_id
   GROUP BY committee;
   ```

2. **Display Best Practices:**
   - Avoid: "Senator X's committee assignment explains 8% outperformance" (causal claim)
   - Better: "Senator X sits on Banking Committee and trades financials 3x baseline rate" (correlation)
   - Best: "Senator X trades financials 3x baseline. Possible explanations: committee membership, prior career in banking, home state economy" (multiple hypotheses)

3. **Statistical Disclaimers:**
   - Prominent: "Correlation does not imply causation"
   - Specific: "Committee membership correlates with sector trading (r=0.72, p<0.001), but confounding variables not controlled"
   - Quantified uncertainty: "Effect size: 8% ± 4% (95% CI), confounders may explain 3-6%"

4. **Propensity Score Matching:**
   - Match committee members to non-members with similar confounders
   - Example: Compare Armed Services Democrat from Texas to Judiciary Democrat from Texas (same state/party, different committee)
   - If effect persists after matching, stronger (but not conclusive) evidence

5. **Temporal Analysis:**
   - Does sector trading increase AFTER joining committee or was it pre-existing?
   - Example: Senator joins Finance Committee in 2020, trades financials heavily in 2018-2019 → pre-existing preference, not committee effect
   - Regression discontinuity: Sharp change at committee assignment date suggests causal effect

6. **Negative Controls:**
   - Test implausible relationships: "Does Judiciary Committee membership predict energy stock trading?" (should be no correlation)
   - If negative controls show correlation, confounding bias likely present

**Warning signs:**
- Headlines: "Armed Services Committee trades defense stocks before contracts announced" (causal framing without evidence)
- High correlation (r>0.7) presented without controlling for any confounders
- No discussion of alternative explanations (selection bias, reverse causation, state economy)
- Regression R² = 0.6 interpreted as "committee explains 60% of trading" (correlation ≠ explanatory power)
- User comments: "This proves insider trading!" (users inferring causation from correlation)

**Phase to address:**
Phase 5 (Committee-Sector Analysis) - implement confounding variable controls before analyzing committee effects.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use midpoint of dollar ranges for share estimation | Simple scalar value for calculations | 40-70% variance compounds across portfolio, false precision in rankings | Never for display; OK for internal sorting with disclaimer |
| Exclude delisted stocks from performance calculations | Simpler API integration (active stocks only) | 14% inflated returns, survivorship bias, unfair rankings | Never |
| Use current sector classification for historical trades | Single GICS query per ticker | Historical analysis broken at 2016/2018 GICS changes | OK for <2 year analysis window |
| Compare all trades to S&P 500 regardless of sector | Universally understood benchmark | Sector allocation confounds performance attribution | OK for broad overviews, not detailed analysis |
| Use disclosure_date for all timeline analysis | Simpler single-date logic | False "predictive" signals from disclosure lag artifacts | Never for anomaly detection |
| Calculate returns without dividend adjustment | Simpler price-only API calls | Understates returns for dividend stocks, unfair sector comparisons | OK if labeled "Price Return Only" prominently |
| Flag anomalies with p<0.05 threshold | Standard statistical practice | 60-80% false positives with small samples (N<30) | Never; require p<0.01 with Bonferroni correction |
| Store only current_price, not price_as_of_date | Saves schema complexity, one column | Look-ahead bias impossible to detect or fix | Never; timestamp is essential |
| Present committee-sector correlation as causal | Compelling narrative, simpler explanation | Legal exposure, misleading users, regulatory scrutiny | Never |

---

## Integration Gotchas

Common mistakes when connecting to external services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Yahoo Finance API | Assuming all tickers return current price (delisted stocks return 404) | Catch 404 errors, check SEC EDGAR Form 25 for delisting, store final_price = 0.0 for bankruptcies |
| Yahoo Finance historical prices | Using adjusted_close without understanding splits/dividends | Use adjusted_close for total return calculations, regular close for point-in-time price comparisons |
| Sector ETF data | Picking first ETF matching sector name (XLK vs VGT have different holdings) | Document ETF methodology (market-cap vs equal-weighted, inclusion rules), allow user selection |
| GICS sector classification | Assuming static sector assignments (Telecom → Communication Services in 2016) | Store gics_effective_date, use sector as of trade_date for historical analysis |
| OpenFEC employer data | Direct string matching ("Google" vs "Google LLC" vs "Alphabet Inc") | Use fuzzy matching (Levenshtein distance), manual seed data for top 200 employers, accept 10-15% unmapped |
| News event databases | Treating all "news" as material (minor earnings beats vs fraud announcements) | Filter by price_impact >10%, categorize event_type, require manual review for "insider trading" flags |

---

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Calculating portfolio returns on every page load | Query time <100ms for single politician | Pre-calculate and cache in positions table, refresh daily | 50+ politicians, 5,000+ trades (query time >2 seconds) |
| Running anomaly detection on every trade insert | Real-time anomaly flagging | Batch processing nightly or weekly, use incremental updates | 100+ trades/day (detection takes >10 minutes, blocks writes) |
| Joining trades + issuers + prices + news events in single query | Simpler application code | Denormalize frequently accessed fields (current_price, sector), use materialized views | 10,000+ trades (query time >5 seconds) |
| Storing price history in SQLite without indexes | Works for initial data load | Create indexes on (ticker, price_date), partition by year for tables >1M rows | 100+ tickers, 5+ years history (scan time >30 seconds) |
| Recalculating FIFO lots on every portfolio query | Accurate real-time positions | Cache FIFO state in positions table, recalculate only on new trades | 200+ trades per politician (calculation time >1 second) |
| Using window functions (RANK, ROW_NUMBER) without LIMIT | Elegant SQL for rankings | Limit to top 20-50 results before ranking, use pagination | 500+ politicians (ranking time >10 seconds) |

---

## Data Quality Mistakes

Domain-specific data integrity issues.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Not validating date order (trade_date <= disclosure_date <= current_date) | Impossible timelines in analysis, negative disclosure delays | CHECK constraint in schema, validation on insert |
| Allowing NULL sector classification | Sector analysis excludes unmapped trades, biased results | Require sector (use "Unknown" category), flag for manual review |
| Storing share estimates without source tracking | Can't debug wrong estimates, can't improve algorithm | Add estimate_method column ("midpoint", "conservative", "optimistic") |
| Not tracking delisting reason (bankruptcy vs acquisition) | Treating $54.20 acquisition as total loss, or $0 bankruptcy as acquisition | Enumerate delisting_reason, populate from SEC Form 8-K/Form 25 |
| Overwriting enriched prices on re-sync | Delisting data lost when re-scraping active stocks | Use INSERT OR IGNORE, or CASE WHEN price_enriched_at IS NULL in updates |
| Not storing confidence intervals with estimates | False precision, users overestimate accuracy | Store min/max/midpoint for all estimated fields |

---

## UX Pitfalls

Common user experience mistakes in this domain.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Showing returns to penny precision ($43,782.19) without disclaimers | Users assume exact accuracy, disappointed when copying trades fails | Show ranges: "$25K-$60K (estimated)" or "$43,000 ± $18,000" |
| Ranking politicians 1-50 without confidence intervals | #1 might actually be #15, unfair comparisons | Show tiers: "Top Tier (1-5)", "High Performers (6-15)", or overlapping ranges |
| Flagging "anomalies" without p-values or context | Users think every flag is genuine insider trading | Show: "8 defense trades, expected 3-7 (p=0.12, not significant)" in gray, not red |
| Using technical jargon (Sharpe ratio, tracking error) without explanation | Users misinterpret or ignore metrics | Provide glossary tooltips, use plain language alternatives |
| Displaying 30-day delayed trades as "real-time" | Users copy trades 30 days late, miss price moves | Prominent disclosure delay warning: "Disclosed 32 days after trade" |
| Comparing total return for one politician to price return for another | Unfair comparisons, user confusion | Consistent methodology, allow user to toggle "Price Return" vs "Total Return" |
| Not showing survivorship bias adjustment | Users see inflated returns, assume replicable | Side-by-side: "Unadjusted: 24% / Survivorship-Adjusted: 11%" |
| Presenting committee-sector correlation without caveats | Users assume causation, make investment decisions | Explicit: "Correlation does not prove insider knowledge. State economy and prior career also explain trading patterns." |

---

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Performance Returns:** Often missing survivorship bias adjustment — verify delisted stocks included with final_price
- [ ] **Anomaly Detection:** Often missing minimum sample size checks — verify N>=30 before flagging clusters
- [ ] **Benchmark Comparison:** Often missing dividend adjustment consistency — verify both portfolio and benchmark use total return or both use price return
- [ ] **Share Estimation:** Often missing confidence intervals — verify min/max stored, not just midpoint
- [ ] **Price Enrichment:** Often missing point-in-time validation — verify price_as_of_date <= disclosure_date (no look-ahead bias)
- [ ] **Sector Analysis:** Often missing GICS version tracking — verify sector as of trade_date, not current sector
- [ ] **Committee Analysis:** Often missing confounding variable controls — verify state industry, prior occupation considered
- [ ] **Timeline Displays:** Often missing three-date distinction — verify trade_date, notification_date, disclosure_date all shown
- [ ] **News Event Correlation:** Often missing materiality filter — verify price_impact >10% threshold before flagging
- [ ] **Ranking Tables:** Often missing statistical significance tests — verify p-values shown for "top performers"

---

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Look-ahead bias discovered in production | HIGH | 1. Immediately add disclaimer to all affected metrics. 2. Add price_as_of_date columns (schema migration). 3. Re-enrich all prices with timestamps. 4. Re-run all performance calculations. 5. Send correction notice to users who relied on biased data. |
| Survivorship bias inflating returns | HIGH | 1. Download SEC EDGAR delisting data (Form 25). 2. Scrape historical final prices for acquisitions. 3. Schema migration: add delisting_date, final_price, delisting_reason. 4. Re-calculate all portfolio returns. 5. Display correction: "Previous returns overstated due to survivorship bias." |
| Imprecise share estimates showing false precision | MEDIUM | 1. Schema migration: add amount_min, amount_max, estimated_shares_min, estimated_shares_max. 2. Update display layer to show ranges. 3. Re-calculate confidence intervals for all rankings. 4. No user notification needed if ranges shown going forward. |
| Disclosure delay artifacts creating false anomalies | MEDIUM | 1. Schema migration: add notification_date, disclosure_days_late. 2. Backfill disclosure_date from CapitolTrades scrape. 3. Re-run anomaly detection excluding late disclosures. 4. Remove false flags from DB. 5. Email users who were alerted to false anomalies with apology. |
| False positive clustering overwhelming users | LOW | 1. Increase significance threshold from p<0.05 to p<0.01. 2. Add minimum sample size check (N>=30). 3. Re-run detection on historical data. 4. Clear false positive flags. 5. No user notification if low user exposure. |
| Benchmark mismatch causing unfair rankings | MEDIUM | 1. Add benchmark_id column to performance_metrics table. 2. Calculate against multiple benchmarks (S&P 500, sector ETF, custom). 3. Update UI to show multi-benchmark comparison. 4. Notify users that rankings changed due to methodology update. |
| Correlation presented as causation | LOW | 1. Add disclaimer text to all committee-sector analysis. 2. Populate confounders table (state industry, prior occupation). 3. Re-run analysis with controls. 4. Update copy from causal language to correlational language. 5. Legal review of updated text. |

---

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Look-Ahead Bias | Phase 1: Data Foundation | Run walk-forward test: train on 2020-2022, validate on 2023-2024. Performance should not degrade >50%. Check Sharpe ratio <1.5. |
| Survivorship Bias | Phase 2: Price Enrichment Enhancement | Query for delisted stocks, verify final_price populated. Calculate returns with/without delisted stocks, verify difference 10-15%. |
| Imprecise Share Estimation | Phase 1: Data Foundation | Verify estimated_shares_min, estimated_shares_max columns exist. Display single trade, confirm range shown not point value. |
| Disclosure Delay Artifacts | Phase 3: Performance Scoring | Join trades to news_events, verify trade_date < event_date AND disclosure_date < event_date for "predictive" flags. Check no flags where disclosure_days_late >45 AND news between trade and disclosure. |
| False Positive Clustering | Phase 4: Anomaly Detection | Run permutation test: shuffle trades randomly, verify anomaly detection finds <5% false positives. Verify minimum N=30 before flagging. |
| Benchmark Mismatch | Phase 3: Performance Scoring | Compare politician performance to S&P 500, XLK, and custom sector-weighted benchmark. Verify rankings change <20% across benchmarks. |
| Correlation as Causation | Phase 5: Committee-Sector Analysis | Populate confounders table, run regression with committee + state_industry. Verify R² increases <0.1 when adding committee (confounders explain most variance). |

---

## Sources

**Survivorship Bias:**
- [Survivorship Bias in Trading: Why Most 'Proven' Strategies Are Misleading](https://enlightenedstocktrading.com/survivorship-bias-in-trading/)
- [Survivorship Bias in Backtesting Explained - LuxAlgo](https://www.luxalgo.com/blog/survivorship-bias-in-backtesting-explained/)
- [Survivorship Bias Market Data - Bookmap](https://bookmap.com/blog/survivorship-bias-in-market-data-what-traders-need-to-know)
- [Survivorship Bias In Trading - QuantifiedStrategies](https://www.quantifiedstrategies.com/survivorship-bias-in-backtesting/)

**Look-Ahead Bias:**
- [Understanding Look-Ahead Bias in Trading Strategies - MarketCalls](https://www.marketcalls.in/machine-learning/understanding-look-ahead-bias-and-how-to-avoid-it-in-trading-strategies.html)
- [Backtesting Traps: Common Errors to Avoid - LuxAlgo](https://www.luxalgo.com/blog/backtesting-traps-common-errors-to-avoid/)
- [Look-Ahead Bias In Backtests And How To Detect It - Michael Harris](https://mikeharrisny.medium.com/look-ahead-bias-in-backtests-and-how-to-detect-it-ad5e42d97879)
- [5 Critical Backtesting Mistakes - BacktestBase](https://www.backtestbase.com/education/5-critical-backtesting-mistakes)

**STOCK Act Disclosure:**
- [Congressional Stock Trading and the STOCK Act - Campaign Legal Center](https://campaignlegal.org/update/congressional-stock-trading-and-stock-act)
- [STOCK Act - Wikipedia](https://en.wikipedia.org/wiki/STOCK_Act)
- [Congressional Stock Trading Disclosure Rules - Nancy Pelosi Stock Tracker](https://nancypelosistocktracker.org/articles/disclosure-rules-explained)

**Benchmark and Dividend Adjustment:**
- [Tracking Difference vs. Tracking Error - Morningstar](https://www.morningstar.com/business/insights/blog/funds/etf-tracking-difference-error)
- [Tracking Error vs Tracking Difference Guide - Zerodha](https://www.zerodhafundhouse.com/blog/tracking-error-vs-tracking-difference-the-guide-every-index-fund-etf-investor-needs/)
- [Does Adding Dividend Stocks Improve Portfolio Performance? - Morningstar](https://www.morningstar.com/columns/rekenthaler-report/does-adding-dividend-stocks-improve-portfolio-performance)

**Anomaly Detection False Positives:**
- [Anomaly Detection: How to Tell Good Performance from Bad - Towards Data Science](https://towardsdatascience.com/anomaly-detection-how-to-tell-good-performance-from-bad-b57116d71a10/)
- [Anomaly detection optimization using big data and deep learning - Journal of Big Data](https://journalofbigdata.springeropen.com/articles/10.1186/s40537-020-00346-1)
- [Enhancing Anomaly Detection Models for Industrial Applications - MDPI](https://www.mdpi.com/2076-3417/13/23/12655)

**Sector Classification:**
- [GICS Codes Explained - SICCode.com](https://siccode.com/page/what-is-a-gics-code)
- [Global Industry Classification Standard - Wikipedia](https://en.wikipedia.org/wiki/Global_Industry_Classification_Standard)
- [GICS Sector and Industry Map - State Street](https://www.ssga.com/us/en/institutional/capabilities/equities/sector-investing/gics-sector-and-industry-map)

**Congressional Committee Jurisdiction:**
- [Committees of the United States Congress - GovTrack](https://www.govtrack.us/congress/committees/)
- [Congressional Committees - OpenSecrets](https://www.opensecrets.org/cong-cmtes/special)

**Correlation vs Causation:**
- [Don't Confuse Correlation and Causation - Morningstar](https://www.morningstar.com/markets/dont-confuse-correlation-causation)
- [Misleading correlations: how to avoid false conclusions - Statsig](https://www.statsig.com/perspectives/misleading-correlations-avoid-false-conclusions)
- [Correlation vs. Causation: Why It Matters for Investors - Practical AI Investor](https://practicalainvestor.substack.com/p/correlation-vs-causation-why-it-matters)
- [Correlation, Causation, and Confounding - Statology](https://www.statology.org/correlation-causation-confounding-decoding-hidden-relationships-data/)

**Fuzzy Matching Data Quality:**
- [Fuzzy Matching 101: Accurate Data Matching - Data Ladder](https://dataladder.com/fuzzy-matching-101/)
- [Fuzzy Matching In Financial Compliance - Financial Crime Academy](https://financialcrimeacademy.org/fuzzy-matching-in-financial-compliance/)

**Time Series Clustering:**
- [Time Series Clustering - Towards Data Science](https://towardsdatascience.com/time-series-clustering-deriving-trends-and-archetypes-from-sequential-data-bb87783312b4/)
- [An intelligent memetic approach to detect online fraud - Springer](https://link.springer.com/article/10.1007/s10660-025-10050-y)

**SQLite Performance:**
- [Window Functions - SQLite Official Documentation](https://sqlite.org/windowfunctions.html)
- [Window functions - High Performance SQLite](https://highperformancesqlite.com/watch/window-functions)
- [SQLite Window Functions Alternatives and Performance - RunEBook](https://runebook.dev/en/articles/sqlite/windowfunctions)

---

*Pitfalls research for: Congressional Trade Analytics & Scoring*
*Researched: 2026-02-14*
