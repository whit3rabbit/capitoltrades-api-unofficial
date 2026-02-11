# Capitol Traders

A command-line tool for querying congressional stock trading data from [CapitolTrades](https://www.capitoltrades.com), enriching trades with Yahoo Finance market prices, and tracking per-politician portfolio positions with P&L.

There is no public API (as far as I can tell). The CLI uses an **unofficial API** by scraping the public site (Next.js RSC payloads) and normalizes the data for output.
The vendored [capitoltrades_api](https://github.com/TommasoAmici/capitoltrades) crate is still used for shared types
and validation helpers.

## Install

Requires Rust 1.70+.

```sh
cargo build --release
# Binary is at target/release/capitoltraders
```

## Usage

```sh
# List recent trades
capitoltraders trades

# Filter trades from the last 7 days (by publication date)
capitoltraders trades --days 7

# Filter trades from the last 30 days (by trade execution date)
capitoltraders trades --tx-days 30

# Filter trades published within an absolute date range
capitoltraders trades --since 2024-01-01 --until 2024-06-30

# Filter trades executed within an absolute date range
capitoltraders trades --tx-since 2024-01-01 --tx-until 2024-06-30

# Search trades by politician name
capitoltraders trades --politician pelosi

# Search trades by issuer name
capitoltraders trades --issuer nvidia

# Senate Democrats buying stock in the last 30 days
capitoltraders trades --chamber senate --party democrat --tx-type buy --days 30

# Technology sector trades
capitoltraders trades --sector information-technology

# Trades by state
capitoltraders trades --state CA --party republican

# Sort trades by reporting gap (how long after the trade it was disclosed)
capitoltraders trades --sort-by reporting-gap --asc

# List politicians sorted by trade volume
capitoltraders politicians

# Search for a politician by name
capitoltraders politicians --name pelosi

# List issuers in the technology sector
capitoltraders issuers --sector information-technology

# Look up a single issuer by ID
capitoltraders issuers --id 5678

# Output as JSON instead of a table
capitoltraders trades --output json

# Full SQLite dump (all trades, politicians, issuers)
capitoltraders sync --db capitoltraders.db --full

# Incremental SQLite update (since last stored pub date)
capitoltraders sync --db capitoltraders.db

# Sync and enrich: fetch detail pages for trades and issuers
capitoltraders sync --db capitoltraders.db --enrich

# Dry run: see how many items would be enriched
capitoltraders sync --db capitoltraders.db --enrich --dry-run

# Enrich with tuning: batch size, concurrency, failure threshold
capitoltraders sync --db capitoltraders.db --enrich --batch-size 100 --concurrency 5 --max-failures 10

# Query enriched trades from local database
capitoltraders trades --db capitoltraders.db

# Query enriched politicians with committee data
capitoltraders politicians --db capitoltraders.db --output json

# Query enriched issuers with performance data
capitoltraders issuers --db capitoltraders.db --sector information-technology

# Enrich trades with Yahoo Finance prices (historical + current)
capitoltraders enrich-prices --db capitoltraders.db

# Enrich with custom batch size
capitoltraders enrich-prices --db capitoltraders.db --batch-size 100

# View per-politician portfolio positions with P&L
capitoltraders portfolio --db capitoltraders.db

# Filter portfolio by politician, party, state, or ticker
capitoltraders portfolio --db capitoltraders.db --politician P000197
capitoltraders portfolio --db capitoltraders.db --party democrat --state CA
capitoltraders portfolio --db capitoltraders.db --ticker AAPL

# Portfolio output as JSON
capitoltraders portfolio --db capitoltraders.db --output json

# Include closed positions (shares near zero)
capitoltraders portfolio --db capitoltraders.db --include-closed
```

### Subcommands

**trades** -- List recent congressional stock trades.

| Flag | Description | Default |
|---|---|---|
| `--name` | Search by politician name (broad text search) | -- |
| `--politician` | Filter by politician name | -- |
| `--issuer` | Filter by issuer name/ticker | -- |
| `--issuer-id` | Filter by issuer ID (numeric) | -- |
| `--party` | `democrat` (`d`), `republican` (`r`), `other` | all |
| `--state` | US state code, e.g. `CA`, `TX`, `NY` | all |
| `--committee` | Committee code or full name, e.g. `ssfi`, `"Senate - Finance"` | all |
| `--days` | Trades published in last N days | all |
| `--tx-days` | Trades executed in last N days | all |
| `--since` | Trades published on/after this date (YYYY-MM-DD) | all |
| `--until` | Trades published on/before this date (YYYY-MM-DD) | all |
| `--tx-since` | Trades executed on/after this date (YYYY-MM-DD) | all |
| `--tx-until` | Trades executed on/before this date (YYYY-MM-DD) | all |
| `--trade-size` | Size bracket 1-10, comma-separated. 1=<$1K, 5=$100K-$250K, 10=$25M-$50M | all |
| `--gender` | `female` (`f`), `male` (`m`), comma-separated | all |
| `--market-cap` | `mega`, `large`, `mid`, `small`, `micro`, `nano` (or `1`-`6`), comma-separated | all |
| `--asset-type` | `stock`, `etf`, `cryptocurrency`, `mutual-fund`, etc., comma-separated | all |
| `--label` | `faang`, `crypto`, `memestock`, `spac`, comma-separated | all |
| `--sector` | `energy`, `financials`, `information-technology`, etc., comma-separated | all |
| `--tx-type` | `buy`, `sell`, `exchange`, `receive`, comma-separated | all |
| `--chamber` | `house` (`h`), `senate` (`s`), comma-separated | all |
| `--politician-id` | Politician ID (e.g. `P000197`), comma-separated | all |
| `--issuer-state` | 2-letter issuer state code (lowercase), comma-separated | all |
| `--country` | 2-letter ISO country code (lowercase), comma-separated | all |
| `--page` | Page number | 1 |
| `--page-size` | Results per page (ignored in scrape mode; fixed at 12) | 12 |
| `--sort-by` | `pub-date`, `trade-date`, `reporting-gap` | `pub-date` |
| `--asc` | Sort ascending | descending |
| `--details-delay-ms` | Delay between trade detail requests (ms) | 250 |
| `--db` | Read from local SQLite database instead of scraping | -- |

Most filter flags accept comma-separated values for multi-select, e.g. `--asset-type stock,etf` or `--trade-size 7,8,9`.
Date filters are mutually exclusive: use `--days`/`--tx-days` for relative days, or `--since`/`--until` and
`--tx-since`/`--tx-until` for absolute date ranges.

Scrape mode limitations: `--committee`, `--trade-size`, `--market-cap`, `--asset-type`, and `--label` are not
supported and will return an error. `--page-size` is fixed at 12.

DB mode (`--db`): Supported filters are `--party`, `--state`, `--tx-type`, `--name`, `--issuer`, `--since`, `--until`, `--days`.
Other filters are not yet supported and will return an error.

The `trades` command fetches each trade's detail page to populate `filingURL`/`filingId`. Use
`--details-delay-ms` to throttle those requests.

**politicians** -- List politicians and their trading activity.

| Flag | Description | Default |
|---|---|---|
| `--name` | Search by name | -- |
| `--party` | `democrat` (`d`), `republican` (`r`), `other` | all |
| `--state` | US state code | all |
| `--committee` | Committee code or full name | all |
| `--issuer-id` | Filter by issuer ID (numeric), comma-separated | all |
| `--page` | Page number | 1 |
| `--page-size` | Results per page (ignored in scrape mode; fixed at 12) | 12 |
| `--sort-by` | `volume`, `name`, `issuers`, `trades`, `last-traded` | `volume` |
| `--asc` | Sort ascending | descending |
| `--db` | Read from local SQLite database instead of scraping | -- |

Scrape mode limitations: `--committee` and `--issuer-id` are not supported and will return an error.
`--page-size` is fixed at 12.

DB mode (`--db`): Supported filters are `--party`, `--state`, `--name`.
Shows committee memberships when data has been enriched via `sync --enrich`.

**issuers** -- List or look up stock issuers.

| Flag | Description | Default |
|---|---|---|
| `--id` | Look up a single issuer by ID | -- |
| `--search` | Search by name | -- |
| `--sector` | `financials`, `health-care`, `information-technology`, etc. | all |
| `--market-cap` | `mega`, `large`, `mid`, `small`, `micro`, `nano` | all |
| `--state` | US state code | all |
| `--country` | 2-letter ISO country code (lowercase), comma-separated | all |
| `--politician-id` | Politician ID (e.g. `P000197`), comma-separated | all |
| `--page` | Page number | 1 |
| `--page-size` | Results per page (ignored in scrape mode; fixed at 12) | 12 |
| `--sort-by` | `volume`, `politicians`, `trades`, `last-traded`, `mcap` | `volume` |
| `--asc` | Sort ascending | descending |
| `--db` | Read from local SQLite database instead of scraping | -- |
| `--limit` | Maximum results to return (DB mode only) | all |

Scrape mode limitations: `--market-cap`, `--state`, `--country`, `--politician-id`, and `--sort-by mcap`
are not supported and will return an error. `--page-size` is fixed at 12.

DB mode (`--db`): Supported filters are `--search`, `--sector`, `--state`, `--country`, `--limit`.
Shows performance metrics and EOD price data when data has been enriched via `sync --enrich`.

**sync** -- Ingest CapitolTrades data into SQLite.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path | `capitoltraders.db` |
| `--full` | Full refresh of trades, politicians, issuers | off |
| `--since` | Override incremental cutoff date (YYYY-MM-DD, pub date) | -- |
| `--refresh-politicians` | Refresh full politician catalog during incremental run | off |
| `--refresh-issuers` | Refresh full issuer catalog during incremental run | off |
| `--page-size` | Page size for API pagination (1-100, ignored in scrape mode) | 100 |
| `--enrich` | Enrich trade, issuer, and politician details after sync | off |
| `--dry-run` | Show how many items would be enriched (requires `--enrich`) | off |
| `--batch-size` | Maximum items to enrich per entity type per run | all |
| `--details-delay-ms` | Delay between detail page requests (ms) | 500 |
| `--concurrency` | Number of concurrent detail page fetches (1-10) | 3 |
| `--max-failures` | Stop enrichment after N consecutive HTTP failures | 5 |

Enrichment (`--enrich`) fetches individual detail pages for trades, issuers, and politicians to
populate fields that listing pages leave empty: asset types, filing details, trade sizing, pricing,
committee memberships, performance metrics, and EOD price history. Smart-skip avoids re-fetching
already-enriched records. Progress bars show enrichment status. A circuit breaker stops after
`--max-failures` consecutive HTTP failures.

**enrich-prices** -- Enrich trades with Yahoo Finance market prices.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--batch-size` | Maximum trades to enrich per run | 50 |
| `--force` | Re-enrich already-enriched trades (reserved, not yet active) | off |

Enrichment runs in two phases: (1) historical trade-date prices fetched per unique (ticker, date) pair,
then (2) current prices fetched per unique ticker. Trades without valid tickers are marked as processed
and skipped on future runs. Rate limiting (200-500ms jittered delay, max 5 concurrent) prevents Yahoo
Finance throttling. A circuit breaker trips after 10 consecutive failures. Progress displays ticker
counts and success/fail/skip summary.

**portfolio** -- View per-politician stock positions with unrealized P&L.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician ID (e.g. `P000197`) | all |
| `--party` | `democrat` (`d`), `republican` (`r`) | all |
| `--state` | US state code, e.g. `CA`, `TX` | all |
| `--ticker` | Filter by ticker symbol, e.g. `AAPL` | all |
| `--include-closed` | Include positions with near-zero shares | off |

Requires a synced and price-enriched database (`sync` then `enrich-prices`). Positions are calculated
using FIFO (First-In-First-Out) accounting from estimated share counts. Output columns: Politician,
Ticker, Shares, Avg Cost, Current Price, Current Value, Unrealized P&L, P&L %. Option trades are
excluded from position calculations and noted separately in table/markdown output.

### Global Flags

| Flag | Description | Default |
|---|---|---|
| `--output` | `table`, `json`, `csv`, `md`, or `xml` | `table` |
| `--base-url` | Override scraping base URL (or set `CAPITOLTRADES_BASE_URL`) | `https://www.capitoltrades.com` |

## CI

The daily SQLite sync workflow lives at `.github/workflows/sqlite-sync.yml`. It restores the previous
database from a GitHub Actions cache, runs `capitoltraders sync`, and uploads the updated database as
an artifact.

## Output Formats & Schemas

Output is written to stdout; pagination metadata is written to stderr. Supported formats:

- `table` (human-readable table)
- `json` (array of items)
- `csv` (header + rows)
- `md` (Markdown table)
- `xml` (well-formed XML with root `<trades>`, `<politicians>`, or `<issuers>`)

Schemas live in `schema/`:

- JSON Schema: `schema/trade.schema.json`, `schema/politician.schema.json`, `schema/issuer.schema.json`
- XML Schema: `schema/trades.xsd`, `schema/politicians.xsd`, `schema/issuers.xsd`
- SQLite DDL: `schema/sqlite.sql`

These schemas describe the CLI output (arrays of items), not the PaginatedResponse wrapper.

## Project Structure

```
capitoltraders/
  Cargo.toml                    # workspace root
  capitoltrades_api/            # vendored upstream API client
  capitoltraders_lib/           # library: cache, analysis, validation, scraping, db, yahoo, pricing, portfolio
  capitoltraders_cli/           # CLI binary (6 subcommands)
  schema/sqlite.sql             # SQLite schema (v2) with price columns and positions table
```

## Development

```sh
# Run all tests (366 total)
cargo test --workspace

# Lint
cargo clippy --workspace

# Run the CLI in dev mode
cargo run -p capitoltraders_cli -- trades --days 7
```

## Data Source

There is no public API. All data is scraped from the [CapitolTrades](https://www.capitoltrades.com) website by
parsing the Next.js RSC payloads embedded in HTML responses. Results are cached in-memory for 5 minutes to
reduce request load. Use `sync --enrich` to fetch detail pages and populate enriched fields (asset types,
filing details, committee memberships, performance data). Senate filings often use UUID-style URLs, so
`filing_id` may remain `0` while `filing_url` is populated.

## SQLite

The `sync` subcommand writes to SQLite using the schema in `schema/sqlite.sql` (currently at v2). Tables map
directly to the CLI JSON output schemas (`schema/*.schema.json`), including nested data:

- `trades`, `assets`, `issuers`, `politicians`
- `trade_committees`, `trade_labels`, `politician_committees`
- `issuer_stats`, `politician_stats`, `issuer_performance`, `issuer_eod_prices`
- `positions` (materialized FIFO portfolio positions per politician per ticker)
- `ingest_meta` (tracks `last_trade_pub_date` for incremental sync)

The trades table includes price enrichment columns: `trade_date_price`, `current_price`,
`price_enriched_at`, `estimated_shares`, `estimated_value`. These are populated by `enrich-prices`.

Incremental runs use `last_trade_pub_date` to request only recent pages from the API, then upsert by
primary key to keep the database current. Enrichment (`--enrich`) populates the join and detail tables
by fetching individual detail pages post-ingest.

## Rate Limiting

This tool uses an unofficial API and adds a randomized 5-10 second delay between HTTP requests to avoid putting unnecessary load on the CapitolTrades servers. Cache hits are not delayed, so repeated queries within the 5-minute cache window return instantly. The first request in a session has no delay. Enrichment uses a configurable delay (default 500ms) between detail page fetches with bounded concurrency (default 3).

## License

This project vendors code from [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). See that repository for its license terms.
