use anyhow::Result;
use serde::Serialize;
use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};
use tabled::settings::Style;
use tabled::{Table, Tabled};

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Markdown,
}

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

pub fn print_trades_table(trades: &[Trade]) {
    println!("{}", Table::new(build_trade_rows(trades)));
}

pub fn print_politicians_table(politicians: &[PoliticianDetail]) {
    println!("{}", Table::new(build_politician_rows(politicians)));
}

pub fn print_issuers_table(issuers: &[IssuerDetail]) {
    println!("{}", Table::new(build_issuer_rows(issuers)));
}

// -- Markdown output --

pub fn print_trades_markdown(trades: &[Trade]) {
    let mut table = Table::new(build_trade_rows(trades));
    table.with(Style::markdown());
    println!("{}", table);
}

pub fn print_politicians_markdown(politicians: &[PoliticianDetail]) {
    let mut table = Table::new(build_politician_rows(politicians));
    table.with(Style::markdown());
    println!("{}", table);
}

pub fn print_issuers_markdown(issuers: &[IssuerDetail]) {
    let mut table = Table::new(build_issuer_rows(issuers));
    table.with(Style::markdown());
    println!("{}", table);
}

// -- CSV output --

pub fn print_trades_csv(trades: &[Trade]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for row in build_trade_rows(trades) {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn print_politicians_csv(politicians: &[PoliticianDetail]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for row in build_politician_rows(politicians) {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

pub fn print_issuers_csv(issuers: &[IssuerDetail]) -> Result<()> {
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for row in build_issuer_rows(issuers) {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

// -- JSON output --

pub fn print_json<T: serde::Serialize>(data: &T) {
    match serde_json::to_string_pretty(data) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Failed to serialize to JSON: {}", e),
    }
}

fn format_value(value: i64) -> String {
    if value >= 1_000_000 {
        format!("${:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("${:.1}K", value as f64 / 1_000.0)
    } else {
        format!("${}", value)
    }
}
