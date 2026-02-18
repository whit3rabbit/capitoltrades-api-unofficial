# Capitol Traders

A command-line tool for querying congressional stock trading data from [CapitolTrades](https://www.capitoltrades.com), enriching trades with Yahoo Finance market prices, tracking per-politician portfolio positions with P&L, and scoring trading anomalies, committee conflicts, and performance analytics.

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

# Sync FEC candidate mappings
capitoltraders sync-fec --db capitoltraders.db

# Sync donations for a politician
capitoltraders sync-donations --db capitoltraders.db --politician pelosi --cycle 2024

# Query donations aggregated by employer
capitoltraders donations --db capitoltraders.db --politician pelosi --group-by employer --top 10

# Load curated employer-to-issuer mappings
capitoltraders map-employers --db capitoltraders.db load-seed

# Export unmatched employers for review
capitoltraders map-employers --db capitoltraders.db export -o unmatched.csv

# View trades with donor context
capitoltraders trades --db capitoltraders.db --show-donor-context

# View politician performance leaderboard
capitoltraders analytics --db capitoltraders.db --sort-by alpha --top 10

# Analytics filtered by party and period
capitoltraders analytics --db capitoltraders.db --party democrat --period 1y

# View committee trading conflict scores
capitoltraders conflicts --db capitoltraders.db --min-committee-pct 20

# Include donation-trade correlation analysis
capitoltraders conflicts --db capitoltraders.db --include-donations --politician pelosi

# Detect unusual trading patterns
capitoltraders anomalies --db capitoltraders.db --min-score 0.5

# Show pre-move trade signals sorted by volume
capitoltraders anomalies --db capitoltraders.db --show-pre-move --sort-by volume
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
| `--show-donor-context` | Show donation context for traded securities (DB mode only) | off |

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
| `--batch-size` | Maximum trades to enrich per run | all |
| `--force` | Re-enrich already-enriched trades (reserved, not yet active) | off |
| `--diagnose` | Print enrichment diagnostics and exit (no Yahoo API calls) | off |
| `--retry-failed` | Reset trades that were attempted but got no price, then re-enrich | off |

Enrichment runs in three phases: (1) historical trade-date prices fetched per unique (ticker, date) pair,
(2) current prices fetched per unique ticker, and (3) benchmark prices (sector ETF or SPY) per unique
(ETF, date) pair. A ticker alias system (`seed_data/ticker_aliases.yml`) resolves renamed stocks, acquired
companies, and known-unenrichable tickers (money market funds, indices) before calling Yahoo Finance.
When Yahoo returns no data for a ticker (e.g., delisted/acquired companies), the system automatically
falls back to Tiingo for historical prices if a `TIINGO_API_KEY` is configured in `.env`. The fallback
is silently skipped when no key is present.
Trades without valid tickers are marked as processed and skipped on future runs. Rate limiting (200-500ms
jittered delay, max 5 concurrent) prevents Yahoo Finance throttling. A circuit breaker trips after 10
consecutive failures. Progress displays ticker counts and success/fail/skip summary. The `price_source`
column tracks which API provided each price (yahoo or tiingo).

Use `--diagnose` to see a full breakdown including price source distribution, and `--retry-failed` to
re-attempt previously failed tickers (now with Tiingo fallback for delisted equities).

To enable Tiingo fallback for delisted equities, add `TIINGO_API_KEY` to your `.env` file.
Get a free key at [tiingo.com](https://www.tiingo.com/account/api/token) (500 unique symbols/month).
The fallback is optional -- enrichment works without it using Yahoo Finance only.

**portfolio** -- View per-politician stock positions with unrealized P&L.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician ID (e.g. `P000197`) | all |
| `--party` | `democrat` (`d`), `republican` (`r`) | all |
| `--state` | US state code, e.g. `CA`, `TX` | all |
| `--ticker` | Filter by ticker symbol, e.g. `AAPL` | all |
| `--include-closed` | Include positions with near-zero shares | off |
| `--show-donations` | Show donation summary for the politician | off |

Requires a synced and price-enriched database (`sync` then `enrich-prices`).
 Positions are calculated
using FIFO (First-In-First-Out) accounting from estimated share counts. Output columns: Politician,
Ticker, Shares, Avg Cost, Current Price, Current Value, Unrealized P&L, P&L %. Option trades are
excluded from position calculations and noted separately in table/markdown output.

**sync-fec** -- Populates FEC candidate ID mappings.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |

Downloads the `congress-legislators` dataset and matches politicians by name and state to resolve their FEC candidate IDs.

**sync-donations** -- Fetches FEC Schedule A contributions.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician name (partial match) | all |
| `--cycle` | Election cycle year (e.g. 2024) | all |
| `--batch-size` | Donations per API page | 100 |

Requires an `OPENFEC_API_KEY` in your `.env` file. Fetches contributions for all authorized committees associated with the politician's FEC ID. Supports resumable sync via persistent cursors. A sliding-window rate limiter (900 req/hr budget) paces requests proactively, and 429 responses trigger exponential backoff retries (up to 3 attempts). Progress output shows remaining API budget and a post-run summary of request stats.

**donations** -- Query and aggregate synced donation data.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician name | all |
| `--cycle` | Filter by election cycle year | all |
| `--min-amount` | Minimum contribution amount | all |
| `--employer` | Filter by employer name (partial match) | all |
| `--state` | Filter by contributor state | all |
| `--top` | Show top N results | all |
| `--group-by` | Group results by: `contributor`, `employer`, `state` | -- |

**map-employers** -- Build employer-to-issuer mapping database.

| Subcommand | Description |
|---|---|
| `load-seed` | Load curated employer mappings into database |
| `export` | Export unmatched employers with suggestions to CSV |
| `import` | Import confirmed mappings from CSV |

Used to correlate FEC donation employers with stock issuers. `export` uses fuzzy matching to suggest tickers for donor employers.

**analytics** -- View politician performance rankings.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--period` | `ytd`, `1y`, `2y`, `all` | `all` |
| `--min-trades` | Minimum closed trades for inclusion | 5 |
| `--sort-by` | `return`, `win-rate`, `alpha` | `return` |
| `--party` | `democrat` (`d`), `republican` (`r`) | all |
| `--state` | US state code | all |
| `--top` | Number of results | 25 |

**conflicts** -- View committee trading scores and donation-trade correlations.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician name (partial match) | all |
| `--committee` | Filter by committee name (exact match) | all |
| `--min-committee-pct` | Minimum committee trading percentage (0-100) | 0 |
| `--include-donations` | Include donation-trade correlations | off |
| `--min-confidence` | Minimum employer mapping confidence (0.0-1.0) | 0.90 |
| `--top` | Number of results | 25 |

**anomalies** -- Detect unusual trading patterns.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path (required) | -- |
| `--politician` | Filter by politician name (partial match) | all |
| `--min-score` | Minimum composite anomaly score (0.0-1.0) | 0.0 |
| `--min-confidence` | Minimum confidence threshold (0.0-1.0) | 0.0 |
| `--show-pre-move` | Show detailed pre-move trade signals | off |
| `--top` | Number of results | 25 |
| `--sort-by` | `score`, `volume`, `hhi`, `pre-move` | `score` |

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
  capitoltraders_lib/           # library: cache, scraping, db, yahoo, tiingo, pricing, openfec, mapping, analytics, anomaly, conflict
  capitoltraders_cli/           # CLI binary (13 subcommands)
  schema/sqlite.sql             # SQLite schema (v9) with FEC, donation, analytics, and price source tables
  seed_data/                    # GICS sector mappings, committee jurisdictions, employer-issuer mappings, ticker aliases
```

## Development

```sh
# Run all tests (650 total)
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

The `sync` subcommand writes to SQLite using the schema in `schema/sqlite.sql` (currently at v9). Tables map
directly to the CLI JSON output schemas (`schema/*.schema.json`), including nested data:

- `trades`, `assets`, `issuers`, `politicians`
- `trade_committees`, `trade_labels`, `politician_committees`
- `issuer_stats`, `politician_stats`, `issuer_performance`, `issuer_eod_prices`
- `positions` (materialized FIFO portfolio positions per politician per ticker)
- `fec_mappings`, `fec_committees`, `donations`, `donation_sync_meta`
- `employer_mappings`, `employer_lookup`
- `sector_benchmarks` (GICS sector benchmark ETF reference data)
- `ingest_meta` (tracks `last_trade_pub_date` for incremental sync)

The trades table includes price enrichment columns: `trade_date_price`, `current_price`,
`price_enriched_at`, `estimated_shares`, `estimated_value`, `benchmark_price`, `price_source`. These are
populated by `enrich-prices`. The `price_source` column tracks which API provided the price (`yahoo` or
`tiingo`). The issuers table includes `gics_sector` for GICS sector classification.

Incremental runs use `last_trade_pub_date` to request only recent pages from the API, then upsert by
primary key to keep the database current. Enrichment (`--enrich`) populates the join and detail tables
by fetching individual detail pages post-ingest.

## Rate Limiting

This tool uses an unofficial API and adds a randomized 5-10 second delay between HTTP requests to avoid putting unnecessary load on the CapitolTrades servers. Cache hits are not delayed, so repeated queries within the 5-minute cache window return instantly. The first request in a session has no delay. Enrichment uses a configurable delay (default 500ms) between detail page fetches with bounded concurrency (default 3).

OpenFEC API requests (`sync-donations`) use a sliding-window rate limiter that tracks request timestamps and proactively paces calls to stay under the 1,000 req/hr free-tier limit (900 default budget with 10% safety margin). If a 429 response still occurs, individual requests retry with exponential backoff (60s base, doubling, up to 3 retries). A circuit breaker halts the pipeline after 5 consecutive post-retry failures.

## License

This project vendors code from [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). See that repository for its license terms.
