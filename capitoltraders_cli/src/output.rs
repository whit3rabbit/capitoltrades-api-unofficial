//! Output formatting for all supported formats: table, JSON, CSV, Markdown, and XML.
//!
//! Each data type (trades, politicians, issuers) has dedicated print functions
//! for each format. Data is first mapped to flat row structs, then rendered.

use anyhow::Result;
use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};
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
fn sanitize_csv_field(s: &str) -> String {
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

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
