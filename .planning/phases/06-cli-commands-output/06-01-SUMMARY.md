---
phase: 06-cli-commands-output
plan: 01
subsystem: cli
tags:
  - portfolio
  - output-formatting
  - user-interface
dependency_graph:
  requires:
    - "05-02: Portfolio DB operations (get_portfolio, count_option_trades)"
    - "05-01: FIFO portfolio calculator"
    - "04-01: Price enrichment pipeline"
  provides:
    - "Portfolio CLI command with P&L display"
    - "Portfolio output formatting for all 5 formats"
  affects:
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/xml_output.rs
    - capitoltraders_cli/src/commands/portfolio.rs
tech_stack:
  added:
    - "format_currency_with_commas helper for thousand-separator formatting"
  patterns:
    - "DB-only command path (no scrape mode)"
    - "Option trades exclusion note for human-readable formats"
key_files:
  created:
    - capitoltraders_cli/src/commands/portfolio.rs
  modified:
    - capitoltraders_cli/src/output.rs
    - capitoltraders_cli/src/output_tests.rs
    - capitoltraders_cli/src/xml_output.rs
    - capitoltraders_cli/src/commands/mod.rs
    - capitoltraders_cli/src/main.rs
decisions:
  - "Option note only in table/markdown (human-readable), not JSON/CSV/XML (pure data formats)"
  - "Rust format strings do not support thousand separators, implemented custom format_currency_with_commas"
  - "Portfolio command is DB-only (no scrape mode) - requires synced + price-enriched database"
metrics:
  duration: "4.4 minutes"
  tasks_completed: 2
  commits: 2
  tests_added: 6
  files_created: 1
  files_modified: 5
  completed_at: "2026-02-11T11:55:21Z"
---

# Phase 6 Plan 1: Portfolio CLI Command Summary

Add the portfolio CLI subcommand with full output formatting support, completing the final user-facing feature for viewing per-politician stock positions with unrealized P&L.

## One-liner

Portfolio CLI command displays per-politician positions with unrealized P&L across all 5 output formats (table, JSON, CSV, markdown, XML).

## What Was Done

### Task 1: Portfolio output formatting and XML serialization
- Added PortfolioRow struct with 8 columns: politician_id, ticker, shares_held, avg_cost_basis, current_price, current_value, unrealized_pnl, unrealized_pnl_pct
- Implemented format_shares (2 decimal places) and format_currency (dollar sign + 2 decimals)
- Added format_currency_with_commas helper for readable large values (e.g., $2,500.00)
- Created 5 print functions: print_portfolio_table, print_portfolio_markdown, print_portfolio_csv, print_portfolio_xml, plus generic print_json
- Added portfolio_to_xml function in xml_output.rs using items_to_xml pattern
- Added 6 tests: format_shares, format_currency, build_portfolio_rows_with_pnl, build_portfolio_rows_missing_price, portfolio_csv_sanitization, portfolio_csv_headers
- All portfolio tests pass
- Commit: 8ec772b

### Task 2: Portfolio command module and CLI wiring
- Created capitoltraders_cli/src/commands/portfolio.rs with PortfolioArgs and run function
- PortfolioArgs includes --db (required), --politician, --party, --state, --ticker, --include-closed filters
- Implemented filter validation using validation module (validate_party, validate_state, validate_politician_id)
- Added option trades note to table/markdown output (count_option_trades from DB)
- Updated Commands enum with Portfolio variant
- Added Portfolio dispatch in main.rs (non-async, DB-only path)
- Updated commands/mod.rs to export portfolio module
- Updated module doc to mention 6 subcommands
- All 366 workspace tests pass
- No clippy warnings
- Commit: aa7cc69

## Deviations from Plan

None - plan executed exactly as written.

## Key Decisions

**1. Thousand separator formatting** (Task 1)
- Issue: Rust format strings do not support `,` flag for thousand separators
- Decision: Implemented custom format_currency_with_commas helper that manually inserts commas
- Rationale: Provides readable output for large portfolio values (e.g., $7,500.00 instead of $7500.00)

**2. Option trades note placement** (Task 2)
- Issue: Where to display the option trades exclusion note
- Decision: Show note only for table and markdown formats (human-readable), not JSON/CSV/XML (pure data formats)
- Rationale: Aligns with research recommendation in 06-RESEARCH.md open question #2 - data formats should be clean, notes are for human consumption

