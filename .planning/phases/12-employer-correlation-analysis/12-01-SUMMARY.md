---
phase: 12
plan: 01
subsystem: employer-mapping
tags: [fuzzy-matching, normalization, seed-data, pure-logic]
dependency_graph:
  requires: []
  provides: [employer-normalization, employer-fuzzy-matching, seed-data-loading]
  affects: [employer-mapping-db, employer-correlation-cli]
tech_stack:
  added: [strsim-0.11, toml-0.8]
  patterns: [jaro-winkler-similarity, compile-time-toml-embed, corporate-suffix-stripping]
key_files:
  created:
    - capitoltraders_lib/src/employer_mapping.rs (429 lines)
    - seed_data/employer_issuers.toml (332 lines, 52 mappings)
  modified:
    - capitoltraders_lib/Cargo.toml (added strsim, toml)
    - capitoltraders_lib/src/lib.rs (module declaration, re-exports)
    - capitoltraders_lib/src/committee.rs (clippy fix)
decisions:
  - decision: "Use scoped MutexGuard pattern to fix await_holding_lock clippy warning in committee.rs"
    rationale: "Pre-existing clippy warning prevented clean compilation with -D warnings. Fixed by wrapping DB access in a scope to ensure lock drops before async operations."
    alternatives: ["Leave warning unfixed", "Use async-aware Mutex (tokio::sync::Mutex)"]
  - decision: "Start with 52 seed mappings instead of aspirational 200"
    rationale: "Quality over quantity for initial seed data. Export-review-import workflow (Plan 03) designed to grow database incrementally from real FEC data. High-confidence manual curation is time-intensive with diminishing returns beyond top contributors."
    alternatives: ["Generate 200 mappings with lower confidence", "Start with minimal set of 10-20"]
  - decision: "Corporate suffix list sorted by length descending"
    rationale: "Ensures longer suffixes like 'corporation' match before shorter ones like 'corp', preventing incorrect partial matches."
    alternatives: ["Alphabetical sorting", "Unsorted list with exact match logic"]
  - decision: "Short employer names (< 5 chars) require exact match only"
    rationale: "Prevents false positives from fuzzy matching on abbreviated names like 'IBM' or 'AMD'. Reduces noise in matching results."
    alternatives: ["Allow fuzzy matching for all lengths", "Use different threshold for short names"]
metrics:
  duration_minutes: 4
  completed_at: "2026-02-14T01:24:00Z"
  tasks_completed: 2
  tests_added: 17
  files_created: 2
  files_modified: 3
  lines_added: 761
---

# Phase 12 Plan 01: Employer Mapping Module Summary

Employer-to-issuer matching logic with normalization, fuzzy matching via Jaro-Winkler, and seed data for 52 top employers.

## What Was Built

Created a pure logic module (no DB or CLI dependencies) for matching employer names from FEC donation data to issuer records. The module provides:

1. **Normalization**: Strips corporate suffixes (Inc, LLC, Corp, etc.), collapses whitespace, lowercase conversion
2. **Blacklisting**: Filters out non-corporate employers (Retired, Self-employed, N/A, etc.)
3. **Matching Tiers**:
   - Exact match: normalized employer == normalized issuer (confidence 1.0)
   - Fuzzy match: Jaro-Winkler similarity >= threshold (default 0.85), only for names >= 5 chars
4. **Seed Data**: 52 manually curated employer-to-issuer mappings loaded at compile time via include_str!

## Implementation Details

### Module Structure

**capitoltraders_lib/src/employer_mapping.rs** exports:
- `normalize_employer(raw: &str) -> String` - Normalization pipeline
- `match_employer(employer: &str, issuers: &[(i64, String, String)], threshold: f64) -> Option<MatchResult>` - Multi-tier matching
- `is_blacklisted(employer: &str) -> bool` - Non-corporate employer filter
- `load_seed_data() -> Result<Vec<SeedMapping>, EmployerMappingError>` - Seed TOML parser
- Types: `MatchResult`, `MatchType`, `SeedMapping`, `EmployerMappingError`

### Normalization Algorithm

1. Trim whitespace
2. Convert to lowercase
3. Strip ONE trailing corporate suffix (from `CORPORATE_SUFFIXES` list)
4. Strip trailing punctuation (dots, commas, spaces)
5. Collapse multiple spaces to single space

**Corporate suffixes** (26 total, sorted by length descending):
- "information requested per best efforts" (longest)
- "corporation", "incorporated", "partnership", "associates", "holdings", "partners", "limited", "company"
- "l.l.c.", "group", "gmbh", "corp", "inc", "llc", "ltd", "l.p.", "plc", "n.v.", "s.a.", "co", "ag"

### Blacklist Patterns

**11 blacklisted patterns** for non-corporate employers:
- "self-employed", "self employed", "retired", "not employed"
- "n/a", "none", "homemaker", "student", "unemployed"
- "information requested", "information requested per best efforts"

### Matching Logic

```rust
// Tier 1: Blacklist check
if is_blacklisted(employer) { return None; }

// Tier 2: Exact match
if normalize(employer) == normalize(issuer_name) {
    return MatchResult { confidence: 1.0, match_type: Exact }
}

// Tier 3: Fuzzy match (only if employer.len() >= 5 after normalization)
if normalized_employer.len() >= 5 {
    let score = jaro_winkler(employer, issuer_name);
    if score >= threshold {
        return MatchResult { confidence: score, match_type: Fuzzy }
    }
}

return None;
```

