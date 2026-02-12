# Project Research Summary

**Project:** Capitol Traders v1.2 -- OpenFEC Donation Integration
**Domain:** FEC Campaign Finance Data Integration for Congressional Trade Analysis
**Researched:** 2026-02-11
**Confidence:** HIGH (stack, architecture), MEDIUM (features, employer correlation)

## Executive Summary

Integrating OpenFEC campaign donation data into Capitol Traders is architecturally straightforward but operationally complex. The API itself is a standard JSON REST endpoint that slots into the existing reqwest 0.12 + tokio 1.x + SQLite stack with only one new dependency (dotenvy for .env loading). No Rust OpenFEC client library exists, so we build a thin wrapper following the same patterns as the existing CapitolTrades scraper and Yahoo Finance client. The real complexity lies not in HTTP plumbing but in the FEC data model: donations are committee-centric, not politician-centric, requiring a multi-step resolution chain (politician name -> candidate ID -> committee IDs -> Schedule A contributions) before any donation data can be fetched.

The recommended approach is to use the unitedstates/congress-legislators public domain dataset as an authoritative crosswalk for politician-to-FEC ID mapping, avoiding hundreds of expensive API search calls. Donations sync on-demand per politician (not bulk for all 535 members), using keyset pagination for Schedule A's 67M+ record dataset and incremental date-based checkpointing for resume after interruption. The sync pipeline reuses the existing Semaphore+JoinSet+mpsc concurrent enrichment pattern with reduced concurrency (3 vs 5) to respect the 1,000 calls/hour rate limit. Schema v3 adds three tables (donations, fec_mappings, donation_sync_meta) with no changes to existing trade/politician tables.

The key risks are: (1) rate limiting makes full-congress sync a multi-day operation (mitigated by on-demand per-politician approach), (2) employer name normalization is required for the differentiator correlation features but employer data is free-text garbage (mitigated by manual seed data for top 200 employers covering ~50% of donation volume), and (3) FEC filing lag means recent donations (last 30 days) may be incomplete (mitigated by displaying coverage dates and re-syncing a 90-day window periodically). The table-stakes features (sync donations, query top donors, filter by cycle/amount) are well-understood. The differentiator features (employer-to-issuer correlation, sector analysis, timing correlation) carry higher implementation risk and should be phased after core donation sync is proven.

## Key Findings

### Recommended Stack

Only one new dependency required. The existing workspace stack covers all other needs.

**Core technologies:**
- **dotenvy 0.15** -- .env file loading for API key storage. Maintained fork of unmaintained dotenv crate, addresses RUSTSEC-2021-0141.
- **reqwest 0.12** (existing) -- HTTP client for OpenFEC API. Already in workspace deps, no upgrade needed (0.13 changes default TLS which we already override with rustls-tls).
- **tokio 1.x** (existing) -- Async runtime. Already workspace-wide.
- **rusqlite 0.31** (existing) -- SQLite for donation persistence. Upgrade to 0.38 deferred (not required).
- **strsim** (Phase 2+) -- Jaro-Winkler fuzzy string matching for employer-to-issuer correlation. Pure Rust, lightweight, no ML dependencies.

**Not needed:** reqwest-middleware, governor, config-rs, figment, envy, OpenAPI codegen, new async runtime, new JSON library. See STACK.md for full alternatives analysis.

### Expected Features

**Must have (table stakes):**
- Schedule A individual contributions sync to SQLite
- Politician-to-committee ID mapping (cached in DB)
- Top donors by amount per politician
- Total donations by election cycle (2-year periods)
- Date range filtering (--since/--until)
- Employer and occupation display
- Committee type classification (campaign vs leadership PAC)
- Sync resumability with keyset pagination checkpoints
- Data staleness indicators (synced_at timestamp, coverage dates)

**Should have (differentiators):**
- Employer-to-issuer correlation with confidence scoring
- Sector-based donation analysis (manual seed for top 200 employers)
- Individual donor lookup (reverse search by donor name)
- Geographic donor concentration (state breakdown)
- Max-out donor identification ($3,300 limit for 2025-2026 cycle)
- Committee-level donation breakdown (campaign vs leadership PAC split)

**Defer (v2+):**
- Donation-to-trade timing correlation (requires employer-to-issuer matching as prerequisite)
- First-time vs repeat donor analysis
- Schedule B expenditure tracking (different schema, different domain)
- Full campaign finance platform features (link to OpenSecrets instead)
- Automatic employer name resolution without user confirmation
- Real-time donation alerts (FEC data is daily at best)
- Multi-candidate aggregation reports

### Architecture Approach

