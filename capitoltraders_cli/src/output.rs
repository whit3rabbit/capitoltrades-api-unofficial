use capitoltraders_lib::types::{IssuerDetail, PoliticianDetail, Trade};
use tabled::{Table, Tabled};

#[derive(Clone, Debug)]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Tabled)]
struct TradeRow {
    #[tabled(rename = "Date")]
    tx_date: String,
    #[tabled(rename = "Politician")]
    politician: String,
    #[tabled(rename = "Party")]
    party: String,
    #[tabled(rename = "Issuer")]
    issuer: String,
    #[tabled(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Type")]
    tx_type: String,
    #[tabled(rename = "Value")]
    value: String,
}

#[derive(Tabled)]
struct PoliticianRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Party")]
    party: String,
    #[tabled(rename = "State")]
    state: String,
    #[tabled(rename = "Chamber")]
    chamber: String,
    #[tabled(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Volume")]
    volume: String,
}

#[derive(Tabled)]
struct IssuerRow {
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Trades")]
    trades: i64,
    #[tabled(rename = "Politicians")]
    politicians: i64,
    #[tabled(rename = "Volume")]
    volume: String,
    #[tabled(rename = "Last Traded")]
    last_traded: String,
}

pub fn print_trades_table(trades: &[Trade]) {
    let rows: Vec<TradeRow> = trades
        .iter()
        .map(|t| TradeRow {
            tx_date: t.tx_date.to_string(),
            politician: format!("{} {}", t.politician.first_name, t.politician.last_name),
            party: t.politician.party.to_string(),
            issuer: t.issuer.issuer_name.clone(),
            ticker: t.issuer.issuer_ticker.clone().unwrap_or_default(),
            tx_type: serde_json::to_value(&t.tx_type).unwrap().as_str().unwrap_or("unknown").to_string(),
            value: format_value(t.value),
        })
        .collect();
    println!("{}", Table::new(rows));
}

pub fn print_politicians_table(politicians: &[PoliticianDetail]) {
    let rows: Vec<PoliticianRow> = politicians
        .iter()
        .map(|p| PoliticianRow {
            name: format!("{} {}", p.first_name, p.last_name),
            party: p.party.to_string(),
            state: p.state_id.clone(),
            chamber: serde_json::to_value(&p.chamber)
                .unwrap()
                .as_str()
                .unwrap_or("unknown")
                .to_string(),
            trades: p.stats.count_trades,
            volume: format_value(p.stats.volume),
        })
        .collect();
    println!("{}", Table::new(rows));
}

pub fn print_issuers_table(issuers: &[IssuerDetail]) {
    let rows: Vec<IssuerRow> = issuers
        .iter()
        .map(|i| IssuerRow {
            name: i.issuer_name.clone(),
            ticker: i.issuer_ticker.clone().unwrap_or_default(),
            trades: i.stats.count_trades,
            politicians: i.stats.count_politicians,
            volume: format_value(i.stats.volume),
            last_traded: i.stats.date_last_traded.to_string(),
        })
        .collect();
    println!("{}", Table::new(rows));
}

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
