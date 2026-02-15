# Roadmap: Capitol Traders

## Milestones

- ✅ **v1.1 Yahoo Finance Price Enrichment** - Phases 1-6 (shipped 2026-02-11)
- ✅ **v1.2 FEC Donation Integration** - Phases 7-12 (shipped 2026-02-14)
- 🚧 **v1.3 Analytics & Scoring** - Phases 13-17 (in progress)

## Phases

<details>
<summary>✅ v1.1 Yahoo Finance Price Enrichment (Phases 1-6) - SHIPPED 2026-02-11</summary>

- [x] Phase 1: Schema Migration & Data Model (1/1 plans) - completed 2026-02-10
- [x] Phase 2: Yahoo Finance Client Integration (1/1 plans) - completed 2026-02-10
- [x] Phase 3: Ticker Validation & Trade Value Estimation (1/1 plans) - completed 2026-02-11
- [x] Phase 4: Price Enrichment Pipeline (1/1 plans) - completed 2026-02-11
- [x] Phase 5: Portfolio Calculator (FIFO) (2/2 plans) - completed 2026-02-10
- [x] Phase 6: CLI Commands & Output (1/1 plans) - completed 2026-02-11

Full details: `.planning/milestones/v1.1-ROADMAP.md`

</details>

<details>
<summary>✅ v1.2 FEC Donation Integration (Phases 7-12) - SHIPPED 2026-02-14</summary>

- [x] Phase 7: Foundation & Environment Setup (2/2 plans) - completed 2026-02-12
- [x] Phase 8: OpenFEC API Client (2/2 plans) - completed 2026-02-12
- [x] Phase 9: Politician-to-Committee Mapping & Schema v4 (2/2 plans) - completed 2026-02-12
- [x] Phase 10: Donation Sync Pipeline (2/2 plans) - completed 2026-02-12
- [x] Phase 11: Donations CLI Command (2/2 plans) - completed 2026-02-13
- [x] Phase 12: Employer Correlation & Analysis (5/5 plans) - completed 2026-02-14

Full details: `.planning/milestones/v1.2-ROADMAP.md`

</details>

### 🚧 v1.3 Analytics & Scoring (In Progress)

**Milestone Goal:** Derive actionable insights from trade, price, portfolio, and donation data through performance scoring, historical anomaly detection, and sector/committee cross-reference analysis.

#### Phase 13: Data Foundation & Sector Classification -- COMPLETE 2026-02-15
**Goal**: Users can store benchmark prices and sector mappings for analytics
**Depends on**: Nothing (foundation)
**Requirements**: FOUND-01, FOUND-02, FOUND-04
**Verification**: 4/4 must-haves passed
**Plans**: 2/2 complete

Plans:
- [x] 13-01-PLAN.md -- Schema v6 migration, sector_benchmarks table, benchmark population, DB query helpers
- [x] 13-02-PLAN.md -- Sector mapping module, GICS YAML data file, issuer sector update operations

#### Phase 14: Benchmark Price Enrichment
**Goal**: Users can enrich trades with S&P 500 and sector ETF benchmark prices
**Depends on**: Phase 13
**Requirements**: FOUND-03
**Success Criteria** (what must be TRUE):
  1. User can run enrich-prices command which fetches benchmark prices in Phase 3
  2. User can see benchmark_price column populated for trades with valid trade dates
  3. User can see 12 benchmark tickers cached (SPY + 11 sector ETFs)
  4. Weekend/holiday dates fall back to previous trading day for benchmark prices
  5. Circuit breaker stops enrichment if 10+ consecutive benchmark price failures
**Plans**: 2 plans

Plans:
- [ ] 14-01-PLAN.md -- Schema v7 migration, BenchmarkEnrichmentRow, get_benchmark_unenriched_trades, update_benchmark_price
- [ ] 14-02-PLAN.md -- Phase 3 benchmark enrichment loop in enrich_prices.rs with sector-to-ETF mapping