The integration follows the established Capitol Traders pattern: thin HTTP client wrapper (openfec/ module in capitoltraders_lib), SQLite for persistence (Schema v3 with donations/fec_mappings/donation_sync_meta tables), and Semaphore+JoinSet+mpsc for concurrent enrichment. Two new CLI commands: `sync-donations` (write path, requires API key) and `donations` (read path, SQLite-only). The .env file loads once at startup via dotenvy; the FEC client initializes lazily only when donation commands are invoked.

**Major components:**
1. **openfec/ module** (client.rs, types.rs, error.rs) -- OpenFEC API wrapper with candidate search, committee lookup, Schedule A keyset-paginated fetch
2. **Schema v3 tables** (donations, fec_mappings, donation_sync_meta) -- Persistent storage with deduplication by FEC sub_id and incremental sync state tracking
3. **sync-donations command** -- Maps politicians to committees, fetches Schedule A data, upserts to SQLite with circuit breaker on 429 errors
4. **donations command** -- Queries/aggregates donation data with group-by (contributor, employer, state), outputs in all 5 formats
5. **Three-tier cache** -- DashMap (in-memory) -> fec_mappings (SQLite) -> OpenFEC API (source of truth) for committee resolution

### Critical Pitfalls

1. **Name Mapping Problem** -- FEC is committee-centric, not politician-centric. Multi-step resolution (name -> candidate_id -> committee_ids) burns 3+ API calls per politician. **Mitigation:** Use unitedstates/congress-legislators YAML dataset as authoritative FEC ID crosswalk, reducing mapping to a local lookup. Saves 1,000+ API calls for full Congress.

2. **Rate Limiting Bottleneck** -- 1,000 calls/hour hard cap, no paid tier available. A single high-profile politician (50K donations) consumes 500 calls (half the hourly budget). **Mitigation:** On-demand per-politician sync (not bulk), circuit breaker with exponential backoff on 429, Semaphore concurrency = 3, quota budgeting (reserve 20% for metadata lookups).

3. **Employer Normalization Hell** -- Free-text employer field produces dozens of variants per company ("Google" vs "Google Inc" vs "Google LLC" vs "Alphabet Inc"). Direct matching yields <50% hit rate. **Mitigation:** Two-tier matching (exact after normalization, then Jaro-Winkler fuzzy at 0.85+ threshold). Manual seed data for top 200 employers. Never auto-link without user confirmation.

4. **Keyset Pagination Misimplementation** -- Schedule A uses cursor-based pagination (last_index + last_contribution_receipt_date), not page numbers. Naive page-number iteration duplicates or skips records on a 67M-record dataset. **Mitigation:** Implement keyset pagination from day 1, store cursor state in donation_sync_meta for resume.

5. **Committee Multiplicity** -- Politicians have 2-5 committees (campaign, leadership PAC, joint fundraising). Querying only the principal campaign committee misses 30-50% of total fundraising. **Mitigation:** Query all authorized committees, display type breakdown, provide --include-leadership-pacs opt-in flag.

## Implications for Roadmap

Based on combined research, the following 6-phase structure covers foundation through advanced correlation features. Phases start at 7 (v1.1 ended at phase 6). Dependencies flow strictly downward: each phase builds on the previous.

### Phase 7: Foundation and Environment Setup
**Rationale:** Every subsequent phase depends on .env loading, .gitignore hygiene, and the congress-legislators ID mapping dataset. Do this first because it is zero-risk infrastructure that unblocks everything.
**Delivers:** dotenvy integration, .env/.gitignore setup, .env.example template, congress-legislators YAML download + parsing, politician_fec_mapping SQLite table, README documentation for API key registration.
**Addresses:** Table stakes (API key management), Pitfall #1 (name mapping problem).
**Avoids:** Pitfall #1 entirely by using local ID crosswalk instead of multi-step API search.

### Phase 8: OpenFEC API Client
**Rationale:** The HTTP client must exist before any data can flow. This phase is pure library code with no CLI integration, making it independently testable with wiremock fixtures.
**Delivers:** openfec/ module (client.rs, types.rs, error.rs) in capitoltraders_lib. Candidate search, committee lookup, Schedule A keyset-paginated fetch. OpenFecError enum with 429/403 handling. Deserialization unit tests with JSON fixtures. Wiremock integration tests (success, rate limit, invalid key).
**Addresses:** Table stakes (API integration foundation).
**Avoids:** Pitfall #4 (keyset pagination) by implementing correctly from day 1, Pitfall #3 (rate limiting) with circuit breaker + status code mapping.

