//! Output formatting for all supported formats: table, JSON, CSV, Markdown, and XML.
//!
//! Each data type (trades, politicians, issuers) has dedicated print functions
//! for each format. Data is first mapped to flat row structs, then rendered.

use anyhow::Result;
use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};
use capitoltraders_lib::{
    ContributorAggRow, DbIssuerRow, DbPoliticianRow, DbTradeRow, DonationRow, EmployerAggRow,
    PortfolioPosition, StateAggRow,
};
use serde::Serialize;
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::xml_output;

/// Supported output formats for CLI results.
#[derive(Clone, Debug)]
pub enum OutputFormat {
    /// ASCII table (default).
    Table,
    /// Pretty-printed JSON array.
    Json,
    /// Comma-separated values with header row.
    Csv,
    /// GitHub-flavored Markdown table.
    Markdown,
    /// Well-formed XML document.
    Xml,
}

/// Flattened row representation of a trade for tabular output.
#[derive(Tabled, Serialize)]
struct TradeRow {
    #[tabled(rename = "Date")]
    #[serde(rename = "Date")]
    tx_date: String,
    #[tabled(rename = "Politician")]
    #[serde(rename = "Politician")]
    politician: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "Issuer")]
    #[serde(rename = "Issuer")]
    issuer: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Type")]
    #[serde(rename = "Type")]
    tx_type: String,
    #[tabled(rename = "Value")]
    #[serde(rename = "Value")]
    value: String,
}

/// Flattened row representation of a politician for tabular output.
#[derive(Tabled, Serialize)]
struct PoliticianRow {
    #[tabled(rename = "Name")]
    #[serde(rename = "Name")]
    name: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Chamber")]
    #[serde(rename = "Chamber")]
    chamber: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    #[serde(rename = "Volume")]
    volume: String,
}

/// Flattened row representation of an issuer for tabular output.
#[derive(Tabled, Serialize)]
struct IssuerRow {
    #[tabled(rename = "Name")]
    #[serde(rename = "Name")]
    name: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Politicians")]
    #[serde(rename = "Politicians")]
    politicians: i64,
    #[tabled(rename = "Volume")]
    #[serde(rename = "Volume")]
    volume: String,
    #[tabled(rename = "Last Traded")]
    #[serde(rename = "Last Traded")]
    last_traded: String,
}

// -- Row builders --

fn build_trade_rows(trades: &[Trade]) -> Vec<TradeRow> {
    trades
        .iter()
        .map(|t| TradeRow {
            tx_date: t.tx_date.to_string(),
            politician: format!("{} {}", t.politician.first_name, t.politician.last_name),
            party: t.politician.party.to_string(),
            issuer: t.issuer.issuer_name.clone(),
            ticker: t.issuer.issuer_ticker.clone().unwrap_or_default(),
            tx_type: serde_json::to_value(t.tx_type)
                .unwrap()
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            value: format_value(t.value),
        })
        .collect()
}

fn build_politician_rows(politicians: &[PoliticianDetail]) -> Vec<PoliticianRow> {
    politicians
        .iter()
        .map(|p| PoliticianRow {
            name: format!("{} {}", p.first_name, p.last_name),
            party: p.party.to_string(),
            state: p.state_id.clone(),
            chamber: serde_json::to_value(p.chamber)
                .unwrap()
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            trades: p.stats.count_trades,
            volume: format_value(p.stats.volume),
        })
        .collect()
}

fn build_issuer_rows(issuers: &[IssuerDetail]) -> Vec<IssuerRow> {
    issuers
        .iter()
        .map(|i| IssuerRow {
            name: i.issuer_name.clone(),
            ticker: i.issuer_ticker.clone().unwrap_or_default(),
            trades: i.stats.count_trades,
            politicians: i.stats.count_politicians,
            volume: format_value(i.stats.volume),
            last_traded: i.stats.date_last_traded.to_string(),
        })
        .collect()
}

// -- Table output --

/// Prints trades as an ASCII table to stdout.
pub fn print_trades_table(trades: &[Trade]) {
    println!("{}", Table::new(build_trade_rows(trades)));
}

/// Prints politicians as an ASCII table to stdout.
pub fn print_politicians_table(politicians: &[PoliticianDetail]) {
    println!("{}", Table::new(build_politician_rows(politicians)));
}

/// Prints issuers as an ASCII table to stdout.
pub fn print_issuers_table(issuers: &[IssuerDetail]) {
    println!("{}", Table::new(build_issuer_rows(issuers)));
}

// -- Markdown output --

