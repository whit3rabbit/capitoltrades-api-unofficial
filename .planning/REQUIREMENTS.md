# Requirements - FEC Donation Integration (v1.2)

**Milestone:** FEC Donation Integration
**Created:** 2026-02-11

## Environment & Foundation

- [ ] **REQ-v1.2-001**: .env file loading for API key storage
  - Add dotenvy 0.15 dependency to workspace
  - Load .env file at CLI startup (before command dispatch)
  - Read `OPENFEC_API_KEY` environment variable
  - Provide .env.example template with placeholder key
  - Update .gitignore to exclude .env files
  - Fail with clear error message if key is missing when donation commands are invoked

- [ ] **REQ-v1.2-002**: Congress-legislators FEC ID crosswalk
  - Download unitedstates/congress-legislators YAML dataset (current-legislators + historical)
  - Parse YAML to extract Bioguide ID, FEC candidate IDs, name, party, state
  - Store politician-to-FEC-ID mappings in SQLite (fec_mappings table)
  - Provide CLI command or automatic mechanism to populate/refresh mappings
  - Handle politicians with multiple FEC candidate IDs across election cycles

## API Client

- [ ] **REQ-v1.2-003**: OpenFEC API client module
  - Create openfec/ module in capitoltraders_lib (client.rs, types.rs, error.rs)
  - Implement candidate search endpoint (/candidates/search/)
  - Implement committee lookup endpoint (/candidate/{id}/committees/)
  - Implement Schedule A contributions endpoint (/schedules/schedule_a/)
  - Keyset pagination for Schedule A (last_index + last_contribution_receipt_date cursor)
  - OpenFecError enum with variants for 429 rate limit, 403 invalid key, network, parse errors
  - Rate limiting: respect 1,000 calls/hour cap with Semaphore concurrency = 3
  - Circuit breaker: trip after 5 consecutive 429 responses
  - Deserialization tests with JSON fixtures
  - Wiremock integration tests (success, rate limit, invalid key, pagination)

## Data Model & Mapping

- [ ] **REQ-v1.2-004**: Schema v3 migration
  - Add fec_mappings table (politician_id, fec_candidate_id, bioguide_id, committee_ids JSON, mapped_at)
  - Add donations table (sub_id PK, committee_id, contributor_name, contributor_employer, contributor_occupation, contributor_state, contributor_city, contributor_zip, contribution_receipt_amount, contribution_receipt_date, election_cycle, memo_text, receipt_type)
  - Add donation_sync_meta table (politician_id, committee_id, last_index, last_contribution_receipt_date, last_synced_at, total_synced)
  - Use existing PRAGMA user_version migration pattern (increment to v3)
  - Fresh DBs include all v1+v2+v3 columns in base schema
  - Migration tests (v2-to-v3, idempotency)

- [ ] **REQ-v1.2-005**: Politician-to-committee resolution pipeline
  - Three-tier cache: DashMap (in-memory) -> fec_mappings (SQLite) -> OpenFEC API (source of truth)
  - Resolve CapitolTrades politician to FEC candidate ID (via congress-legislators crosswalk first, API fallback)
  - Fetch all authorized committees per candidate (campaign, leadership PAC, joint fundraising)
  - Classify committee types (campaign vs leadership PAC vs joint fundraising)
  - Store resolved committee IDs in fec_mappings table
  - Handle politicians not found in FEC (log warning, skip gracefully)

## Sync Pipeline

- [ ] **REQ-v1.2-006**: Donation sync command
  - New subcommand: `capitoltraders sync-donations --db <path>`
  - Required: `--db` flag (DB-only operation), `OPENFEC_API_KEY` environment variable
  - Optional: `--politician <name>` (sync specific politician), `--cycle <year>` (specific election cycle), `--batch-size <N>` (default 100)
  - Fetch Schedule A contributions for each committee via keyset pagination
  - Deduplicate by FEC sub_id (ON CONFLICT ignore)
  - Incremental sync: use min_date checkpointing from donation_sync_meta
  - Persist cursor state in donation_sync_meta for resume after interruption
  - Concurrent fetching: Semaphore + JoinSet + mpsc pattern (concurrency = 3)
  - Circuit breaker: 5 consecutive 429s = stop with message
  - Progress reporting: N donations synced for politician X, total elapsed time
  - Exit code 0 on success (even with partial failures), non-zero on total failure