### Phase 9: Politician-to-Committee Mapping and Schema v3
**Rationale:** Committee mapping is the bridge between Capitol Traders' politician-centric model and FEC's committee-centric model. Schema v3 (fec_mappings, donations, donation_sync_meta tables) must exist before sync can write data.
**Delivers:** Schema v3 migration. Three-tier cache for committee resolution (DashMap -> SQLite -> API). politician_fec_mapping -> OpenFEC committee lookup pipeline. Committee type classification (campaign vs leadership PAC vs joint fundraising).
**Addresses:** Table stakes (politician-to-committee mapping, committee type classification), Pitfall #5 (committee multiplicity).
**Avoids:** Pitfall #5 by querying all authorized committees and classifying by type.

### Phase 10: Donation Sync Pipeline
**Rationale:** With the client built and schema in place, the sync pipeline is the core data ingestion path. Reuses the proven Semaphore+JoinSet+mpsc pattern from price enrichment.
**Delivers:** sync-donations CLI command. Concurrent Schedule A fetch per committee. Keyset pagination loop with cursor state persistence. Incremental sync via min_date checkpointing. Deduplication by FEC sub_id. Circuit breaker (5 consecutive 429 = stop). Progress reporting (N donations synced for politician X). Batch size limiting flag.
**Addresses:** Table stakes (Schedule A sync, sync resumability, data staleness indicators).
**Avoids:** Pitfall #2 (data volume) via on-demand per-politician sync, Pitfall #3 (rate limiting) via circuit breaker + concurrency = 3, Pitfall #4 (keyset pagination) via cursor state persistence.

### Phase 11: Donations CLI Command
**Rationale:** With data in SQLite, the query/display command provides the user-facing value. This is the read path, fully SQLite-based (no API calls needed).
**Delivers:** donations CLI subcommand with --politician, --cycle, --min-amount, --employer, --top N, --group-by (contributor/employer/state) flags. All 5 output formats (table, JSON, CSV, markdown, XML). Top donors aggregation. Date range filtering. Employer/occupation display.
**Addresses:** Table stakes (top donors, cycle totals, date filtering, employer display), differentiators (geographic concentration, max-out donor identification, committee-level breakdown).
**Avoids:** Feature bloat by keeping query path separate from sync path.

### Phase 12: Employer Correlation and Analysis
**Rationale:** This is the differentiator phase that links donation data to trade data. Deferred after core sync/query because it requires both donation data and trade data to exist, and the fuzzy matching infrastructure is higher risk.
**Delivers:** Employer normalization module (lowercase, strip suffixes, collapse whitespace). Manual seed data for top 200 employers with ticker/sector mappings. Jaro-Winkler fuzzy matching via strsim crate. Confidence-scored employer-to-issuer suggestions (not auto-linked). Sector-based donation analysis. Export-review-import workflow for unmatched employers. Optional --show-donor-context flag on trades command.
**Addresses:** Differentiators (employer-to-issuer correlation, sector analysis, individual donor lookup).
**Avoids:** Pitfall #4 (employer normalization) via tiered matching + manual seed data + user confirmation requirement.

### Phase Ordering Rationale

- **Phase 7 before Phase 8:** Environment setup (.env, .gitignore, congress-legislators mapping) is prerequisite for any API interaction and prevents the name-mapping pitfall from consuming API budget.
- **Phase 8 before Phase 9:** The API client must exist to resolve committees; schema depends on understanding the data shapes.
- **Phase 9 before Phase 10:** Schema and committee mapping must exist before sync can write donation records.
- **Phase 10 before Phase 11:** Data must be synced before it can be queried/displayed.
- **Phase 11 before Phase 12:** Core donation display proves the data pipeline works before adding complex correlation logic on top.
- **Phase 12 last:** Employer correlation is highest-risk, highest-reward. Deferring it means core donation features ship even if fuzzy matching proves harder than expected.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 9 (Committee Mapping):** The CapitolTrades politician_id format needs investigation to determine if it maps to Bioguide IDs or is proprietary. This determines whether the congress-legislators crosswalk is a direct lookup or requires fuzzy name matching. Validate with 5-10 real politician IDs.
- **Phase 12 (Employer Correlation):** Employer normalization and fuzzy matching thresholds need empirical tuning with real FEC data. The strsim crate is well-documented but threshold selection (0.85 vs 0.90) requires testing against actual employer name distributions.

Phases with standard patterns (skip research-phase):
- **Phase 7 (Foundation):** dotenvy is well-documented, .env loading is standard, congress-legislators dataset format is known.
- **Phase 8 (API Client):** OpenFEC API is thoroughly documented. JSON deserialization + wiremock testing follows the exact same pattern as existing CapitolTrades API and Yahoo Finance clients.
- **Phase 10 (Sync Pipeline):** Directly reuses Semaphore+JoinSet+mpsc pattern from price enrichment. Only novelty is keyset pagination, which is well-documented.
- **Phase 11 (Donations CLI):** Mirrors existing trades/politicians command structure exactly.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Only dotenvy is new, all other deps already in workspace. OpenFEC API verified via official docs. No Rust client exists (confirmed via crates.io/GitHub search). |
| Features | MEDIUM | Table stakes features well-defined. Differentiator features (employer correlation, sector analysis) depend on data quality assumptions not yet validated with real FEC data. |
| Architecture | HIGH | Follows established codebase patterns exactly (client wrapper, enrichment pipeline, schema migration, CLI command structure). No architectural novelty. |
| Pitfalls | HIGH | All 7 pitfalls verified via official FEC/OpenFEC documentation and GitHub issues. congress-legislators dataset verified as actively maintained. Rate limits confirmed. |

