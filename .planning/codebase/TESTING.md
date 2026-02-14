# Testing Patterns

**Analysis Date:** 2026-02-14

## Test Framework

**Runner:** Standard Rust test harness (`cargo test`)

**Run Commands:**
```bash
cargo test --workspace              # Run all 503 tests
cargo test -p capitoltraders_lib    # Run lib tests (OpenFEC, Yahoo, FIFO)
cargo test --test openfec_integration  # Run specific integration suite
```

**Test Count:**
- 503 total tests across three crates
- Key focus areas:
  - Validation (83 tests)
  - Database operations and migrations (90+ tests)
  - Output formatting (50+ tests)
  - OpenFEC and Yahoo Finance integration (Wiremock-backed)
  - Employer normalization and fuzzy matching
  - FIFO portfolio accounting logic

## Test File Organization

**Location:**
- Unit tests: Inline in `#[cfg(test)]` modules
- Integration tests: `capitoltrades_api/tests/`, `capitoltraders_lib/tests/`, `capitoltraders_cli/tests/`
- Fixtures: `tests/fixtures/` (HTML, JSON, YAML)

## Mocking

**Framework:** `wiremock` 0.6 (for HTTP mocking)

**Patterns:**
- `openfec_integration.rs`: Mocks Schedule A contributions, candidate searches, and committee resolution. Validates keyset pagination logic.
- `client_integration.rs`: Mocks CapitolTrades BFF API responses.
- `committee_resolver_integration.rs`: Mocks multi-tier committee lookup (Memory -> DB -> API).

## Fixtures and Factories

- **HTML Fixtures:** Used for scraping validation (e.g., `trade_detail.html`).
- **JSON Fixtures:** Used for API response deserialization.
- **YAML Fixtures:** Used for congress-legislators dataset parsing tests.
- **In-memory SQLite:** `Db::open_in_memory()` used for all database tests to ensure isolation and performance.

## Core Patterns

**Keyset Pagination Testing:**
Integration tests specifically verify that the OpenFEC client correctly propagates `last_index` and `last_contribution_receipt_date` through multiple pages, matching the mock server expectations.

**Circuit Breaker Testing:**
Tests in `capitoltraders_cli/src/commands/` verify that enrichment pipelines halt execution after the configurable failure threshold is reached.

**Fuzzy Match Validation:**
Employer mapping tests verify confidence score thresholds for Jaro-Winkler similarity across various corporate naming variations.

---

*Testing analysis: 2026-02-14*
