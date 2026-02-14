# Milestones

## v1.1 Yahoo Finance Price Enrichment -- 2026-02-11

**Phases:** 1-6 | **Plans:** 7 | **Tasks:** 13 | **Tests added:** 72

**Accomplishments:**
1. Schema migration v2 with 5 price columns on trades and materialized positions table
2. YahooClient wrapper with adjclose price fetching, DashMap caching, and weekend/holiday fallback
3. Dollar range parsing and share estimation primitives with DB enrichment operations
4. Price enrichment pipeline with two-phase fetching (historical + current), rate limiting, and circuit breaker
5. FIFO portfolio calculator with lot-based cost basis tracking and realized P&L accumulation
6. Portfolio CLI command displaying per-politician positions with unrealized P&L across all 5 output formats

**Stats:**
- 40 commits, 42 files changed, +9,797 / -87 lines
- 16,776 total Rust LOC
- 366 workspace tests (all passing)
- Execution time: 0.52 hours (~31 min across 6 phases)
- Timeline: 2026-02-09 to 2026-02-11 (3 days)
- Git range: `c9746b1`..`f5d0a50`

**Archive:** `.planning/milestones/v1.1-ROADMAP.md`, `.planning/milestones/v1.1-REQUIREMENTS.md`

## v1.2 FEC Donation Integration -- 2026-02-14

**Phases:** 7-12 | **Plans:** 15 | **Tests:** 503 (all passing)

**Accomplishments:**
1. .env API key management, schema v3 migration, and congress-legislators FEC ID crosswalk
2. OpenFEC API client with candidate search, committee lookup, and Schedule A keyset pagination
3. Schema v4 with FEC committee storage and three-tier CommitteeResolver (DashMap/SQLite/API)
4. Concurrent donation sync pipeline with circuit breaker, cursor persistence, and resume support
5. Donations CLI with 4 display modes, 8 filters, and 5 output formats
6. Employer-to-issuer fuzzy matching, schema v5, map-employers CLI, and donor context in trades/portfolio

**Stats:**
- 56 commits, 75 files changed, +21,346 / -2,748 lines
- 23,537 total Rust LOC
- 503 workspace tests (all passing)
- Execution time: 2.18 hours across 15 plans
- Timeline: 2026-02-04 to 2026-02-14 (10 days)
- Git range: `598dbdd`..`f3b11a0`

**Archive:** `.planning/milestones/v1.2-ROADMAP.md`, `.planning/milestones/v1.2-REQUIREMENTS.md`
