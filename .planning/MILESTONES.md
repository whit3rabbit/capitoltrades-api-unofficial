# Milestones

## v1.0 Detail Page Enrichment (Shipped: 2026-02-09)

**Phases completed:** 6 phases, 15 plans
**Tests:** 294 passing
**Lines changed:** +13,585 / -95 across 60 files
**Total Rust LOC:** 13,589
**Timeline:** 2026-02-07 to 2026-02-08 (1 day)
**Git range:** feat(01-01) to docs(phase-06)

**Key accomplishments:**
- Schema migration system with version-gated ALTER TABLE and sentinel-protected upserts that prevent data corruption on re-sync
- Trade detail extraction with full RSC payload parsing for asset types, sizing, filing details, and pricing
- Post-ingest enrichment pipeline with smart-skip, batch checkpointing, dry-run mode, and --db query path for all 5 output formats
- Politician committee extraction via listing page committee-filter iteration (48 codes) with real HTML fixture testing
- Issuer detail extraction with performance metrics and EOD price history persistence
- Bounded concurrent enrichment (Semaphore+JoinSet+mpsc), indicatif progress bars, and circuit breaker for failure recovery

**Tech debt carried forward:**
- TRADE-05/TRADE-06 committees and labels from trade RSC unconfirmed on live site
- Synthetic HTML fixtures may not match actual live RSC payload structure
- get_unenriched_politician_ids exists but is unused (committee enrichment runs as full refresh)

---

