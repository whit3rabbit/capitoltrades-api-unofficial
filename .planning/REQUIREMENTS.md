# Requirements: Capitol Traders v1.3

**Defined:** 2026-02-14
**Core Value:** Users can see what politicians are trading, what those positions are currently worth, whether they are making or losing money, and who is funding their campaigns -- now with performance analytics, anomaly detection, and conflict-of-interest signals.

## v1.3 Requirements

Requirements for Analytics & Scoring milestone. Each maps to roadmap phases.

### Data Foundation

- [ ] **FOUND-01**: User can run schema v6 migration adding benchmark and analytics tables
- [ ] **FOUND-02**: User can store benchmark prices (S&P 500 + 11 sector ETFs) in SQLite
- [ ] **FOUND-03**: User can enrich benchmark prices via Yahoo Finance during enrich-prices run
- [ ] **FOUND-04**: User can map issuers to GICS sectors via static YAML classification

### Performance Scoring

- [ ] **PERF-01**: User can see absolute return (%) for each trade with estimated P&L
- [ ] **PERF-02**: User can see win/loss rate per politician (% of trades with positive return)
- [ ] **PERF-03**: User can see S&P 500 alpha (trade return minus benchmark return over same period)
- [ ] **PERF-04**: User can see sector ETF relative return for trades in mapped sectors
- [ ] **PERF-05**: User can see annualized return for trades with known holding period
- [ ] **PERF-06**: User can see holding period analysis (average days held per politician)

### Leaderboards

- [ ] **LEAD-01**: User can view politician rankings sorted by performance metrics via new CLI subcommand
- [ ] **LEAD-02**: User can filter rankings by time period (YTD, 1Y, 2Y, all-time)
- [ ] **LEAD-03**: User can filter rankings by minimum trade count to exclude low-activity politicians
- [ ] **LEAD-04**: User can see percentile rank for each politician

### Conflict Detection

- [ ] **CONF-01**: User can see trades flagged as "committee-related" when trade sector matches committee jurisdiction
- [ ] **CONF-02**: User can see per-politician committee trading score (% of trades in committee-related sectors)
- [ ] **CONF-03**: User can see donation-trade correlation flags when donors' employers match traded issuers
- [ ] **CONF-04**: User can query conflict signals via new CLI subcommand with politician/committee filters

### Anomaly Detection

- [ ] **ANOM-01**: User can see pre-move trade flags (trades followed by >10% price change within 30 days)
- [ ] **ANOM-02**: User can see unusual volume flags (trade frequency exceeding politician's historical baseline)
- [ ] **ANOM-03**: User can see sector concentration score (HHI) per politician
- [ ] **ANOM-04**: User can see composite anomaly score combining timing, volume, and concentration signals
- [ ] **ANOM-05**: User can filter anomaly results by minimum confidence threshold

### Output Integration

- [ ] **OUTP-01**: User can see performance summary (return, alpha) in existing trades output
- [ ] **OUTP-02**: User can see conflict flags in existing portfolio output
- [ ] **OUTP-03**: User can see analytics scores in existing politicians output
- [ ] **OUTP-04**: All new analytics output supports 5 formats (table, JSON, CSV, markdown, XML)

## Future Requirements

### Advanced Analytics

- **ADV-01**: Risk-adjusted returns (Sharpe ratio with configurable risk-free rate)
- **ADV-02**: Sector rotation analysis (how politician sector allocation changes over time)
- **ADV-03**: Copycat portfolio simulation (what if you followed a politician's trades?)
- **ADV-04**: Historical backtest with walk-forward validation
- **ADV-05**: Survivorship bias correction (track delisted stocks with final prices)

### External Data

- **EXT-01**: SEC EDGAR Form 25 integration for delisting events
- **EXT-02**: News event correlation (material corporate events near trade dates)
- **EXT-03**: Historical committee assignment tracking (point-in-time membership)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Real-time alerting | 30-day disclosure delay makes real-time detection impractical |
| Machine learning models | Over-engineering for CLI; simple statistics sufficient for N<50 samples |
| Causal inference claims | Cannot prove insider trading from public data; present as correlation only |
| Web dashboard | CLI-first; defer to separate milestone |
| Option trade analytics | Option valuation requires strike/expiry data not available from CapitolTrades |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | -- | Pending |
| FOUND-02 | -- | Pending |
| FOUND-03 | -- | Pending |
| FOUND-04 | -- | Pending |
| PERF-01 | -- | Pending |
| PERF-02 | -- | Pending |
| PERF-03 | -- | Pending |
| PERF-04 | -- | Pending |
| PERF-05 | -- | Pending |
| PERF-06 | -- | Pending |
| LEAD-01 | -- | Pending |
| LEAD-02 | -- | Pending |
| LEAD-03 | -- | Pending |
| LEAD-04 | -- | Pending |
| CONF-01 | -- | Pending |
| CONF-02 | -- | Pending |
| CONF-03 | -- | Pending |
| CONF-04 | -- | Pending |
| ANOM-01 | -- | Pending |
| ANOM-02 | -- | Pending |
| ANOM-03 | -- | Pending |
| ANOM-04 | -- | Pending |
| ANOM-05 | -- | Pending |
| OUTP-01 | -- | Pending |
| OUTP-02 | -- | Pending |
| OUTP-03 | -- | Pending |
| OUTP-04 | -- | Pending |

**Coverage:**
- v1.3 requirements: 24 total
- Mapped to phases: 0
- Unmapped: 24

---
*Requirements defined: 2026-02-14*
*Last updated: 2026-02-14 after initial definition*
