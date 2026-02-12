# Roadmap: Capitol Traders

## Milestones

- v1.1 **Yahoo Finance Price Enrichment** -- Phases 1-6 (shipped 2026-02-11)
- v1.2 **FEC Donation Integration** -- Phases 7-12 (in progress)

## Phases

<details>
<summary>v1.1 Yahoo Finance Price Enrichment (Phases 1-6) -- SHIPPED 2026-02-11</summary>

- [x] Phase 1: Schema Migration & Data Model (1/1 plans) -- completed 2026-02-10
- [x] Phase 2: Yahoo Finance Client Integration (1/1 plans) -- completed 2026-02-10
- [x] Phase 3: Ticker Validation & Trade Value Estimation (1/1 plans) -- completed 2026-02-11
- [x] Phase 4: Price Enrichment Pipeline (1/1 plans) -- completed 2026-02-11
- [x] Phase 5: Portfolio Calculator (FIFO) (2/2 plans) -- completed 2026-02-10
- [x] Phase 6: CLI Commands & Output (1/1 plans) -- completed 2026-02-11

Full details: `.planning/milestones/v1.1-ROADMAP.md`

</details>

### v1.2 FEC Donation Integration

**Milestone Goal:** Integrate OpenFEC donation data to show who funds each politician, correlated against their trading activity.

#### Phase 7: Foundation & Environment Setup
**Goal**: Project can load API keys from environment and resolve politician-to-FEC ID mappings without consuming API budget
**Depends on**: Phase 6 (v1.1 complete)
**Requirements**: REQ-v1.2-001, REQ-v1.2-002
**Success Criteria** (what must be TRUE):
  1. Running any donation-related command without a .env file or OPENFEC_API_KEY produces a clear error message explaining how to get and configure the key
  2. .env file is loaded at startup and .gitignore excludes it from version control
  3. Congress-legislators dataset is parsed and politician-to-FEC-ID mappings are stored in SQLite
  4. A lookup by politician name or Bioguide ID returns the correct FEC candidate ID(s)
**Plans**: 2 plans

Plans:
- [x] 07-01-PLAN.md -- Environment setup, dependencies, schema v3 migration with fec_mappings table
- [x] 07-02-PLAN.md -- FEC mapping module, YAML parsing, DB operations, sync-fec CLI command

#### Phase 8: OpenFEC API Client
**Goal**: System can communicate with the OpenFEC API, handling pagination, rate limits, and errors correctly
**Depends on**: Phase 7
**Requirements**: REQ-v1.2-003
**Success Criteria** (what must be TRUE):
  1. Client can search for candidates by name and return structured candidate records
  2. Client can fetch all authorized committees for a given candidate ID
  3. Client can fetch Schedule A contributions using keyset pagination (not page numbers)
  4. A 429 rate limit response triggers backoff and circuit breaker, not a crash
  5. Wiremock tests verify all endpoints (success, rate limit, invalid key, multi-page pagination)
**Plans**: TBD

Plans:
- [ ] 08-01: TBD

#### Phase 9: Politician-to-Committee Mapping & Schema v3
**Goal**: Database schema supports donation storage and politician-to-committee resolution is fully operational
**Depends on**: Phase 8
**Requirements**: REQ-v1.2-004, REQ-v1.2-005
**Success Criteria** (what must be TRUE):
  1. Schema v3 migration adds donations, fec_mappings, and donation_sync_meta tables without breaking existing v2 data
  2. Fresh database creation includes all v1+v2+v3 schema in the base DDL
  3. Given a CapitolTrades politician, the system resolves their FEC candidate ID and all authorized committee IDs
  4. Committee resolution uses three-tier cache (memory -> SQLite -> API) to minimize API calls
  5. Committee types are classified (campaign vs leadership PAC vs joint fundraising)
**Plans**: TBD

Plans:
- [ ] 09-01: TBD

#### Phase 10: Donation Sync Pipeline
**Goal**: Users can sync FEC donation data into their local database for any politician
**Depends on**: Phase 9
**Requirements**: REQ-v1.2-006
**Success Criteria** (what must be TRUE):
  1. `capitoltraders sync-donations --db trades.db --politician "Nancy Pelosi"` fetches and stores Schedule A contributions for all of that politician's committees
  2. Sync is resumable: interrupting and re-running picks up where it left off (keyset cursor persisted)
  3. Duplicate donations are rejected (sub_id deduplication)
  4. Progress is reported during sync (donations synced count, elapsed time)
  5. Circuit breaker halts sync after 5 consecutive 429 errors with an informative message
**Plans**: TBD

Plans:
- [ ] 10-01: TBD

#### Phase 11: Donations CLI Command
**Goal**: Users can query and analyze synced donation data through the CLI
**Depends on**: Phase 10
**Requirements**: REQ-v1.2-007, REQ-v1.2-008
**Success Criteria** (what must be TRUE):
  1. `capitoltraders donations --db trades.db --politician "Nancy Pelosi"` lists individual contributions sorted by amount
  2. `--group-by employer` aggregates donations by employer with total amount and count
  3. `--top 10` shows the top N donors by total contribution amount
  4. All 5 output formats work (table, JSON, CSV, markdown, XML)
  5. Filters (--cycle, --min-amount, --employer, --state) narrow results correctly
**Plans**: TBD

Plans:
- [ ] 11-01: TBD

#### Phase 12: Employer Correlation & Analysis
**Goal**: Users can see connections between donation sources and traded securities
**Depends on**: Phase 11
**Requirements**: REQ-v1.2-009, REQ-v1.2-010
**Success Criteria** (what must be TRUE):
  1. Employer names are normalized and matched against stock issuers with confidence scores
  2. `--show-donor-context` on trades command displays donation context for the politician's traded sectors
  3. Portfolio output includes optional donation summary (total received, top employer sectors) when donation data exists
  4. Unmatched employers can be exported for manual review and re-imported as confirmed mappings
**Plans**: TBD

Plans:
- [ ] 12-01: TBD

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Schema Migration & Data Model | v1.1 | 1/1 | Complete | 2026-02-10 |
| 2. Yahoo Finance Client Integration | v1.1 | 1/1 | Complete | 2026-02-10 |
| 3. Ticker Validation & Trade Value Estimation | v1.1 | 1/1 | Complete | 2026-02-11 |
| 4. Price Enrichment Pipeline | v1.1 | 1/1 | Complete | 2026-02-11 |
| 5. Portfolio Calculator (FIFO) | v1.1 | 2/2 | Complete | 2026-02-10 |
| 6. CLI Commands & Output | v1.1 | 1/1 | Complete | 2026-02-11 |
| 7. Foundation & Environment Setup | v1.2 | 2/2 | Complete | 2026-02-12 |
| 8. OpenFEC API Client | v1.2 | 0/TBD | Not started | - |
| 9. Politician-to-Committee Mapping & Schema v3 | v1.2 | 0/TBD | Not started | - |
| 10. Donation Sync Pipeline | v1.2 | 0/TBD | Not started | - |
| 11. Donations CLI Command | v1.2 | 0/TBD | Not started | - |
| 12. Employer Correlation & Analysis | v1.2 | 0/TBD | Not started | - |