**3. Party validation string conversion** (Task 2)
- Issue: validate_party returns Party enum, but DB filter expects String
- Decision: Call .to_string() on validated Party enum to get display string (e.g., "Democrat")
- Rationale: Party Display impl returns lowercase ("democrat"), but DB stores titlecase ("Democrat") - .to_string() produces correct format

## Tests Added

1. test_format_shares - verify format_shares(100.5) == "100.50"
2. test_format_currency - verify format_currency(50.0) == "$50.00"
3. test_build_portfolio_rows_with_pnl - verify row formatting with known P&L values (shares, cost, price, P&L columns)
4. test_build_portfolio_rows_missing_price - verify "-" placeholder for None values (price, value, P&L)
5. test_portfolio_csv_sanitization - verify ticker="=SUM(A1)" gets tab-prefixed via sanitize_csv_field
6. test_portfolio_csv_headers - verify CSV header row matches PortfolioRow field names

All 6 tests pass. Total workspace tests: 366 (240 lib + 63 CLI + 9 schema validation + 8 integration + 7 deserialization + 36 query builders + 3 snapshots).

## Verification Results

1. `cargo test --workspace` - all 366 tests pass
2. `cargo clippy --workspace` - no warnings
3. `cargo run -p capitoltraders_cli -- portfolio --help` - shows expected flags and descriptions
4. `cargo run -p capitoltraders_cli -- enrich-prices --help` - still works (regression check)
5. PortfolioRow has 8 columns matching success criteria
6. Option note logic present in run() function (db.count_option_trades call)
7. All 5 output formats have corresponding print functions

## Self-Check: PASSED

**Files created:**
- [FOUND] capitoltraders_cli/src/commands/portfolio.rs

**Files modified:**
- [FOUND] capitoltraders_cli/src/output.rs (PortfolioRow, build_portfolio_rows, 5 print functions)
- [FOUND] capitoltraders_cli/src/output_tests.rs (6 portfolio tests)
- [FOUND] capitoltraders_cli/src/xml_output.rs (portfolio_to_xml function)
- [FOUND] capitoltraders_cli/src/commands/mod.rs (pub mod portfolio)
- [FOUND] capitoltraders_cli/src/main.rs (Commands::Portfolio, dispatch, doc update)

**Commits:**
- [FOUND] 8ec772b feat(06-01): add portfolio output formatting and XML serialization
- [FOUND] aa7cc69 feat(06-01): wire portfolio command into CLI

## Output Spec

Portfolio command provides comprehensive filtering and output options:

**Filters:**
- --politician <POLITICIAN_ID> - filter by politician ID (e.g., P000001)
- --party <PARTY> - filter by party: democrat (d), republican (r)
- --state <STATE> - filter by state (e.g., CA, TX)
- --ticker <TICKER> - filter by ticker symbol (e.g., AAPL)
- --include-closed - include closed positions (shares near zero)

**Output columns (8 total):**
1. Politician - politician ID
2. Ticker - stock symbol
3. Shares - shares held (formatted to 2 decimals)
4. Avg Cost - average cost basis per share ($X.XX)
5. Current Price - current market price ($X.XX or "-")
6. Current Value - total position value ($X,XXX.XX or "-")
7. Unrealized P&L - gain/loss with +/- prefix ($X,XXX.XX or "-")
8. P&L % - percentage gain/loss with +/- prefix (X.X% or "-")

**Output formats:**
- table (ASCII table)
- json (pretty-printed JSON array)
- csv (with formula sanitization)
- md/markdown (GitHub-flavored Markdown table)
- xml (well-formed XML document with <portfolio> root)

**Example usage:**
```bash
# View all positions
capitoltraders portfolio --db trades.db

# Filter by politician
capitoltraders portfolio --db trades.db --politician P000123

# Filter by party and state
capitoltraders portfolio --db trades.db --party democrat --state CA

# Filter by ticker
capitoltraders portfolio --db trades.db --ticker AAPL

# JSON output
capitoltraders portfolio --db trades.db --output json
```

## Impact

**User value:**
- Users can now see what their politicians' positions are currently worth
- Users can see whether politicians are making or losing money on their trades
- Users get a complete view of per-politician P&L across all holdings
- Completes the core user journey: trades → prices → portfolio → P&L

**Technical completeness:**
- Phase 6 Plan 1 complete (portfolio command)
- Enrich-prices command already existed from Phase 4 (per plan context)
- All 5 output formats supported consistently
- Option trades excluded with clear note (valuation deferred)

**Next steps:**
- No additional CLI commands planned
- Portfolio command complete and ready for use
- Future enhancements could add sorting, grouping, or aggregation options
