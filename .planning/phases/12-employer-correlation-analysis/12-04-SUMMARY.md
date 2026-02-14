---
phase: 12-employer-correlation-analysis
plan: 04
subsystem: employer-correlation
tags: [cli, trades, portfolio, donor-context, user-facing]
dependency_graph:
  requires: [12-02-employer-mapping-db-layer]
  provides: [donor-context-ui, donation-summary-ui]
  affects: [trades-command, portfolio-command]
tech_stack:
  added: [show-donor-context-flag, show-donations-flag]
  patterns: [opt-in-features, stderr-output, graceful-degradation]
key_files:
  created: []
  modified:
    - capitoltraders_cli/src/commands/trades.rs
    - capitoltraders_cli/src/commands/portfolio.rs
    - capitoltraders_lib/src/db.rs
    - capitoltraders_cli/src/output_tests.rs
decisions:
  - DbTradeRow extended with politician_id and issuer_sector for donor context lookup
  - Donor context groups by (politician, sector) to avoid duplicate output for same sector
  - --show-donor-context requires --db mode, shows helpful note in scrape mode
  - --show-donations requires --politician filter for targeted donation summary
  - All donor/donation output on stderr to preserve stdout for piped data formats
  - Non-fatal error handling: donation summary errors print warnings, don't fail portfolio command
metrics:
  duration: 3min
  completed: 2026-02-14T01:42:21Z
  tasks: 2
  files: 4
  commits: 2
---

# Phase 12 Plan 04: Donor Context UI Summary

User-facing correlation features: --show-donor-context on trades shows top employers in traded sectors, --show-donations on portfolio shows politician donation summary.

## One-Liner

Added --show-donor-context to trades (top 5 employers per traded sector) and --show-donations to portfolio (total donations + top employer sectors) with graceful no-data handling.

## Tasks Completed

| Task | Name                                           | Commit  | Key Changes                                                              |
| ---- | ---------------------------------------------- | ------- | ------------------------------------------------------------------------ |
| 1    | Add --show-donor-context flag to trades       | cc21c98 | Flag, DbTradeRow fields, donor context display, test fixture update     |
| 2    | Add --show-donations flag to portfolio         | 7ddbe99 | Flag, donation summary display, error handling                           |

## Deviations from Plan

None - plan executed exactly as written.

## Implementation Details

### Task 1: Trades Donor Context

**Flag added:**
- `--show-donor-context` on TradesArgs (boolean, defaults to false)

**DbTradeRow extended:**
- `politician_id: String` (from trades table via SELECT)
- `issuer_sector: Option<String>` (from issuers table via existing JOIN)

**SQL changes:**
- query_trades SELECT adds `t.politician_id` (index 24) and `i.sector AS issuer_sector` (index 25)
- No new JOINs needed (issuers already joined for issuer_name/issuer_ticker)

**Display logic (run_db):**
```rust
if args.show_donor_context {
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for trade in &rows {
        if let Some(ref sector) = trade.issuer_sector {
            let key = (trade.politician_id.clone(), sector.clone());
            if seen.contains(&key) { continue; }
            seen.insert(key);

            let context = db.get_donor_context_for_sector(&trade.politician_id, sector, 5)?;
            // Display employer, total_amount, donation_count
        }
    }
}
```

**HashSet deduplication:**
- Key: (politician_id, sector)
- Prevents duplicate sector output when multiple trades in same sector
- Example: Politician trades AAPL + MSFT (both Technology) -> only one "Technology sector" output

**Scrape mode handling:**
- Prints: "Note: --show-donor-context requires --db mode."
- Does not fail, just informative

**Test fixture update:**
- sample_db_trade_row() in output_tests.rs updated with:
  - `politician_id: "P000001".to_string()`
  - `issuer_sector: Some("Technology".to_string())`

### Task 2: Portfolio Donation Summary

**Flag added:**
- `--show-donations` on PortfolioArgs (boolean, defaults to false)

**Display logic (run function):**
```rust
if args.show_donations {
    if let Some(ref pid) = filter.politician_id {
        match db.get_donation_summary(pid) {
            Ok(Some(summary)) => {
                eprintln!("Total received: ${:.0} ({} contributions)", ...);
                eprintln!("Top employer sectors (matched):");
                for st in &summary.top_sectors {
                    eprintln!("  {:30} ${:>12.0} ({} employers)", ...);
                }
            }
            Ok(None) => eprintln!("No donation data available..."),
            Err(e) => eprintln!("Warning: Could not load donation summary: {}", e),
        }
    } else {
        eprintln!("Note: --show-donations requires --politician filter...");
    }
}
```

**Error handling pattern:**
- Ok(Some(...)) -> display summary
- Ok(None) -> helpful hint to sync donations
- Err(...) -> warning message, does NOT fail command
- No politician filter -> helpful note

**Output format:**
```
--- Donation Summary ---
Total received: $123456 (78 contributions)
Top employer sectors (matched):
  Technology                    $     45000 (3 employers)
  Finance                       $     28000 (2 employers)
```

