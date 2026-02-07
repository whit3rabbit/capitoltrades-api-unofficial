# Capitol Traders

A command-line tool for querying congressional stock trading data from [CapitolTrades](https://www.capitoltrades.com).

Built in Rust on top of a vendored fork of the [capitoltrades_api](https://github.com/TommasoAmici/capitoltrades) crate.

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

# Search trades by politician name (two-step lookup by name)
capitoltraders trades --politician pelosi

# Search trades by issuer name (two-step lookup by name)
capitoltraders trades --issuer nvidia

# Senate Democrats buying stock in the last 30 days
capitoltraders trades --chamber senate --party democrat --tx-type buy --days 30

# Large FAANG trades by female politicians
capitoltraders trades --label faang --gender female --trade-size 7,8,9,10

# Technology sector trades from mega-cap companies
capitoltraders trades --sector information-technology --market-cap mega

# Crypto and memestock trades
capitoltraders trades --label crypto,memestock

# Filter by multiple asset types
capitoltraders trades --asset-type stock,etf --tx-type buy,sell

# Trades from specific committees
capitoltraders trades --committee "Senate - Finance"

# Trades by state
capitoltraders trades --state CA --party republican

# Sort trades by reporting gap (how long after the trade it was disclosed)
capitoltraders trades --sort-by reporting-gap --asc

# List politicians sorted by trade volume
capitoltraders politicians

# Search for a politician by name
capitoltraders politicians --name pelosi

# Republican senators on the Armed Services committee
capitoltraders politicians --party r --state TX --committee ssas

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

# Refresh issuer/politician catalogs during incremental runs
capitoltraders sync --db capitoltraders.db --refresh-issuers --refresh-politicians
```

### Subcommands

**trades** -- List recent congressional stock trades.

| Flag | Description | Default |
|---|---|---|
| `--name` | Search by politician name (broad text search) | -- |
| `--politician` | Filter by politician name (two-step lookup by ID) | -- |
| `--issuer` | Filter by issuer name/ticker (two-step lookup by ID) | -- |
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
| `--page-size` | Results per page | 20 |
| `--sort-by` | `pub-date`, `trade-date`, `reporting-gap` | `pub-date` |
| `--asc` | Sort ascending | descending |

Most filter flags accept comma-separated values for multi-select, e.g. `--asset-type stock,etf` or `--trade-size 7,8,9`.
Date filters are mutually exclusive: use `--days`/`--tx-days` for relative days, or `--since`/`--until` and
`--tx-since`/`--tx-until` for absolute date ranges.

**politicians** -- List politicians and their trading activity.

| Flag | Description | Default |
|---|---|---|
| `--name` | Search by name | -- |
| `--party` | `democrat` (`d`), `republican` (`r`), `other` | all |
| `--state` | US state code | all |
| `--committee` | Committee code or full name | all |
| `--issuer-id` | Filter by issuer ID (numeric), comma-separated | all |
| `--page` | Page number | 1 |
| `--page-size` | Results per page | 20 |
| `--sort-by` | `volume`, `name`, `issuers`, `trades`, `last-traded` | `volume` |
| `--asc` | Sort ascending | descending |

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
| `--page-size` | Results per page | 20 |
| `--sort-by` | `volume`, `politicians`, `trades`, `last-traded`, `mcap` | `volume` |
| `--asc` | Sort ascending | descending |

**sync** -- Ingest CapitolTrades data into SQLite.

| Flag | Description | Default |
|---|---|---|
| `--db` | SQLite database path | `capitoltraders.db` |
| `--full` | Full refresh of trades, politicians, issuers | off |
| `--since` | Override incremental cutoff date (YYYY-MM-DD, pub date) | -- |
| `--refresh-politicians` | Refresh full politician catalog during incremental run | off |
| `--refresh-issuers` | Refresh full issuer catalog during incremental run | off |
| `--page-size` | API page size (1-100) | 100 |

### Global Flags

| Flag | Description | Default |
|---|---|---|
| `--output` | `table`, `json`, `csv`, `md`, or `xml` | `table` |

## Project Structure

```
capitoltraders/
  Cargo.toml                    # workspace root
  capitoltrades_api/            # vendored upstream API client
  capitoltraders_lib/           # library: cache, analysis, validation, error types
  capitoltraders_cli/           # CLI binary
  schema/sqlite.sql             # SQLite schema aligned with CLI JSON output
```

## Development

```sh
# Run all tests (188 total)
cargo test --workspace

# Lint
cargo clippy --workspace

# Run the CLI in dev mode
cargo run -p capitoltraders_cli -- trades --days 7
```

## Data Source

All data comes from the [CapitolTrades](https://www.capitoltrades.com) API. This tool queries their public BFF endpoint. Results are cached in-memory for 5 minutes to reduce API load.

## SQLite

The `sync` subcommand writes to SQLite using the schema in `schema/sqlite.sql`. Tables map directly to
the CLI JSON output schemas (`schema/*.schema.json`), including nested data:

- `trades`, `assets`, `issuers`, `politicians`
- `trade_committees`, `trade_labels`, `politician_committees`
- `issuer_stats`, `politician_stats`, `issuer_performance`, `issuer_eod_prices`
- `ingest_meta` (tracks `last_trade_pub_date` for incremental sync)

Incremental runs use `last_trade_pub_date` to request only recent pages from the API, then upsert by
primary key to keep the database current.

## Rate Limiting

This tool uses an unofficial API and adds a randomized 5-10 second delay between HTTP requests to avoid putting unnecessary load on the CapitolTrades servers. Cache hits are not delayed, so repeated queries within the 5-minute cache window return instantly. The first request in a session has no delay.

## License

This project vendors code from [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). See that repository for its license terms.
