---
phase: 12-employer-correlation-analysis
plan: 03
subsystem: employer-correlation
tags: [cli, csv-export-import, seed-data-bootstrap, fuzzy-matching-ui]
dependency_graph:
  requires: [12-01-employer-mapping-module, 12-02-employer-db-layer]
  provides: [map-employers-cli, employer-mapping-workflow]
  affects: [employer_mappings-table, employer_lookup-table]
tech_stack:
  added: []
  patterns: [csv-export-import, dry-run-flag, configurable-threshold]
key_files:
  created:
    - capitoltraders_cli/src/commands/map_employers.rs (287 lines)
  modified:
    - capitoltraders_cli/src/commands/mod.rs (added map_employers module)
    - capitoltraders_cli/src/main.rs (added MapEmployers command variant)
decisions:
  - decision: "Export uses configurable threshold parameter instead of hardcoded 0.85"
    rationale: "Allows users to tune fuzzy matching sensitivity based on data quality. Some employer datasets may need stricter (0.90+) or looser (0.75-0.80) thresholds."
    alternatives: ["Hardcode 0.85 from Plan 01", "Add multiple preset profiles (strict/balanced/loose)"]
  - decision: "Import validates ticker existence before persisting"
    rationale: "Prevents invalid mappings from entering database. User may have typos or reference tickers not yet synced from CapitolTrades."
    alternatives: ["Trust user input without validation", "Validate against external data source"]
  - decision: "Load-seed skips tickers not in database with warning instead of failing"
    rationale: "User may not have synced all issuers yet. Seed data includes 52 mappings across multiple sectors - partial loading is better than all-or-nothing failure."
    alternatives: ["Fail entire load if any ticker missing", "Pre-filter seed data against DB before loading"]
  - decision: "CSV sanitization applied only to employer field, not all columns"
    rationale: "Only user-generated content (employer names from FEC data) poses formula injection risk. Suggestion columns are from controlled issuer data."
    alternatives: ["Sanitize all string columns", "Add sanitization flag to disable for trusted data"]
metrics:
  duration_minutes: 2
  completed_at: "2026-02-14T01:47:19Z"
  tasks_completed: 1
  tests_added: 0
  files_created: 1
  files_modified: 2
  lines_added: 290
---

# Phase 12 Plan 03: Map Employers CLI Summary

CLI command for building employer-to-issuer mapping database through export/import/load-seed workflow.

## One-Liner

map-employers CLI with export (CSV + fuzzy suggestions), import (confirmed mappings), and load-seed (TOML bootstrap) subcommands.

## Tasks Completed

| Task | Name                                          | Commit  | Key Changes                                                   |
| ---- | --------------------------------------------- | ------- | ------------------------------------------------------------- |
| 1    | Create map-employers CLI with 3 subcommands   | 5c24ac0 | map_employers.rs + mod.rs/main.rs registration, CSV handling  |

## What Was Built

Created the `map-employers` CLI command with three subcommands for building and managing the employer-to-issuer correlation database:

1. **Export**: Generates CSV of unmatched employers with fuzzy match suggestions
2. **Import**: Reads user-confirmed mappings from edited CSV and persists to DB
3. **Load-seed**: Bootstraps database with curated mappings from TOML seed file

## Implementation Details

### Command Structure

**Top-level args:**
- `--db <PATH>`: SQLite database path (required)

**Subcommands:**

1. **export**
   - `--output <PATH>`: CSV output file path
   - `--threshold <FLOAT>`: Jaro-Winkler threshold (default 0.85)
   - `--limit <INT>`: Max employers to export (optional)

2. **import**
   - `--input <PATH>`: CSV input file with confirmed mappings

3. **load-seed**
   - `--dry-run`: Show what would be loaded without writing to DB

### Export Flow

```rust
// 1. Validate threshold (0.0-1.0)
// 2. Get unmatched employers from DB
// 3. Get all issuers for matching
// 4. For each employer:
//    - Skip if blacklisted
//    - Normalize
//    - Match with configurable threshold
//    - Sanitize employer name for CSV
// 5. Write CSV with columns:
//    employer, normalized, suggestion_ticker, suggestion_name,
//    suggestion_sector, confidence, confirmed_ticker, notes
```

**Empty state handling:**
- No unmatched employers: "Run 'capitoltraders sync-donations' first"
- No issuers: "Run 'capitoltraders sync' first"

**CSV row structure:**
```rust
#[derive(Serialize)]
struct ExportRow {
    employer: String,           // Sanitized raw employer name
    normalized: String,         // Normalized for matching
    suggestion_ticker: String,  // Best match ticker (may be empty)
    suggestion_name: String,    // Issuer name
    suggestion_sector: String,  // Empty (not available from issuer data)
    confidence: String,         // Match score formatted to 2 decimals
    confirmed_ticker: String,   // Empty - for user to fill
    notes: String,              // Empty - for user annotations
}
```

### Import Flow

```rust
// 1. Read CSV from input path
// 2. For each row:
//    - Read confirmed_ticker column (index 6)
//    - Skip if empty (user didn't confirm)
//    - Validate ticker exists in DB
//    - Skip if invalid (with warning)
//    - Collect mapping: (normalized, ticker, 1.0, "manual")
//    - Collect lookup: (raw_lower, normalized)
// 3. Batch upsert to employer_mappings
// 4. Batch insert to employer_lookup
```

**Validation:**
- Checks `db.issuer_exists_by_ticker()` before persisting
- Warns and skips invalid tickers
- Prints summary: imported count + skipped count

### Load-Seed Flow