#### Phase 15: Performance Scoring & Leaderboards
**Goal**: Users can see performance metrics and politician rankings
**Depends on**: Phase 14
**Requirements**: PERF-01, PERF-02, PERF-03, PERF-04, PERF-05, PERF-06, LEAD-01, LEAD-02, LEAD-03, LEAD-04
**Success Criteria** (what must be TRUE):
  1. User can see absolute return (%) for each closed trade with estimated P&L
  2. User can see win/loss rate per politician (% of trades with positive return)
  3. User can see S&P 500 alpha (trade return minus benchmark return over same period)
  4. User can see sector ETF relative return for trades in mapped sectors
  5. User can see annualized return for trades with known holding period
  6. User can view politician rankings sorted by performance metrics via new analytics CLI subcommand
  7. User can filter rankings by time period (YTD, 1Y, 2Y, all-time)
  8. User can filter rankings by minimum trade count to exclude low-activity politicians
  9. User can see percentile rank for each politician
**Plans**: TBD

Plans:
- [ ] 15-01: TBD
- [ ] 15-02: TBD
- [ ] 15-03: TBD

#### Phase 16: Conflict Detection
**Goal**: Users can identify committee-sector overlaps and donation-trade correlations
**Depends on**: Phase 15
**Requirements**: CONF-01, CONF-02, CONF-03, CONF-04
**Success Criteria** (what must be TRUE):
  1. User can see trades flagged as "committee-related" when trade sector matches committee jurisdiction
  2. User can see per-politician committee trading score (% of trades in committee-related sectors)
  3. User can see donation-trade correlation flags when donors' employers match traded issuers
  4. User can query conflict signals via analytics CLI with politician/committee filters
  5. User can see disclaimer "current committee only (may not reflect assignment at trade time)"
**Plans**: TBD

Plans:
- [ ] 16-01: TBD
- [ ] 16-02: TBD

#### Phase 17: Anomaly Detection & Output Integration
**Goal**: Users can detect unusual trading patterns and see analytics in all outputs
**Depends on**: Phase 16
**Requirements**: ANOM-01, ANOM-02, ANOM-03, ANOM-04, ANOM-05, OUTP-01, OUTP-02, OUTP-03, OUTP-04
**Success Criteria** (what must be TRUE):
  1. User can see pre-move trade flags (trades followed by >10% price change within 30 days)
  2. User can see unusual volume flags (trade frequency exceeding politician's historical baseline)
  3. User can see sector concentration score (HHI) per politician
  4. User can see composite anomaly score combining timing, volume, and concentration signals
  5. User can filter anomaly results by minimum confidence threshold
  6. User can see performance summary (return, alpha) in existing trades output
  7. User can see conflict flags in existing portfolio output
  8. User can see analytics scores in existing politicians output
  9. All new analytics output supports 5 formats (table, JSON, CSV, markdown, XML)
**Plans**: TBD

Plans:
- [ ] 17-01: TBD
- [ ] 17-02: TBD
- [ ] 17-03: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 13 -> 14 -> 15 -> 16 -> 17

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Schema Migration & Data Model | v1.1 | 1/1 | Complete | 2026-02-10 |
| 2. Yahoo Finance Client Integration | v1.1 | 1/1 | Complete | 2026-02-10 |
| 3. Ticker Validation & Trade Value Estimation | v1.1 | 1/1 | Complete | 2026-02-11 |
| 4. Price Enrichment Pipeline | v1.1 | 1/1 | Complete | 2026-02-11 |
| 5. Portfolio Calculator (FIFO) | v1.1 | 2/2 | Complete | 2026-02-10 |
| 6. CLI Commands & Output | v1.1 | 1/1 | Complete | 2026-02-11 |
| 7. Foundation & Environment Setup | v1.2 | 2/2 | Complete | 2026-02-12 |
| 8. OpenFEC API Client | v1.2 | 2/2 | Complete | 2026-02-12 |
| 9. Politician-to-Committee Mapping & Schema v4 | v1.2 | 2/2 | Complete | 2026-02-12 |
| 10. Donation Sync Pipeline | v1.2 | 2/2 | Complete | 2026-02-12 |
| 11. Donations CLI Command | v1.2 | 2/2 | Complete | 2026-02-13 |
| 12. Employer Correlation & Analysis | v1.2 | 5/5 | Complete | 2026-02-14 |
| 13. Data Foundation | v1.3 | 2/2 | Complete | 2026-02-15 |
| 14. Benchmark Enrichment | v1.3 | 0/2 | Not started | - |
| 15. Performance Scoring | v1.3 | 0/0 | Not started | - |
| 16. Conflict Detection | v1.3 | 0/0 | Not started | - |
| 17. Anomaly Detection | v1.3 | 0/0 | Not started | - |
