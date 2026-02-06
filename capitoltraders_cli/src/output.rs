use anyhow::Result;
use serde::Serialize;
use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};
use tabled::settings::Style;
use tabled::{Table, Tabled};

use crate::xml_output;

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Markdown,
    Xml,
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

// -- XML output --

pub fn print_trades_xml(trades: &[Trade]) {
    println!("{}", xml_output::trades_to_xml(trades));
}

pub fn print_politicians_xml(politicians: &[PoliticianDetail]) {
    println!("{}", xml_output::politicians_to_xml(politicians));
}

pub fn print_issuers_xml(issuers: &[IssuerDetail]) {
    println!("{}", xml_output::issuers_to_xml(issuers));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn load_trades_fixture() -> Vec<Trade> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/trades.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    fn load_politicians_fixture() -> Vec<PoliticianDetail> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/politicians.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    fn load_issuers_fixture() -> Vec<IssuerDetail> {
        let json_str = include_str!("../../capitoltrades_api/tests/fixtures/issuers.json");
        let resp: serde_json::Value = serde_json::from_str(json_str).unwrap();
        serde_json::from_value(resp["data"].clone()).unwrap()
    }

    // -- format_value tests --

    #[test]
    fn test_format_value_millions() {
        assert_eq!(format_value(15_000_000), "$15.0M");
    }

    #[test]
    fn test_format_value_thousands() {
        assert_eq!(format_value(50_000), "$50.0K");
    }

    #[test]
    fn test_format_value_small() {
        assert_eq!(format_value(500), "$500");
    }

    #[test]
    fn test_format_value_zero() {
        assert_eq!(format_value(0), "$0");
    }

    // -- Row builder tests --

    #[test]
    fn test_build_trade_rows_mapping() {
        let trades = load_trades_fixture();
        let rows = build_trade_rows(&trades);
        assert_eq!(rows.len(), 1);

        let row = &rows[0];
        assert_eq!(row.tx_date, "2024-03-01");
        assert_eq!(row.politician, "Jane Smith");
        assert_eq!(row.party, "democrat");
        assert_eq!(row.issuer, "Apple Inc");
        assert_eq!(row.ticker, "AAPL");
        assert_eq!(row.tx_type, "buy");
        assert_eq!(row.value, "$50.0K");
    }

    #[test]
    fn test_build_trade_rows_empty() {
        let rows = build_trade_rows(&[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn test_build_politician_rows_mapping() {
        let politicians = load_politicians_fixture();
        let rows = build_politician_rows(&politicians);

        let row = &rows[0];
        assert_eq!(row.name, "Nancy Pelosi");
        assert_eq!(row.party, "democrat");
        assert_eq!(row.state, "CA");
        assert_eq!(row.chamber, "house");
        assert_eq!(row.trades, 250);
        assert_eq!(row.volume, "$15.0M");
    }

    #[test]
    fn test_build_politician_rows_count() {
        let politicians = load_politicians_fixture();
        let rows = build_politician_rows(&politicians);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn test_build_issuer_rows_mapping() {
        let issuers = load_issuers_fixture();
        let rows = build_issuer_rows(&issuers);

        let row = &rows[0];
        assert_eq!(row.name, "Apple Inc");
        assert_eq!(row.ticker, "AAPL");
        assert_eq!(row.trades, 500);
        assert_eq!(row.politicians, 85);
        assert_eq!(row.volume, "$50.0M");
        assert_eq!(row.last_traded, "2024-03-14");
    }

    #[test]
    fn test_build_issuer_rows_empty() {
        let rows = build_issuer_rows(&[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn test_build_issuer_rows_missing_ticker() {
        let json = serde_json::json!([{
            "_issuerId": 9999,
            "_stateId": null,
            "c2iq": null,
            "country": null,
            "issuerName": "Mystery Corp",
            "issuerTicker": null,
            "performance": null,
            "sector": null,
            "stats": {
                "countTrades": 10,
                "countPoliticians": 3,
                "volume": 1000,
                "dateLastTraded": "2024-01-01"
            }
        }]);
        let issuers: Vec<IssuerDetail> = serde_json::from_value(json).unwrap();
        let rows = build_issuer_rows(&issuers);
        assert_eq!(rows[0].ticker, "");
    }

    // -- CSV output tests --

    fn csv_from_rows<T: Serialize>(rows: &[T]) -> String {
        let mut wtr = csv::Writer::from_writer(Vec::new());
        for row in rows {
            wtr.serialize(row).unwrap();
        }
        wtr.flush().unwrap();
        String::from_utf8(wtr.into_inner().unwrap()).unwrap()
    }

    #[test]
    fn test_csv_trades_headers() {
        let trades = load_trades_fixture();
        let rows = build_trade_rows(&trades);
        let csv = csv_from_rows(&rows);
        let header = csv.lines().next().unwrap();
        assert_eq!(header, "Date,Politician,Party,Issuer,Ticker,Type,Value");
    }

    #[test]
    fn test_csv_politicians_headers() {
        let politicians = load_politicians_fixture();
        let rows = build_politician_rows(&politicians);
        let csv = csv_from_rows(&rows);
        let header = csv.lines().next().unwrap();
        assert_eq!(header, "Name,Party,State,Chamber,Trades,Volume");
    }

    #[test]
    fn test_csv_issuers_headers() {
        let issuers = load_issuers_fixture();
        let rows = build_issuer_rows(&issuers);
        let csv = csv_from_rows(&rows);
        let header = csv.lines().next().unwrap();
        assert_eq!(header, "Name,Ticker,Trades,Politicians,Volume,Last Traded");
    }

    // -- JSON output tests --

    #[test]
    fn test_json_trades_serializable() {
        let trades = load_trades_fixture();
        let val = serde_json::to_value(&trades).unwrap();
        assert!(val.is_array());
        assert_eq!(val.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_json_politicians_serializable() {
        let politicians = load_politicians_fixture();
        let val = serde_json::to_value(&politicians).unwrap();
        assert!(val.is_array());
        assert_eq!(val.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_json_issuers_serializable() {
        let issuers = load_issuers_fixture();
        let val = serde_json::to_value(&issuers).unwrap();
        assert!(val.is_array());
        assert_eq!(val.as_array().unwrap().len(), 1);
    }

    // -- Markdown output tests --

    #[test]
    fn test_markdown_trades_structure() {
        let trades = load_trades_fixture();
        let rows = build_trade_rows(&trades);
        let mut table = Table::new(&rows);
        table.with(Style::markdown());
        let md = table.to_string();

        // Should contain pipe chars and separator line
        assert!(md.contains('|'));
        assert!(md.contains("---"));
        // Should contain column headers
        assert!(md.contains("Date"));
        assert!(md.contains("Politician"));
        assert!(md.contains("Value"));
    }

    #[test]
    fn test_markdown_politicians_headers() {
        let politicians = load_politicians_fixture();
        let rows = build_politician_rows(&politicians);
        let mut table = Table::new(&rows);
        table.with(Style::markdown());
        let md = table.to_string();

        let header_line = md.lines().next().unwrap();
        assert!(header_line.contains("Name"));
        assert!(header_line.contains("Party"));
        assert!(header_line.contains("State"));
        assert!(header_line.contains("Chamber"));
    }

    #[test]
    fn test_markdown_empty_produces_headers_only() {
        let rows: Vec<TradeRow> = build_trade_rows(&[]);
        let mut table = Table::new(&rows);
        table.with(Style::markdown());
        let md = table.to_string();

        // Should have header and separator but no data rows
        let lines: Vec<&str> = md.lines().collect();
        // tabled markdown with empty data: header line + separator line = 2 lines
        assert!(lines.len() <= 2, "expected at most 2 lines for empty table, got {}", lines.len());
        if !lines.is_empty() {
            assert!(lines[0].contains("Date"));
        }
    }
}