**Overall confidence:** HIGH for core donation sync (phases 7-11), MEDIUM for employer correlation features (phase 12).

### Gaps to Address

- **CapitolTrades politician_id to Bioguide/FEC ID mapping:** The exact format and mapping strategy depends on whether Capitol Trades IDs are Bioguide IDs, FEC IDs, or proprietary. Validate during Phase 9 planning with 5-10 real politician records.

- **OpenFEC rate limit: 100 vs 1,000 calls/hour:** STACK.md says 1,000 calls/hour, FEATURES.md says 100 calls/hour in one place. The official api.data.gov documentation states 1,000/hour for registered API keys and a lower rate for DEMO_KEY. Verify the actual rate limit empirically during Phase 8 development by inspecting X-RateLimit-Limit response headers.

- **Schedule A field naming (abbreviated vs full):** STACK.md uses abbreviated FEC field names (contbr_nm, contb_receipt_amt) while ARCHITECTURE.md uses full names (contributor_name, contribution_receipt_amount). OpenFEC API documentation should be consulted during Phase 8 to confirm the actual JSON field names returned by the API. Use the exact names from the API response.

- **FEC data coverage window:** FEATURES.md states "last 4 years only via API" for Schedule A itemized contributions. This needs verification. If true, historical cycle data (pre-2022) requires bulk CSV downloads rather than API pagination.

- **Employer-to-issuer match rate baseline:** No empirical data on what percentage of donation employers can be matched to stock issuers. The 40-50% estimate for top 200 employers is reasonable but unverified. Run a prototype match against one politician's real donation data during Phase 12 planning.

## Sources

### Primary (HIGH confidence)
- [OpenFEC API Documentation](https://api.open.fec.gov/developers/) -- Endpoints, pagination, authentication, rate limits
- [GitHub - fecgov/openFEC](https://github.com/fecgov/openFEC) -- Source code, issues, keyset pagination design
- [Schedule A Column Documentation](https://github.com/fecgov/openFEC/wiki/Schedule-A-column-documentation) -- Field definitions for contribution records
- [GitHub - unitedstates/congress-legislators](https://github.com/unitedstates/congress-legislators) -- Authoritative politician-to-FEC ID crosswalk dataset
- [dotenvy 0.15.7 Documentation](https://docs.rs/dotenvy/latest/dotenvy/) -- .env file loading API reference
- [FEC Contribution Limits 2025-2026](https://www.fec.gov/help-candidates-and-committees/candidate-taking-receipts/contribution-limits/) -- Legal context for max-out donor analysis

### Secondary (MEDIUM confidence)
- [18F: OpenFEC API Update - 67 Million Records](https://18f.gsa.gov/2015/07/15/openfec-api-update/) -- Keyset pagination rationale and design history
- [FEC Candidate Master File Description](https://www.fec.gov/campaign-finance-data/candidate-master-file-description/) -- Candidate ID format (H/S/P prefix + state + district)
- [FEC Leadership PACs](https://www.fec.gov/help-candidates-and-committees/registering-pac/types-nonconnected-pacs/leadership-pacs/) -- Committee multiplicity context
- [api.data.gov Rate Limiting](https://api.data.gov/docs/rate-limits/) -- Rate limit enforcement details
- [FEC Bulk Data Downloads](https://www.fec.gov/data/browse-data/?tab=bulk-data) -- Fallback for historical data outside API window
- [OpenFEC Postman Collection](https://www.postman.com/api-evangelist/federal-election-commission-fec/documentation/19lr6vr/openfec) -- API usage examples

### Tertiary (LOW confidence)
- [FEC Name Standardization: What's in a name?](https://www.fec.gov/updates/whats-in-a-name/) -- Employer normalization context (FEC acknowledges the problem)
- [Employer name standardization - RecordLinker](https://recordlinker.com/name-normalization-matching/) -- Fuzzy matching approach validation
- [FEC Standardizer - Donor Clustering](https://github.com/cjdd3b/fec-standardizer/wiki/Defining-donor-clusters) -- Random Forest approach (0.96 F1), more complex than needed for MVP

---
*Research completed: 2026-02-11*
*Ready for roadmap: yes*