```rust
// 1. Load seed data from TOML (compile-time embedded)
// 2. For each SeedMapping:
//    - Check ticker exists in DB
//    - Skip with warning if missing
//    - For each employer_name variant:
//      - Normalize
//      - Collect mapping: (normalized, ticker, confidence, "exact")
//      - Collect lookup: (variant_lower, normalized)
// 3. If dry-run: print summary and exit
// 4. Batch upsert mappings + lookups
```

**Graceful degradation:**
- Missing tickers skipped with warning (user may not have synced all issuers)
- Partial loading succeeds (doesn't fail entire operation)
- Dry-run flag for preview without DB writes

## CSV Formula Injection Protection

**sanitize_csv_field applied to employer names:**
```rust
use crate::output::sanitize_csv_field;

// In export:
employer: sanitize_csv_field(employer),  // User-generated content from FEC
```

**Not applied to:**
- suggestion_ticker, suggestion_name: Controlled data from issuers table
- normalized, confidence: Generated by our code

## Key Features

**Configurable threshold:**
- Export accepts `--threshold` flag (0.0-1.0)
- Passed to `match_employer()` for fuzzy matching
- Allows tuning based on data quality
- Default 0.85 matches Plan 01 recommendation

**Dry-run support:**
- Load-seed `--dry-run` flag shows preview
- Prints mapping count, issuer count, skipped count
- No DB writes performed

**Helpful error messages:**
- Empty donations: "Run 'capitoltraders sync-donations' first"
- Empty issuers: "Run 'capitoltraders sync' first"
- Invalid ticker: "Warning: Ticker 'XYZ' not found in database. Skipping."
- Missing ticker in seed: "Warning: Ticker 'ABC' not found. Skipping N variants. (User may not have synced this issuer yet.)"

## Deviations from Plan

None - plan executed exactly as written. The previous attempt (which couldn't compile due to parallel plan 12-04 modifying DbTradeRow mid-flight) created all the correct code. This execution verified compilation after 12-04 completed and committed the work.

## Integration

**Consumes from Plan 01 & 02:**
- `normalize_employer()` - normalization logic
- `match_employer()` - fuzzy matching (now with threshold parameter)
- `is_blacklisted()` - non-corporate filter
- `load_seed_data()` - TOML parser
- DB methods: `get_unmatched_employers()`, `get_all_issuers_for_matching()`, `issuer_exists_by_ticker()`, `upsert_employer_mappings()`, `insert_employer_lookups()`

**Enables user workflow:**
1. `map-employers load-seed --db trades.db` - Bootstrap with 52 curated mappings
2. `map-employers export --db trades.db -o unmatched.csv --threshold 0.90` - Export with strict threshold
3. User edits CSV in spreadsheet, fills confirmed_ticker column
4. `map-employers import --db trades.db -i unmatched.csv` - Import confirmed mappings
5. Repeat export/import to incrementally grow mapping database

**Ready for Plan 04:**
- Employer mappings populated via load-seed or import
- employer_lookup table populated for SQL JOINs
- Donation queries can now correlate employers to issuers

## Testing

**Manual verification:**
```bash
# Help screens work
cargo run -p capitoltraders_cli -- map-employers --help
cargo run -p capitoltraders_cli -- map-employers export --help
cargo run -p capitoltraders_cli -- map-employers import --help
cargo run -p capitoltraders_cli -- map-employers load-seed --help

# All flags present
# export: --output, --threshold (default 0.85), --limit
# import: --input
# load-seed: --dry-run
```

**Workspace tests:**
- 356 tests pass (no new tests added - CLI wrapper over tested DB/mapping modules)
- Clippy clean
- Compiles successfully

## Files Modified

**capitoltraders_cli/src/commands/map_employers.rs (created):**
- 287 lines
- MapEmployersArgs, MapEmployersAction, ExportArgs, ImportArgs, LoadSeedArgs
- run(), run_export(), run_import(), run_load_seed()
- ExportRow struct for CSV serialization

**capitoltraders_cli/src/commands/mod.rs:**
- Added `pub mod map_employers;`

**capitoltraders_cli/src/main.rs:**
- Added MapEmployers variant to Commands enum
- Added match arm: `Commands::MapEmployers(args) => commands::map_employers::run(args)?`
- Updated doc comment: "eight subcommands" (was seven)

## Performance Considerations

**Export:**
- Batch fetch of unmatched employers and issuers (2 queries)
- In-memory matching (O(n*m) where n=employers, m=issuers)
- Single CSV write operation
- For 10,000 unmatched employers x 500 issuers: ~5M comparisons (reasonable for CLI)

**Import:**
- CSV streaming with csv::Reader
- Batch upsert (single transaction via upsert_employer_mappings)
- Batch insert to employer_lookup (single transaction)

**Load-seed:**
- TOML embedded at compile time (zero I/O cost)
- 52 seed mappings x ~3 variants each = ~150 mappings
- Batch operations (fast)

## Self-Check: PASSED

**Created files:**
```bash
# FOUND: capitoltraders_cli/src/commands/map_employers.rs
ls -l /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/commands/map_employers.rs
```

**Modified files:**
```bash
# FOUND: capitoltraders_cli/src/commands/mod.rs
# FOUND: capitoltraders_cli/src/main.rs
git diff HEAD~1 capitoltraders_cli/src/commands/mod.rs
git diff HEAD~1 capitoltraders_cli/src/main.rs
```

**Commits:**
```bash
# FOUND: 5c24ac0 (Task 1)
git log --oneline | grep 5c24ac0
```

**Command verification:**
```bash
cargo run -p capitoltraders_cli -- map-employers --help  # Shows 3 subcommands
cargo run -p capitoltraders_cli -- map-employers export --help  # Shows --threshold, --limit
```

**Tests:**
```bash
cargo test --workspace  # 356 tests pass
cargo clippy --workspace -- -D warnings  # Clean
```

All claims verified. Plan 03 complete.
