# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-02-14)

**Core value:** Users can see what politicians are trading, what those positions are currently worth, whether they are making or losing money, and who is funding their campaigns.
**Current focus:** Phase 15 - Performance Scoring (v1.3 Analytics & Scoring)

## Current Position

Phase: 15 of 17 (Performance Scoring)
Plan: 01 of 3 (15-02 complete)
Status: In Progress
Last activity: 2026-02-15 - Completed 15-02 (Analytics trade query)

Progress: [█████████████░░░░░░░] 76%

## Shipped Milestones

- v1.1 Yahoo Finance Price Enrichment - 2026-02-11 (6 phases, 7 plans)
- v1.2 FEC Donation Integration - 2026-02-14 (6 phases, 15 plans)

## Performance Metrics

**Velocity:**
- Total plans completed: 27
- Average duration: 6.6 min
- Total execution time: 3.08 hours

**By Milestone:**

| Milestone | Phases | Plans | Total Time | Avg/Plan |
|-----------|--------|-------|------------|----------|
| v1.1 | 6 | 7 | 0.52 hours | 4.5 min |
| v1.2 | 6 | 15 | 2.18 hours | 8.7 min |
| v1.3 | 5 | 5 | 0.38 hours | 4.6 min |

**Recent Trend:**
- Last 5 plans: [6min, 5.6min, 4.7min, 2.1min, 4.4min]
- Trend: Stable (recent plans consistently under 5 minutes, excellent velocity)

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- v1.3: Separate BenchmarkEnrichmentRow from PriceEnrichmentRow (different data needs - gics_sector vs size_range fields)
- v1.3: Benchmark enrichment does not touch price_enriched_at (separate concerns for trade vs benchmark enrichment)
- v1.3: Compile-time YAML Inclusion for Sector Mappings (include_str! vs runtime file loading - ensures YAML validity at build time)
- v1.3: Case-insensitive Sector Validation (prevents YAML casing errors while enforcing official GICS capitalization)
- v1.3: SPDR Sector ETFs for GICS Benchmarks (11 sector SPDRs + SPY for market benchmark - high liquidity, direct GICS mapping)
- v1.3: Phase 3 uses separate semaphore from Phase 1 (Phase 1 permits may not be released if circuit breaker tripped)
- v1.3: BenchmarkPriceResult uses Vec<i64> (tx_ids) not Vec<usize> (Phase 3 uses separate query, indices would reference wrong vec)
- v1.3: query_trades_for_analytics does NOT filter benchmark_price IS NOT NULL (trades without benchmarks needed for FIFO matching)

### Pending Todos

None yet.

### Blockers/Concerns

None yet. v1.3 builds on existing v1.1 price enrichment infrastructure with no new external dependencies.

## Session Continuity

Last session: 2026-02-15
Stopped at: Completed 15-02-PLAN.md (Analytics trade query)
Next step: Continue Phase 15 (Plan 03 - Analytics CLI command)
