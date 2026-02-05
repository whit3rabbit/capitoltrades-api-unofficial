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

# Filter trades from the last 7 days
capitoltraders trades --days 7

# Filter trades by issuer
capitoltraders trades --issuer-id 5678

# Sort trades by reporting gap, ascending
capitoltraders trades --sort-by reporting-gap --asc

# List politicians sorted by trade volume
capitoltraders politicians

# Search for a politician by name
capitoltraders politicians --search pelosi

# Filter politicians by party
capitoltraders politicians --party democrat

# List issuers in the technology sector
capitoltraders issuers --sector information-technology

# Look up a single issuer by ID
capitoltraders issuers --id 5678

# Output as JSON instead of a table
capitoltraders trades --output json
```

### Subcommands

**trades** -- List recent congressional stock trades.

| Flag | Description | Default |
|---|---|---|
| `--issuer-id` | Filter by issuer ID | -- |
| `--days` | Trades from last N days | all |
| `--page` | Page number | 1 |
| `--page-size` | Results per page | 20 |
| `--sort-by` | `pub-date`, `trade-date`, `reporting-gap` | `pub-date` |
| `--asc` | Sort ascending | descending |

**politicians** -- List politicians and their trading activity.

| Flag | Description | Default |
|---|---|---|
| `--party` | `democrat`, `republican`, `other` (or `d`, `r`) | all |
| `--search` | Search by name | -- |
| `--page` | Page number | 1 |
| `--page-size` | Results per page | 20 |
| `--sort-by` | `volume`, `name`, `issuers`, `trades`, `last-traded` | `volume` |
| `--asc` | Sort ascending | descending |

**issuers** -- List or look up stock issuers.

| Flag | Description | Default |
|---|---|---|
| `--id` | Look up a single issuer by ID | -- |
| `--sector` | Filter by sector (e.g. `financials`, `health-care`) | all |
| `--market-cap` | `mega`, `large`, `mid`, `small`, `micro`, `nano` | all |
| `--search` | Search by name | -- |
| `--page` | Page number | 1 |
| `--page-size` | Results per page | 20 |
| `--sort-by` | `volume`, `politicians`, `trades`, `last-traded`, `mcap` | `volume` |
| `--asc` | Sort ascending | descending |

### Global Flags

| Flag | Description | Default |
|---|---|---|
| `--output` | `table` or `json` | `table` |

## Project Structure

```
capitoltraders/
  Cargo.toml                    # workspace root
  capitoltrades_api/            # vendored upstream API client
  capitoltraders_lib/           # library: cache, analysis, error types
  capitoltraders_cli/           # CLI binary
```

## Development

```sh
# Run all tests (36 total)
cargo test --workspace

# Lint
cargo clippy --workspace

# Run the CLI in dev mode
cargo run -p capitoltraders_cli -- trades --days 7
```

## Data Source

All data comes from the [CapitolTrades](https://www.capitoltrades.com) API. This tool queries their public BFF endpoint. Results are cached in-memory for 5 minutes to reduce API load.

## License

This project vendors code from [TommasoAmici/capitoltrades](https://github.com/TommasoAmici/capitoltrades). See that repository for its license terms.