## Query & Display

- [ ] **REQ-v1.2-007**: Donations CLI subcommand
  - New subcommand: `capitoltraders donations --db <path>`
  - Required: `--db` flag (SQLite-only, no API calls)
  - Filters: `--politician <name>`, `--cycle <year>`, `--min-amount <dollars>`, `--employer <name>`, `--state <code>`, `--top <N>`
  - Aggregation: `--group-by contributor|employer|state` for summary views
  - Default: list individual contributions sorted by amount descending
  - All 5 output formats (table, JSON, CSV, markdown, XML) via global `--output` flag
  - Display: contributor name, employer, occupation, amount, date, committee type
  - Input validation for all filter parameters (reuse validation module patterns)

- [ ] **REQ-v1.2-008**: Donation analysis aggregations
  - Top N donors by total contribution amount per politician
  - Total donations by election cycle (2-year periods)
  - Geographic donor concentration (state-level breakdown)
  - Max-out donor identification ($3,300 per-election limit for 2025-2026 cycle)
  - Committee-level breakdown (campaign vs leadership PAC split)
  - Employer frequency analysis (top employers by donation count and total amount)

## Correlation & Integration

- [ ] **REQ-v1.2-009**: Employer-to-issuer correlation
  - Employer normalization: lowercase, strip corporate suffixes (Inc, LLC, Corp), collapse whitespace
  - Manual seed data for top 200 employers with ticker/sector mappings (JSON or TOML file)
  - Jaro-Winkler fuzzy matching via strsim crate for unmatched employers (0.85+ threshold)
  - Confidence-scored suggestions: exact match (1.0), fuzzy match (0.85-0.99), no match (skip)
  - Never auto-link: display suggestions for user review
  - Export-review-import workflow for building employer-to-issuer mappings over time

- [ ] **REQ-v1.2-010**: Donation context in existing commands
  - Optional `--show-donor-context` flag on trades command (shows top donors to politician for traded issuer's sector)
  - Optional donation summary line in portfolio output (total donations received, top employer sectors)
  - Both require prior sync-donations run; graceful no-op if no donation data exists

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| REQ-v1.2-001 | Phase 7 | Pending |
| REQ-v1.2-002 | Phase 7 | Pending |
| REQ-v1.2-003 | Phase 8 | Pending |
| REQ-v1.2-004 | Phase 9 | Pending |
| REQ-v1.2-005 | Phase 9 | Pending |
| REQ-v1.2-006 | Phase 10 | Pending |
| REQ-v1.2-007 | Phase 11 | Pending |
| REQ-v1.2-008 | Phase 11 | Pending |
| REQ-v1.2-009 | Phase 12 | Pending |
| REQ-v1.2-010 | Phase 12 | Pending |

**Coverage:** 10/10 requirements mapped (100%)

## Mapping to Active Requirements (PROJECT.md)

| PROJECT.md Active Requirement | REQ-IDs |
|-------------------------------|---------|
| OpenFEC API client with .env API key management | REQ-v1.2-001, REQ-v1.2-003 |
| FEC candidate ID mapping | REQ-v1.2-002, REQ-v1.2-005 |
| Schedule A contribution data ingestion | REQ-v1.2-006 |
| Donation storage in SQLite (schema v3) | REQ-v1.2-004 |
| Donation analysis: top donors, sector breakdown, employer-to-issuer correlation | REQ-v1.2-008, REQ-v1.2-009 |
| `donations` subcommand with filtering and all 5 output formats | REQ-v1.2-007 |
| Donation summary integrated into portfolio/trades output | REQ-v1.2-010 |

All 7 active requirements covered.

---
*Created: 2026-02-11*