### Seed Data Structure

**52 mappings** across 8 sectors in `seed_data/employer_issuers.toml`:

| Sector                   | Count | Example Tickers         |
| ------------------------ | ----- | ----------------------- |
| Big Tech                 | 13    | AAPL, GOOGL, MSFT, AMZN |
| Finance                  | 8     | GS, JPM, MS, BAC, C     |
| Healthcare               | 6     | JNJ, PFE, UNH, MRK      |
| Energy                   | 3     | XOM, CVX, COP           |
| Defense & Aerospace      | 5     | LMT, RTX, BA, NOC, GD   |
| Consumer                 | 5     | WMT, PG, KO, PEP, MCD   |
| Telecom                  | 4     | T, VZ, CMCSA, TMUS      |
| Additional Major Issuers | 8     | TSLA, BRK.B, V, MA, DIS |

Each mapping includes:
- `employer_names` (2-4 name variants for fuzzy matching)
- `issuer_ticker` (join key to issuers table)
- `sector` (for categorization)
- `confidence` (always 1.0 for seed data)
- `notes` (optional context, e.g., "Rebranded from Facebook to Meta in 2021")

## Test Coverage

**17 unit tests** (all passing):

### Normalization Tests (7)
- `test_normalize_basic`: "Apple Inc" -> "apple"
- `test_normalize_llc`: "Google LLC" -> "google"
- `test_normalize_corporation`: "Microsoft Corporation" -> "microsoft"
- `test_normalize_preserves_spaces`: "Goldman Sachs Group" -> "goldman sachs"
- `test_normalize_empty`: "" -> ""
- `test_normalize_whitespace_collapse`: "  Apple   Inc  " -> "apple"
- `test_normalize_international`: "Siemens AG" -> "siemens"

### Blacklist Tests (4)
- `test_blacklisted_retired`: "Retired" returns true
- `test_blacklisted_self_employed`: "SELF-EMPLOYED" returns true
- `test_blacklisted_normal`: "Apple Inc" returns false
- `test_blacklisted_na`: "N/A" returns true

### Matching Tests (6)
- `test_match_exact`: "Apple" matches "Apple" at confidence 1.0
- `test_match_fuzzy`: "Apple Computer" matches "Apple" with score >= 0.85
- `test_match_blacklisted_returns_none`: "Retired" returns None
- `test_match_short_name_no_fuzzy`: "IBM" (3 chars) does not fuzzy match "IBMC Corp"
- `test_match_no_match`: "Random Xyz Company" returns None
- `test_load_seed_data`: Validates TOML structure and non-empty mappings

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed await_holding_lock clippy warning in committee.rs**
- **Found during:** Task 1 verification (cargo clippy)
- **Issue:** MutexGuard held in scope across await points in `CommitteeResolver::resolve_committees`, causing clippy error with -D warnings
- **Fix:** Wrapped DB access in a scope to ensure MutexGuard drops before async operations. Extracted `(fec_ids, politician_info)` from scope as return values.
- **Files modified:** capitoltraders_lib/src/committee.rs
- **Commit:** 3c2815c
- **Rationale:** Pre-existing Phase 9 code had manual `drop(db)` calls but clippy still detected guard in scope during async operations. Scoped pattern ensures lock is automatically dropped when scope exits, satisfying clippy's static analysis.

## Dependencies Added

**New crates:**
- `strsim = "0.11"` - Jaro-Winkler string similarity for fuzzy matching
- `toml = "0.8"` - Parse seed data at compile time

## Performance Characteristics

**Normalization**: O(n) where n is employer name length (single pass with suffix check)

**Blacklist check**: O(k) where k is blacklist size (11 items, constant)

**Exact match**: O(m) where m is number of issuers (linear scan with early return)

**Fuzzy match**: O(m * n^2) where m is issuers, n is average name length (Jaro-Winkler is O(n^2) per comparison)

**Seed data loading**: Zero runtime cost (include_str! embeds TOML at compile time)

## Integration Points

**Consumed by:**
- Plan 02 (Employer Mapping DB layer) - DB schema + upsert operations
- Plan 03 (Employer Correlation CLI) - Command-line interface for matching

**Provides to downstream:**
- Pure functions for normalization and matching (no side effects)
- Seed data pre-loaded at startup
- Configurable fuzzy match threshold for tuning

## Verification Results

```bash
cargo test -p capitoltraders_lib employer_mapping -- --nocapture
# test result: ok. 17 passed; 0 failed

cargo clippy --workspace -- -D warnings
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.58s

cargo check --workspace
# Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.33s
```

## Self-Check: PASSED

**Created files exist:**
```bash
# FOUND: capitoltraders_lib/src/employer_mapping.rs
# FOUND: seed_data/employer_issuers.toml
```

**Commits exist:**
```bash
# FOUND: 3c2815c (Task 1 - employer mapping module)
# FOUND: 66cb443 (Task 2 - seed data)
```

**Mapping count:**
```bash
# grep -c "^\[\[mapping\]\]" seed_data/employer_issuers.toml
# 52 (exceeds requirement of 40+)
```

**Test count:**
```bash
# cargo test -p capitoltraders_lib employer_mapping 2>&1 | grep "test result"
# test result: ok. 17 passed; 0 failed
```