### Key Decisions

**DbTradeRow extension instead of separate query:**
- Adding politician_id + issuer_sector to DbTradeRow avoids N+1 query pattern
- Fields already available in query_trades SQL (via existing JOINs)
- Zero performance cost (no additional JOINs)

**Grouping by (politician, sector):**
- User views trades for multiple politicians or multiple sectors
- Without deduplication: same sector appears multiple times if multiple trades exist
- With HashSet: each (politician, sector) pair shown once

**stderr for all donor/donation output:**
- Preserves stdout for data format piping (JSON, CSV, etc.)
- User can pipe `capitoltraders trades --db ... --output json > trades.json` while still seeing donor context
- Follows existing pattern (option_count note also on stderr)

**--show-donor-context DB-only:**
- Donor context requires employer_mappings table (DB-only feature)
- Scrape mode has no access to donation/mapping data
- Graceful degradation: informative note instead of error

**--show-donations requires --politician:**
- Donation summary is per-politician (get_donation_summary takes politician_id)
- Without filter, unclear which politician's donations to show
- Could show aggregated donations for all filtered politicians (future enhancement)

**Non-fatal donation summary errors:**
- Portfolio command's primary purpose: show positions
- Donation summary is auxiliary information
- Errors in donation summary should warn, not break portfolio output

### Testing

**Test suite:**
- All 473 workspace tests pass
- output_tests.rs fixture updated (sample_db_trade_row)
- No new tests added (UI-facing features, integration testing deferred to UAT)

**Verification commands:**
```bash
cargo check --workspace          # compiles
cargo clippy --workspace         # no warnings
cargo test --workspace           # all tests pass
cargo run -- trades --help       # shows --show-donor-context
cargo run -- portfolio --help    # shows --show-donations
```

### Integration

**Ready for Plan 05 (UAT):**
- trades --db ... --show-donor-context displays employer donations in traded sectors
- portfolio --db ... --politician P000001 --show-donations displays donation summary
- Both features gracefully handle missing data (no employer mappings, no donations)

**Depends on Plan 03 (map-employers CLI) for data:**
- Without employer mappings: donor context shows "No donor context available" message
- Without synced donations: donation summary shows "No donation data available" message

**User workflows:**
1. View trades with donor context: `capitoltraders trades --db ... --party Democrat --show-donor-context`
2. View portfolio with donations: `capitoltraders portfolio --db ... --politician P000001 --show-donations`
3. Combine filters: `capitoltraders trades --db ... --state CA --sector Technology --show-donor-context`

## Files Modified

**capitoltraders_cli/src/commands/trades.rs:**
- Added show_donor_context flag to TradesArgs
- Added use std::collections::HashSet
- Added scrape mode note for --show-donor-context
- Added donor context display logic (49 lines)

**capitoltraders_cli/src/commands/portfolio.rs:**
- Added show_donations flag to PortfolioArgs
- Added donation summary display logic (38 lines)

**capitoltraders_lib/src/db.rs:**
- Extended DbTradeRow struct: politician_id, issuer_sector fields
- Updated query_trades SELECT: added t.politician_id, i.sector AS issuer_sector
- Updated row mapping: read politician_id (index 24), issuer_sector (index 25)

**capitoltraders_cli/src/output_tests.rs:**
- Updated sample_db_trade_row() fixture: politician_id, issuer_sector fields

## Performance Considerations

**No additional queries:**
- politician_id from trades table (already selected)
- issuer_sector from issuers table (already JOINed)
- Zero query overhead for DbTradeRow extension

**Donor context deduplication:**
- HashSet lookup: O(1) per trade
- Minimal memory (one entry per unique politician/sector pair)
- Typical case: 10-20 trades, 1-5 unique sectors -> negligible overhead

**Donation summary:**
- One get_donation_summary call per portfolio invocation
- Only called when --show-donations + --politician filter active
- Two SQL queries in get_donation_summary (total + top sectors)

## Self-Check: PASSED

**Modified files:**
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/commands/trades.rs: EXISTS
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/commands/portfolio.rs: EXISTS
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_lib/src/db.rs: EXISTS
- /Users/whit3rabbit/Documents/GitHub/capitoltraders/capitoltraders_cli/src/output_tests.rs: EXISTS

**Commits:**
- cc21c98 (Task 1): EXISTS in git log
- 7ddbe99 (Task 2): EXISTS in git log

**Verification commands:**
```bash
ls -l capitoltraders_cli/src/commands/trades.rs      # exists
ls -l capitoltraders_cli/src/commands/portfolio.rs   # exists
git log --oneline | grep cc21c98                      # found
git log --oneline | grep 7ddbe99                      # found
cargo test --workspace                                # 473 tests pass
cargo clippy --workspace                              # no warnings
cargo run -- trades --help | grep show-donor-context  # flag exists
cargo run -- portfolio --help | grep show-donations   # flag exists
```

All claims verified. Plan 04 complete.