/// Prints trades as a GitHub-flavored Markdown table to stdout.
pub fn print_trades_markdown(trades: &[Trade]) {
    let mut table = Table::new(build_trade_rows(trades));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints politicians as a GitHub-flavored Markdown table to stdout.
pub fn print_politicians_markdown(politicians: &[PoliticianDetail]) {
    let mut table = Table::new(build_politician_rows(politicians));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints issuers as a GitHub-flavored Markdown table to stdout.
pub fn print_issuers_markdown(issuers: &[IssuerDetail]) {
    let mut table = Table::new(build_issuer_rows(issuers));
    table.with(Style::markdown());
    println!("{}", table);
}

// -- CSV output --

/// Neutralize CSV formula injection by prefixing dangerous leading characters with a tab.
/// Spreadsheet applications (Excel, Google Sheets) interpret cells starting with =, +, -, or @
/// as formulas. A leading tab prevents formula evaluation while remaining visually unobtrusive.
pub(crate) fn sanitize_csv_field(s: &str) -> String {
    if s.starts_with('=') || s.starts_with('+') || s.starts_with('-') || s.starts_with('@') {
        format!("\t{}", s)
    } else {
        s.to_string()
    }
}

/// Prints trades as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_trades_csv(trades: &[Trade]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_trade_rows(trades) {
        row.politician = sanitize_csv_field(&row.politician);
        row.issuer = sanitize_csv_field(&row.issuer);
        row.ticker = sanitize_csv_field(&row.ticker);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints politicians as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_politicians_csv(politicians: &[PoliticianDetail]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_politician_rows(politicians) {
        row.name = sanitize_csv_field(&row.name);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints issuers as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_issuers_csv(issuers: &[IssuerDetail]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_issuer_rows(issuers) {
        row.name = sanitize_csv_field(&row.name);
        row.ticker = sanitize_csv_field(&row.ticker);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

// -- XML output --

/// Prints trades as a well-formed XML document to stdout.
pub fn print_trades_xml(trades: &[Trade]) {
    println!("{}", xml_output::trades_to_xml(trades));
}

/// Prints politicians as a well-formed XML document to stdout.
pub fn print_politicians_xml(politicians: &[PoliticianDetail]) {
    println!("{}", xml_output::politicians_to_xml(politicians));
}

/// Prints issuers as a well-formed XML document to stdout.
pub fn print_issuers_xml(issuers: &[IssuerDetail]) {
    println!("{}", xml_output::issuers_to_xml(issuers));
}

// -- DB trade output --

/// Flattened row representation of a DB trade for tabular output.
///
/// Includes enriched fields (asset type, committees, labels) not present
/// in the API-based [`TradeRow`].
#[derive(Tabled, Serialize)]
struct DbTradeOutputRow {
    #[tabled(rename = "Date")]
    #[serde(rename = "Date")]
    tx_date: String,
    #[tabled(rename = "Politician")]
    #[serde(rename = "Politician")]
    politician: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "Issuer")]
    #[serde(rename = "Issuer")]
    issuer: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Type")]
    #[serde(rename = "Type")]
    tx_type: String,
    #[tabled(rename = "Asset")]
    #[serde(rename = "Asset")]
    asset_type: String,
    #[tabled(rename = "Value")]
    #[serde(rename = "Value")]
    value: String,
    #[tabled(rename = "Committees")]
    #[serde(rename = "Committees")]
    committees: String,
    #[tabled(rename = "Labels")]
    #[serde(rename = "Labels")]
    labels: String,
}

fn build_db_trade_rows(trades: &[DbTradeRow]) -> Vec<DbTradeOutputRow> {
    trades
        .iter()
        .map(|t| DbTradeOutputRow {
            tx_date: t.tx_date.clone(),
            politician: t.politician_name.clone(),
            party: t.party.clone(),
            issuer: t.issuer_name.clone(),
            ticker: t.issuer_ticker.clone(),
            tx_type: t.tx_type.clone(),
            asset_type: t.asset_type.clone(),
            value: format_value(t.value),
            committees: t.committees.join(", "),
            labels: t.labels.join(", "),
        })
        .collect()
}

/// Prints DB trades as an ASCII table to stdout.
pub fn print_db_trades_table(trades: &[DbTradeRow]) {
    println!("{}", Table::new(build_db_trade_rows(trades)));
}

/// Prints DB trades as a GitHub-flavored Markdown table to stdout.
pub fn print_db_trades_markdown(trades: &[DbTradeRow]) {
    let mut table = Table::new(build_db_trade_rows(trades));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints DB trades as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_db_trades_csv(trades: &[DbTradeRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_db_trade_rows(trades) {
        row.politician = sanitize_csv_field(&row.politician);
        row.issuer = sanitize_csv_field(&row.issuer);
        row.ticker = sanitize_csv_field(&row.ticker);
        row.committees = sanitize_csv_field(&row.committees);
        row.labels = sanitize_csv_field(&row.labels);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints DB trades as a well-formed XML document to stdout.
pub fn print_db_trades_xml(trades: &[DbTradeRow]) {
    println!("{}", xml_output::db_trades_to_xml(trades));
}

// -- DB politician output --

/// Flattened row representation of a DB politician for tabular output.
///
/// Includes committee membership data from the politician_committees table
/// and trade stats from the politician_stats table.
#[derive(Tabled, Serialize)]
struct DbPoliticianOutputRow {
    #[tabled(rename = "Name")]
    #[serde(rename = "Name")]
    name: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Chamber")]
    #[serde(rename = "Chamber")]
    chamber: String,
    #[tabled(rename = "Committees")]
    #[serde(rename = "Committees")]
    committees: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    #[serde(rename = "Volume")]
    volume: String,
}

fn build_db_politician_rows(politicians: &[DbPoliticianRow]) -> Vec<DbPoliticianOutputRow> {
    politicians
        .iter()
        .map(|p| DbPoliticianOutputRow {
            name: p.name.clone(),
            party: p.party.clone(),
            state: p.state.clone(),
            chamber: p.chamber.clone(),
            committees: p.committees.join(", "),
            trades: p.trades,
            volume: format_value(p.volume),
        })
        .collect()
}

/// Prints DB politicians as an ASCII table to stdout.
pub fn print_db_politicians_table(politicians: &[DbPoliticianRow]) {
    println!("{}", Table::new(build_db_politician_rows(politicians)));
}

/// Prints DB politicians as a GitHub-flavored Markdown table to stdout.
pub fn print_db_politicians_markdown(politicians: &[DbPoliticianRow]) {
    let mut table = Table::new(build_db_politician_rows(politicians));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints DB politicians as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_db_politicians_csv(politicians: &[DbPoliticianRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_db_politician_rows(politicians) {
        row.name = sanitize_csv_field(&row.name);
        row.committees = sanitize_csv_field(&row.committees);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints DB politicians as a well-formed XML document to stdout.
pub fn print_db_politicians_xml(politicians: &[DbPoliticianRow]) {
    println!("{}", xml_output::db_politicians_to_xml(politicians));
}

// -- DB issuer output --

/// Flattened row representation of a DB issuer for tabular output.
///
/// Includes performance data (market cap, trailing returns) and trade
/// statistics from issuer_stats and issuer_performance tables.
#[derive(Tabled, Serialize)]
struct DbIssuerOutputRow {
    #[tabled(rename = "Name")]
    #[serde(rename = "Name")]
    name: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Sector")]
    #[serde(rename = "Sector")]
    sector: String,
    #[tabled(rename = "Mcap")]
    #[serde(rename = "Mcap")]
    mcap: String,
    #[tabled(rename = "30D Return")]
    #[serde(rename = "30D Return")]
    trailing30: String,
    #[tabled(rename = "YTD")]
    #[serde(rename = "YTD")]
    trailing365: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    #[serde(rename = "Volume")]
    volume: String,
    #[tabled(rename = "Last Traded")]
    #[serde(rename = "Last Traded")]
    last_traded: String,
}

/// Format a large number with T/B/M suffixes for readability.
fn format_large_number(value: i64) -> String {
    let v = value as f64;
    if v >= 1_000_000_000_000.0 {
        format!("${:.1}T", v / 1_000_000_000_000.0)
    } else if v >= 1_000_000_000.0 {
        format!("${:.1}B", v / 1_000_000_000.0)
    } else if v >= 1_000_000.0 {
        format!("${:.1}M", v / 1_000_000.0)
    } else {
        format!("${}", value)
    }
}

/// Format a trailing return value as a percentage string (e.g., "+2.5%" or "-1.3%").
fn format_percent(value: f64) -> String {
    if value >= 0.0 {
        format!("+{:.1}%", value * 100.0)
    } else {
        format!("{:.1}%", value * 100.0)
    }
}

fn build_db_issuer_rows(issuers: &[DbIssuerRow]) -> Vec<DbIssuerOutputRow> {
    issuers
        .iter()
        .map(|i| DbIssuerOutputRow {
            name: i.issuer_name.clone(),
            ticker: i.issuer_ticker.clone().unwrap_or_else(|| "-".to_string()),
            sector: i.sector.clone().unwrap_or_else(|| "-".to_string()),
            mcap: i
                .mcap
                .map(format_large_number)
                .unwrap_or_else(|| "-".to_string()),
            trailing30: i
                .trailing30_change
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string()),
            trailing365: i
                .trailing365_change
                .map(format_percent)
                .unwrap_or_else(|| "-".to_string()),
            trades: i.trades,
            volume: format_value(i.volume),
            last_traded: i.last_traded.clone().unwrap_or_else(|| "-".to_string()),
        })
        .collect()
}

/// Prints DB issuers as an ASCII table to stdout.
pub fn print_db_issuers_table(issuers: &[DbIssuerRow]) {
    println!("{}", Table::new(build_db_issuer_rows(issuers)));
}

/// Prints DB issuers as a GitHub-flavored Markdown table to stdout.
pub fn print_db_issuers_markdown(issuers: &[DbIssuerRow]) {
    let mut table = Table::new(build_db_issuer_rows(issuers));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints DB issuers as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_db_issuers_csv(issuers: &[DbIssuerRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_db_issuer_rows(issuers) {
        row.name = sanitize_csv_field(&row.name);
        row.ticker = sanitize_csv_field(&row.ticker);
        row.sector = sanitize_csv_field(&row.sector);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints DB issuers as a well-formed XML document to stdout.
pub fn print_db_issuers_xml(issuers: &[DbIssuerRow]) {
    println!("{}", xml_output::db_issuers_to_xml(issuers));
}

// -- Portfolio output --

/// Flattened row representation of a portfolio position for tabular output.
///
/// Includes P&L calculations and current market values from the portfolio table.
#[derive(Tabled, Serialize)]
struct PortfolioRow {
    #[tabled(rename = "Politician")]
    #[serde(rename = "Politician")]
    politician_id: String,
    #[tabled(rename = "Ticker")]
    #[serde(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Shares")]
    #[serde(rename = "Shares")]
    shares_held: String,
    #[tabled(rename = "Avg Cost")]
    #[serde(rename = "Avg Cost")]
    avg_cost_basis: String,
    #[tabled(rename = "Current Price")]
    #[serde(rename = "Current Price")]
    current_price: String,
    #[tabled(rename = "Current Value")]
    #[serde(rename = "Current Value")]
    current_value: String,
    #[tabled(rename = "Unrealized P&L")]
    #[serde(rename = "Unrealized P&L")]
    unrealized_pnl: String,
    #[tabled(rename = "P&L %")]
    #[serde(rename = "P&L %")]
    unrealized_pnl_pct: String,
}

/// Format shares with 2 decimal places.
fn format_shares(shares: f64) -> String {
    format!("{:.2}", shares)
}

/// Format currency with dollar sign and 2 decimal places.
fn format_currency(value: f64) -> String {
    format!("${:.2}", value)
}

/// Format currency with thousand separators.
fn format_currency_with_commas(value: f64) -> String {
    let formatted = format!("{:.2}", value);
    let parts: Vec<&str> = formatted.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = if parts.len() > 1 { parts[1] } else { "00" };

    let mut result = String::new();
    for (i, c) in integer_part.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    format!("${}.{}", result, decimal_part)
}

fn build_portfolio_rows(positions: &[PortfolioPosition]) -> Vec<PortfolioRow> {
    positions
        .iter()
        .map(|p| PortfolioRow {
            politician_id: p.politician_id.clone(),
            ticker: p.ticker.clone(),
            shares_held: format_shares(p.shares_held),
            avg_cost_basis: format_currency(p.cost_basis),
            current_price: p
                .current_price
                .map(format_currency)
                .unwrap_or_else(|| "-".to_string()),
            current_value: p
                .current_value
                .map(format_currency_with_commas)
                .unwrap_or_else(|| "-".to_string()),
            unrealized_pnl: p
                .unrealized_pnl
                .map(|pnl| {
                    if pnl >= 0.0 {
                        format!("+{}", format_currency_with_commas(pnl))
                    } else {
                        format!("-{}", format_currency_with_commas(pnl.abs()))
                    }
                })
                .unwrap_or_else(|| "-".to_string()),
            unrealized_pnl_pct: p
                .unrealized_pnl_pct
                .map(|pct| {
                    if pct >= 0.0 {
                        format!("+{:.1}%", pct)
                    } else {
                        format!("{:.1}%", pct)
                    }
                })
                .unwrap_or_else(|| "-".to_string()),
        })
        .collect()
}

/// Prints portfolio positions as an ASCII table to stdout.
pub fn print_portfolio_table(positions: &[PortfolioPosition]) {
    println!("{}", Table::new(build_portfolio_rows(positions)));
}

/// Prints portfolio positions as a GitHub-flavored Markdown table to stdout.
pub fn print_portfolio_markdown(positions: &[PortfolioPosition]) {
    let mut table = Table::new(build_portfolio_rows(positions));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints portfolio positions as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_portfolio_csv(positions: &[PortfolioPosition]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_portfolio_rows(positions) {
        row.politician_id = sanitize_csv_field(&row.politician_id);
        row.ticker = sanitize_csv_field(&row.ticker);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints portfolio positions as a well-formed XML document to stdout.
pub fn print_portfolio_xml(positions: &[PortfolioPosition]) {
    println!("{}", xml_output::portfolio_to_xml(positions));
}

// -- Donations output --

/// Flattened row representation of a donation for tabular output.
#[derive(Tabled, Serialize, Clone)]
struct DonationOutputRow {
    #[tabled(rename = "Date")]
    #[serde(rename = "Date")]
    date: String,
    #[tabled(rename = "Contributor")]
    #[serde(rename = "Contributor")]
    contributor: String,
    #[tabled(rename = "Employer")]
    #[serde(rename = "Employer")]
    employer: String,
    #[tabled(rename = "Amount")]
    #[serde(rename = "Amount")]
    amount: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Committee")]
    #[serde(rename = "Committee")]
    committee: String,
    #[tabled(rename = "Cycle")]
    #[serde(rename = "Cycle")]
    cycle: String,
}

fn build_donation_rows(donations: &[DonationRow]) -> Vec<DonationOutputRow> {
    donations
        .iter()
        .map(|d| DonationOutputRow {
            date: d.date.clone(),
            contributor: d.contributor_name.clone(),
            employer: if d.contributor_employer.is_empty() {
                "-".to_string()
            } else {
                d.contributor_employer.clone()
            },
            amount: format_currency_with_commas(d.amount),
            state: if d.contributor_state.is_empty() {
                "-".to_string()
            } else {
                d.contributor_state.clone()
            },
            committee: if d.committee_name.is_empty() {
                "-".to_string()
            } else {
                d.committee_name.clone()
            },
            cycle: d.cycle.to_string(),
        })
        .collect()
}

/// Prints donations as an ASCII table to stdout.
pub fn print_donations_table(donations: &[DonationRow]) {
    println!("{}", Table::new(build_donation_rows(donations)));
}

/// Prints donations as a GitHub-flavored Markdown table to stdout.
pub fn print_donations_markdown(donations: &[DonationRow]) {
    let mut table = Table::new(build_donation_rows(donations));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints donations as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_donations_csv(donations: &[DonationRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_donation_rows(donations) {
        row.contributor = sanitize_csv_field(&row.contributor);
        row.employer = sanitize_csv_field(&row.employer);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints donations as a well-formed XML document to stdout.
pub fn print_donations_xml(donations: &[DonationRow]) {
    println!("{}", xml_output::donations_to_xml(donations));
}

// -- Contributor aggregation output --

/// Flattened row representation of contributor aggregation for tabular output.
#[derive(Tabled, Serialize, Clone)]
struct ContributorAggOutputRow {
    #[tabled(rename = "Contributor")]
    #[serde(rename = "Contributor")]
    name: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Total")]
    #[serde(rename = "Total")]
    total: String,
    #[tabled(rename = "Count")]
    #[serde(rename = "Count")]
    count: i64,
    #[tabled(rename = "Avg")]
    #[serde(rename = "Avg")]
    avg: String,
    #[tabled(rename = "Max")]
    #[serde(rename = "Max")]
    max: String,
    #[tabled(rename = "First")]
    #[serde(rename = "First")]
    first_date: String,
    #[tabled(rename = "Last")]
    #[serde(rename = "Last")]
    last_date: String,
}

fn build_contributor_agg_rows(rows: &[ContributorAggRow]) -> Vec<ContributorAggOutputRow> {
    rows.iter()
        .map(|r| ContributorAggOutputRow {
            name: r.contributor_name.clone(),
            state: r.contributor_state.clone(),
            total: format_currency_with_commas(r.total_amount),
            count: r.donation_count,
            avg: format_currency_with_commas(r.avg_amount),
            max: format_currency_with_commas(r.max_donation),
            first_date: r.first_donation.clone(),
            last_date: r.last_donation.clone(),
        })
        .collect()
}

/// Prints contributor aggregations as an ASCII table to stdout.
pub fn print_contributor_agg_table(rows: &[ContributorAggRow]) {
    println!("{}", Table::new(build_contributor_agg_rows(rows)));
}

/// Prints contributor aggregations as a GitHub-flavored Markdown table to stdout.
pub fn print_contributor_agg_markdown(rows: &[ContributorAggRow]) {
    let mut table = Table::new(build_contributor_agg_rows(rows));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints contributor aggregations as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_contributor_agg_csv(rows: &[ContributorAggRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_contributor_agg_rows(rows) {
        row.name = sanitize_csv_field(&row.name);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints contributor aggregations as a well-formed XML document to stdout.
pub fn print_contributor_agg_xml(rows: &[ContributorAggRow]) {
    println!("{}", xml_output::contributor_agg_to_xml(rows));
}

// -- Employer aggregation output --

/// Flattened row representation of employer aggregation for tabular output.
#[derive(Tabled, Serialize, Clone)]
struct EmployerAggOutputRow {
    #[tabled(rename = "Employer")]
    #[serde(rename = "Employer")]
    employer: String,
    #[tabled(rename = "Total")]
    #[serde(rename = "Total")]
    total: String,
    #[tabled(rename = "Count")]
    #[serde(rename = "Count")]
    count: i64,
    #[tabled(rename = "Avg")]
    #[serde(rename = "Avg")]
    avg: String,
    #[tabled(rename = "Contributors")]
    #[serde(rename = "Contributors")]
    contributors: i64,
}

fn build_employer_agg_rows(rows: &[EmployerAggRow]) -> Vec<EmployerAggOutputRow> {
    rows.iter()
        .map(|r| EmployerAggOutputRow {
            employer: r.employer.clone(),
            total: format_currency_with_commas(r.total_amount),
            count: r.donation_count,
            avg: format_currency_with_commas(r.avg_amount),
            contributors: r.contributor_count,
        })
        .collect()
}

/// Prints employer aggregations as an ASCII table to stdout.
pub fn print_employer_agg_table(rows: &[EmployerAggRow]) {
    println!("{}", Table::new(build_employer_agg_rows(rows)));
}

/// Prints employer aggregations as a GitHub-flavored Markdown table to stdout.
pub fn print_employer_agg_markdown(rows: &[EmployerAggRow]) {
    let mut table = Table::new(build_employer_agg_rows(rows));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints employer aggregations as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_employer_agg_csv(rows: &[EmployerAggRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mut row in build_employer_agg_rows(rows) {
        row.employer = sanitize_csv_field(&row.employer);
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints employer aggregations as a well-formed XML document to stdout.
pub fn print_employer_agg_xml(rows: &[EmployerAggRow]) {
    println!("{}", xml_output::employer_agg_to_xml(rows));
}

// -- State aggregation output --

/// Flattened row representation of state aggregation for tabular output.
#[derive(Tabled, Serialize, Clone)]
struct StateAggOutputRow {
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Total")]
    #[serde(rename = "Total")]
    total: String,
    #[tabled(rename = "Count")]
    #[serde(rename = "Count")]
    count: i64,
    #[tabled(rename = "Avg")]
    #[serde(rename = "Avg")]
    avg: String,
    #[tabled(rename = "Contributors")]
    #[serde(rename = "Contributors")]
    contributors: i64,
}

fn build_state_agg_rows(rows: &[StateAggRow]) -> Vec<StateAggOutputRow> {
    rows.iter()
        .map(|r| StateAggOutputRow {
            state: r.state.clone(),
            total: format_currency_with_commas(r.total_amount),
            count: r.donation_count,
            avg: format_currency_with_commas(r.avg_amount),
            contributors: r.contributor_count,
        })
        .collect()
}

/// Prints state aggregations as an ASCII table to stdout.
pub fn print_state_agg_table(rows: &[StateAggRow]) {
    println!("{}", Table::new(build_state_agg_rows(rows)));
}

/// Prints state aggregations as a GitHub-flavored Markdown table to stdout.
pub fn print_state_agg_markdown(rows: &[StateAggRow]) {
    let mut table = Table::new(build_state_agg_rows(rows));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints state aggregations as CSV to stdout.
pub fn print_state_agg_csv(rows: &[StateAggRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for row in build_state_agg_rows(rows) {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints state aggregations as a well-formed XML document to stdout.
pub fn print_state_agg_xml(rows: &[StateAggRow]) {
    println!("{}", xml_output::state_agg_to_xml(rows));
}

// -- Leaderboard output --

use crate::commands::analytics::LeaderboardRow;

/// Flattened row representation of leaderboard for tabular output.
#[derive(Tabled, Serialize, Clone)]
struct LeaderboardOutputRow {
    #[tabled(rename = "#")]
    #[serde(rename = "Rank")]
    rank: usize,
    #[tabled(rename = "Politician")]
    #[serde(rename = "Politician")]
    politician: String,
    #[tabled(rename = "Party")]
    #[serde(rename = "Party")]
    party: String,
    #[tabled(rename = "State")]
    #[serde(rename = "State")]
    state: String,
    #[tabled(rename = "Trades")]
    #[serde(rename = "Trades")]
    trades: usize,
    #[tabled(rename = "Win Rate")]
    #[serde(rename = "WinRate")]
    win_rate: String,
    #[tabled(rename = "Avg Return")]
    #[serde(rename = "AvgReturn")]
    avg_return: String,
    #[tabled(rename = "Alpha")]
    #[serde(rename = "Alpha")]
    alpha: String,
    #[tabled(rename = "Avg Hold")]
    #[serde(rename = "AvgHold")]
    avg_hold: String,
    #[tabled(rename = "Pctl")]
    #[serde(rename = "Percentile")]
    percentile: String,
}

fn build_leaderboard_rows(rows: &[LeaderboardRow]) -> Vec<LeaderboardOutputRow> {
    rows.iter()
        .map(|r| LeaderboardOutputRow {
            rank: r.rank,
            politician: r.politician_name.clone(),
            party: r.party.clone(),
            state: r.state.clone(),
            trades: r.total_trades,
            win_rate: format!("{:.1}%", r.win_rate * 100.0),
            avg_return: if r.avg_return >= 0.0 {
                format!("+{:.1}%", r.avg_return)
            } else {
                format!("{:.1}%", r.avg_return)
            },
            alpha: r.avg_alpha.map(|a| {
                if a >= 0.0 {
                    format!("+{:.1}%", a)
                } else {
                    format!("{:.1}%", a)
                }
            }).unwrap_or_else(|| "N/A".to_string()),
            avg_hold: r.avg_holding_days.map(|d| format!("{:.0} days", d)).unwrap_or_else(|| "N/A".to_string()),
            percentile: format!("{:.0}%", r.percentile * 100.0),
        })
        .collect()
}

/// Prints leaderboard as an ASCII table to stdout.
pub fn print_leaderboard_table(rows: &[LeaderboardRow]) {
    println!("{}", Table::new(build_leaderboard_rows(rows)));
}

/// Prints leaderboard as a GitHub-flavored Markdown table to stdout.
pub fn print_leaderboard_markdown(rows: &[LeaderboardRow]) {
    let mut table = Table::new(build_leaderboard_rows(rows));
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints leaderboard as CSV to stdout. Fields are sanitized against formula injection.
pub fn print_leaderboard_csv(rows: &[LeaderboardRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record([
        "rank",
        "politician",
        "party",
        "state",
        "trades",
        "win_rate",
        "avg_return",
        "alpha",
        "avg_holding_days",
        "percentile",
    ])?;

    for r in rows {
        wtr.write_record(&[
            r.rank.to_string(),
            sanitize_csv_field(&r.politician_name),
            r.party.clone(),
            r.state.clone(),
            r.total_trades.to_string(),
            format!("{:.2}", r.win_rate),
            format!("{:.2}", r.avg_return),
            r.avg_alpha.map(|a| format!("{:.2}", a)).unwrap_or_default(),
            r.avg_holding_days.map(|d| format!("{:.2}", d)).unwrap_or_default(),
            format!("{:.2}", r.percentile),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints leaderboard as a well-formed XML document to stdout.
pub fn print_leaderboard_xml(rows: &[LeaderboardRow]) {
    println!("{}", xml_output::leaderboard_to_xml(rows));
}

// -- JSON output --

/// Prints any serializable data as pretty-printed JSON to stdout.
pub fn print_json<T: serde::Serialize>(data: &T) {
    match serde_json::to_string_pretty(data) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Failed to serialize to JSON: {}", e),
    }
}

/// Formats a dollar value with K/M suffixes for readability.
fn format_value(value: i64) -> String {
    if value >= 1_000_000 {
        format!("${:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("${:.1}K", value as f64 / 1_000.0)
    } else {
        format!("${}", value)
    }
}

// -- Conflict output (committee trading scores) --

/// Prints conflict rows (committee trading scores) as ASCII table to stdout.
pub fn print_conflict_table(rows: &[crate::commands::conflicts::ConflictRow]) {
    use tabled::Tabled;

    #[derive(Tabled)]
    struct ConflictTableRow {
        #[tabled(rename = "Rank")]
        rank: usize,
        #[tabled(rename = "Politician")]
        politician_name: String,
        #[tabled(rename = "Committees")]
        committees: String,
        #[tabled(rename = "Scored Trades")]
        total_scored_trades: usize,
        #[tabled(rename = "Committee Trades")]
        committee_related_trades: usize,
        #[tabled(rename = "Committee %")]
        committee_trading_pct: String,
    }

    let table_rows: Vec<ConflictTableRow> = rows
        .iter()
        .map(|r| ConflictTableRow {
            rank: r.rank,
            politician_name: r.politician_name.clone(),
            committees: r.committees.clone(),
            total_scored_trades: r.total_scored_trades,
            committee_related_trades: r.committee_related_trades,
            committee_trading_pct: format!("{:.1}%", r.committee_trading_pct),
        })
        .collect();

    println!("{}", Table::new(table_rows));
}

/// Prints conflict rows as Markdown table to stdout.
pub fn print_conflict_markdown(rows: &[crate::commands::conflicts::ConflictRow]) {
    use tabled::Tabled;

    #[derive(Tabled)]
    struct ConflictTableRow {
        #[tabled(rename = "Rank")]
        rank: usize,
        #[tabled(rename = "Politician")]
        politician_name: String,
        #[tabled(rename = "Committees")]
        committees: String,
        #[tabled(rename = "Scored Trades")]
        total_scored_trades: usize,
        #[tabled(rename = "Committee Trades")]
        committee_related_trades: usize,
        #[tabled(rename = "Committee %")]
        committee_trading_pct: String,
    }

    let table_rows: Vec<ConflictTableRow> = rows
        .iter()
        .map(|r| ConflictTableRow {
            rank: r.rank,
            politician_name: r.politician_name.clone(),
            committees: r.committees.clone(),
            total_scored_trades: r.total_scored_trades,
            committee_related_trades: r.committee_related_trades,
            committee_trading_pct: format!("{:.1}%", r.committee_trading_pct),
        })
        .collect();

    let mut table = Table::new(table_rows);
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints conflict rows as CSV to stdout.
pub fn print_conflict_csv(rows: &[crate::commands::conflicts::ConflictRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record([
        "rank",
        "politician",
        "committees",
        "total_scored_trades",
        "committee_related_trades",
        "committee_trading_pct",
    ])?;

    for row in rows {
        wtr.write_record(&[
            row.rank.to_string(),
            sanitize_csv_field(&row.politician_name),
            sanitize_csv_field(&row.committees),
            row.total_scored_trades.to_string(),
            row.committee_related_trades.to_string(),
            format!("{:.1}", row.committee_trading_pct),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints conflict rows as XML to stdout.
pub fn print_conflict_xml(rows: &[crate::commands::conflicts::ConflictRow]) {
    println!("{}", xml_output::conflicts_to_xml(rows));
}

// -- Donation correlation output --

/// Prints donation correlation rows as ASCII table to stdout.
pub fn print_donation_correlation_table(rows: &[crate::commands::conflicts::DonationCorrelationRow]) {
    use tabled::Tabled;

    #[derive(Tabled)]
    struct DonationTableRow {
        #[tabled(rename = "Politician")]
        politician_name: String,
        #[tabled(rename = "Ticker")]
        ticker: String,
        #[tabled(rename = "Donors")]
        matching_donors: i64,
        #[tabled(rename = "Total Donations")]
        total_donations: String,
        #[tabled(rename = "Donor Employers")]
        donor_employers: String,
    }

    let table_rows: Vec<DonationTableRow> = rows
        .iter()
        .map(|r| DonationTableRow {
            politician_name: r.politician_name.clone(),
            ticker: r.ticker.clone(),
            matching_donors: r.matching_donors,
            total_donations: format!("${:.2}", r.total_donations),
            donor_employers: r.donor_employers.clone(),
        })
        .collect();

    println!("{}", Table::new(table_rows));
}

/// Prints donation correlation rows as Markdown table to stdout.
pub fn print_donation_correlation_markdown(rows: &[crate::commands::conflicts::DonationCorrelationRow]) {
    use tabled::Tabled;

    #[derive(Tabled)]
    struct DonationTableRow {
        #[tabled(rename = "Politician")]
        politician_name: String,
        #[tabled(rename = "Ticker")]
        ticker: String,
        #[tabled(rename = "Donors")]
        matching_donors: i64,
        #[tabled(rename = "Total Donations")]
        total_donations: String,
        #[tabled(rename = "Donor Employers")]
        donor_employers: String,
    }

    let table_rows: Vec<DonationTableRow> = rows
        .iter()
        .map(|r| DonationTableRow {
            politician_name: r.politician_name.clone(),
            ticker: r.ticker.clone(),
            matching_donors: r.matching_donors,
            total_donations: format!("${:.2}", r.total_donations),
            donor_employers: r.donor_employers.clone(),
        })
        .collect();

    let mut table = Table::new(table_rows);
    table.with(Style::markdown());
    println!("{}", table);
}

/// Prints donation correlation rows as CSV to stdout.
pub fn print_donation_correlation_csv(rows: &[crate::commands::conflicts::DonationCorrelationRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    wtr.write_record([
        "politician",
        "ticker",
        "matching_donors",
        "total_donations",
        "donor_employers",
    ])?;

    for row in rows {
        wtr.write_record(&[
            sanitize_csv_field(&row.politician_name),
            sanitize_csv_field(&row.ticker),
            row.matching_donors.to_string(),
            format!("{:.2}", row.total_donations),
            sanitize_csv_field(&row.donor_employers),
        ])?;
    }
    wtr.flush()?;
    Ok(())
}

/// Prints donation correlation rows as XML to stdout.
pub fn print_donation_correlation_xml(rows: &[crate::commands::conflicts::DonationCorrelationRow]) {
    println!("{}", xml_output::donation_correlations_to_xml(rows));
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
