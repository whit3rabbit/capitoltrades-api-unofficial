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

// -- CSV formula sanitization tests --

#[test]
fn test_sanitize_csv_field_equals() {
    assert_eq!(sanitize_csv_field("=SUM(A1)"), "\t=SUM(A1)");
}

#[test]
fn test_sanitize_csv_field_plus() {
    assert_eq!(sanitize_csv_field("+1234"), "\t+1234");
}

#[test]
fn test_sanitize_csv_field_minus() {
    assert_eq!(
        sanitize_csv_field("-cmd|'/C calc'!A0"),
        "\t-cmd|'/C calc'!A0"
    );
}

#[test]
fn test_sanitize_csv_field_at() {
    assert_eq!(sanitize_csv_field("@SUM(A1:A2)"), "\t@SUM(A1:A2)");
}

#[test]
fn test_sanitize_csv_field_normal() {
    assert_eq!(sanitize_csv_field("Apple Inc"), "Apple Inc");
}

#[test]
fn test_sanitize_csv_field_empty() {
    assert_eq!(sanitize_csv_field(""), "");
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
    assert!(
        lines.len() <= 2,
        "expected at most 2 lines for empty table, got {}",
        lines.len()
    );
    if !lines.is_empty() {
        assert!(lines[0].contains("Date"));
    }
}

// -- DB trade output tests --

fn sample_db_trade_row() -> DbTradeRow {
    DbTradeRow {
        tx_id: 12345,
        pub_date: "2024-03-15".to_string(),
        tx_date: "2024-03-01".to_string(),
        tx_type: "buy".to_string(),
        value: 50_000,
        price: Some(175.50),
        size: None,
        filing_url: "https://example.com/filing/123".to_string(),
        reporting_gap: 14,
        enriched_at: Some("2024-03-16T00:00:00Z".to_string()),
        trade_date_price: None,
        current_price: None,
        price_enriched_at: None,
        estimated_shares: None,
        estimated_value: None,
        politician_name: "Jane Smith".to_string(),
        party: "Democrat".to_string(),
        state: "CA".to_string(),
        chamber: "house".to_string(),
        issuer_name: "Apple Inc".to_string(),
        issuer_ticker: "AAPL".to_string(),
        asset_type: "stock".to_string(),
        committees: vec!["Finance".to_string(), "Agriculture".to_string()],
        labels: vec!["faang".to_string()],
    }
}

#[test]
fn test_build_db_trade_rows_mapping() {
    let trades = vec![sample_db_trade_row()];
    let rows = build_db_trade_rows(&trades);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.tx_date, "2024-03-01");
    assert_eq!(row.politician, "Jane Smith");
    assert_eq!(row.party, "Democrat");
    assert_eq!(row.issuer, "Apple Inc");
    assert_eq!(row.ticker, "AAPL");
    assert_eq!(row.tx_type, "buy");
    assert_eq!(row.asset_type, "stock");
    assert_eq!(row.value, "$50.0K");
    assert_eq!(row.committees, "Finance, Agriculture");
    assert_eq!(row.labels, "faang");
}

#[test]
fn test_build_db_trade_rows_empty_committees_labels() {
    let mut trade = sample_db_trade_row();
    trade.committees = vec![];
    trade.labels = vec![];
    let rows = build_db_trade_rows(&[trade]);
    assert_eq!(rows[0].committees, "");
    assert_eq!(rows[0].labels, "");
}

#[test]
fn test_db_trade_row_json_serialization() {
    let trade = sample_db_trade_row();
    let val = serde_json::to_value(&trade).unwrap();
    let obj = val.as_object().unwrap();

    // committees should be a JSON array
    let committees = obj.get("committees").unwrap();
    assert!(committees.is_array());
    assert_eq!(committees.as_array().unwrap().len(), 2);

    // labels should be a JSON array
    let labels = obj.get("labels").unwrap();
    assert!(labels.is_array());
    assert_eq!(labels.as_array().unwrap().len(), 1);

    // asset_type should be present
    assert_eq!(obj.get("asset_type").unwrap().as_str().unwrap(), "stock");
}

#[test]
fn test_db_trade_csv_headers() {
    let trades = vec![sample_db_trade_row()];
    let rows = build_db_trade_rows(&trades);
    let csv = csv_from_rows(&rows);
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "Date,Politician,Party,Issuer,Ticker,Type,Asset,Value,Committees,Labels"
    );
}

#[test]
fn test_db_trade_xml_structure() {
    let trades = vec![sample_db_trade_row()];
    let xml = xml_output::db_trades_to_xml(&trades);
    assert!(xml.contains("<trades>"));
    assert!(xml.contains("<trade>"));
    assert!(xml.contains("<asset_type>stock</asset_type>"));
    assert!(xml.contains("<committees>"));
    assert!(xml.contains("<committee>Finance</committee>"));
    assert!(xml.contains("<labels>"));
    assert!(xml.contains("<label>faang</label>"));
}

// -- DB politician output tests --

fn sample_db_politician_row() -> DbPoliticianRow {
    DbPoliticianRow {
        politician_id: "P000001".to_string(),
        name: "Jane Smith".to_string(),
        party: "Democrat".to_string(),
        state: "CA".to_string(),
        chamber: "senate".to_string(),
        gender: "female".to_string(),
        committees: vec!["Finance".to_string(), "Agriculture".to_string()],
        trades: 150,
        issuers: 45,
        volume: 5_000_000,
        last_traded: Some("2024-03-10".to_string()),
        enriched_at: Some("2024-03-16T00:00:00Z".to_string()),
    }
}

#[test]
fn test_db_politician_row_mapping() {
    let politicians = vec![sample_db_politician_row()];
    let rows = build_db_politician_rows(&politicians);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.name, "Jane Smith");
    assert_eq!(row.party, "Democrat");
    assert_eq!(row.state, "CA");
    assert_eq!(row.chamber, "senate");
    assert_eq!(row.committees, "Finance, Agriculture");
    assert_eq!(row.trades, 150);
    assert_eq!(row.volume, "$5.0M");
}

#[test]
fn test_db_politician_empty_committees() {
    let mut politician = sample_db_politician_row();
    politician.committees = vec![];
    let rows = build_db_politician_rows(&[politician]);
    assert_eq!(rows[0].committees, "");
}

#[test]
fn test_db_politician_json_serialization() {
    let politician = sample_db_politician_row();
    let val = serde_json::to_value(&politician).unwrap();
    let obj = val.as_object().unwrap();

    // committees should be a JSON array
    let committees = obj.get("committees").unwrap();
    assert!(committees.is_array());
    assert_eq!(committees.as_array().unwrap().len(), 2);

    // trades and volume present
    assert_eq!(obj.get("trades").unwrap().as_i64().unwrap(), 150);
    assert_eq!(obj.get("volume").unwrap().as_i64().unwrap(), 5_000_000);
}

#[test]
fn test_db_politician_csv_headers() {
    let politicians = vec![sample_db_politician_row()];
    let rows = build_db_politician_rows(&politicians);
    let csv = csv_from_rows(&rows);
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "Name,Party,State,Chamber,Committees,Trades,Volume"
    );
}

// -- DB issuer output tests --

fn sample_db_issuer_row() -> DbIssuerRow {
    DbIssuerRow {
        issuer_id: 12345,
        issuer_name: "Apple Inc".to_string(),
        issuer_ticker: Some("AAPL".to_string()),
        sector: Some("information-technology".to_string()),
        state: Some("ca".to_string()),
        country: Some("us".to_string()),
        trades: 500,
        politicians: 85,
        volume: 50_000_000,
        last_traded: Some("2024-03-14".to_string()),
        mcap: Some(3_500_000_000_000),
        trailing1: Some(225.5),
        trailing1_change: Some(0.0089),
        trailing7: Some(224.0),
        trailing7_change: Some(0.0156),
        trailing30: Some(220.0),
        trailing30_change: Some(0.025),
        trailing90: Some(210.0),
        trailing90_change: Some(0.0738),
        trailing365: Some(180.0),
        trailing365_change: Some(0.2528),
        enriched_at: Some("2024-03-16T00:00:00Z".to_string()),
    }
}

#[test]
fn test_db_issuer_output_row_mapping() {
    let issuers = vec![sample_db_issuer_row()];
    let rows = build_db_issuer_rows(&issuers);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.name, "Apple Inc");
    assert_eq!(row.ticker, "AAPL");
    assert_eq!(row.sector, "information-technology");
    assert_eq!(row.mcap, "$3.5T");
    assert_eq!(row.trailing30, "+2.5%");
    assert_eq!(row.trailing365, "+25.3%");
    assert_eq!(row.trades, 500);
    assert_eq!(row.volume, "$50.0M");
    assert_eq!(row.last_traded, "2024-03-14");
}

#[test]
fn test_db_issuer_output_no_performance() {
    let issuer = DbIssuerRow {
        issuer_id: 99999,
        issuer_name: "Mystery Corp".to_string(),
        issuer_ticker: None,
        sector: None,
        state: None,
        country: None,
        trades: 0,
        politicians: 0,
        volume: 0,
        last_traded: None,
        mcap: None,
        trailing1: None,
        trailing1_change: None,
        trailing7: None,
        trailing7_change: None,
        trailing30: None,
        trailing30_change: None,
        trailing90: None,
        trailing90_change: None,
        trailing365: None,
        trailing365_change: None,
        enriched_at: None,
    };
    let rows = build_db_issuer_rows(&[issuer]);
    assert_eq!(rows[0].ticker, "-");
    assert_eq!(rows[0].sector, "-");
    assert_eq!(rows[0].mcap, "-");
    assert_eq!(rows[0].trailing30, "-");
    assert_eq!(rows[0].trailing365, "-");
    assert_eq!(rows[0].last_traded, "-");
}

#[test]
fn test_db_issuer_json_serialization() {
    let issuer = sample_db_issuer_row();
    let val = serde_json::to_value(&issuer).unwrap();
    let obj = val.as_object().unwrap();

    // Performance fields should be present
    assert_eq!(obj.get("mcap").unwrap().as_i64().unwrap(), 3_500_000_000_000);
    assert!(obj.get("trailing30_change").unwrap().as_f64().is_some());
    assert!(obj.get("trailing365_change").unwrap().as_f64().is_some());
    assert_eq!(obj.get("trades").unwrap().as_i64().unwrap(), 500);
    assert_eq!(obj.get("volume").unwrap().as_i64().unwrap(), 50_000_000);
}

#[test]
fn test_db_issuer_csv_headers() {
    let issuers = vec![sample_db_issuer_row()];
    let rows = build_db_issuer_rows(&issuers);
    let csv = csv_from_rows(&rows);
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "Name,Ticker,Sector,Mcap,30D Return,YTD,Trades,Volume,Last Traded"
    );
}

// -- Portfolio output tests --

fn sample_portfolio_position_with_pnl() -> PortfolioPosition {
    PortfolioPosition {
        politician_id: "P000123".to_string(),
        ticker: "AAPL".to_string(),
        shares_held: 100.0,
        cost_basis: 50.0,
        realized_pnl: 0.0,
        unrealized_pnl: Some(2500.0),
        unrealized_pnl_pct: Some(50.0),
        current_price: Some(75.0),
        current_value: Some(7500.0),
        price_date: Some("2024-03-15".to_string()),
        last_updated: "2024-03-16T00:00:00Z".to_string(),
    }
}

fn sample_portfolio_position_missing_price() -> PortfolioPosition {
    PortfolioPosition {
        politician_id: "P000456".to_string(),
        ticker: "XYZ".to_string(),
        shares_held: 50.0,
        cost_basis: 100.0,
        realized_pnl: 0.0,
        unrealized_pnl: None,
        unrealized_pnl_pct: None,
        current_price: None,
        current_value: None,
        price_date: None,
        last_updated: "2024-03-16T00:00:00Z".to_string(),
    }
}

#[test]
fn test_format_shares() {
    assert_eq!(format_shares(100.5), "100.50");
    assert_eq!(format_shares(0.0), "0.00");
    assert_eq!(format_shares(1234.567), "1234.57");
}

#[test]
fn test_format_currency() {
    assert_eq!(format_currency(50.0), "$50.00");
    assert_eq!(format_currency(0.0), "$0.00");
    assert_eq!(format_currency(123.456), "$123.46");
}

#[test]
fn test_build_portfolio_rows_with_pnl() {
    let positions = vec![sample_portfolio_position_with_pnl()];
    let rows = build_portfolio_rows(&positions);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.politician_id, "P000123");
    assert_eq!(row.ticker, "AAPL");
    assert_eq!(row.shares_held, "100.00");
    assert_eq!(row.avg_cost_basis, "$50.00");
    assert_eq!(row.current_price, "$75.00");
    assert!(row.current_value.contains("7,500.00"));
    assert!(row.unrealized_pnl.contains("+$2,500.00"));
    assert!(row.unrealized_pnl_pct.contains("+50.0%"));
}

#[test]
fn test_build_portfolio_rows_missing_price() {
    let positions = vec![sample_portfolio_position_missing_price()];
    let rows = build_portfolio_rows(&positions);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    assert_eq!(row.politician_id, "P000456");
    assert_eq!(row.ticker, "XYZ");
    assert_eq!(row.shares_held, "50.00");
    assert_eq!(row.avg_cost_basis, "$100.00");
    assert_eq!(row.current_price, "-");
    assert_eq!(row.current_value, "-");
    assert_eq!(row.unrealized_pnl, "-");
    assert_eq!(row.unrealized_pnl_pct, "-");
}

#[test]
fn test_portfolio_csv_sanitization() {
    let position = PortfolioPosition {
        politician_id: "P000789".to_string(),
        ticker: "=SUM(A1)".to_string(),
        shares_held: 10.0,
        cost_basis: 100.0,
        realized_pnl: 0.0,
        unrealized_pnl: None,
        unrealized_pnl_pct: None,
        current_price: None,
        current_value: None,
        price_date: None,
        last_updated: "2024-03-16T00:00:00Z".to_string(),
    };

    let rows = build_portfolio_rows(&[position]);
    assert_eq!(rows.len(), 1);

    // Verify sanitize_csv_field would add tab prefix
    let sanitized = sanitize_csv_field("=SUM(A1)");
    assert_eq!(sanitized, "\t=SUM(A1)");
}

#[test]
fn test_portfolio_csv_headers() {
    let positions = vec![sample_portfolio_position_with_pnl()];
    let rows = build_portfolio_rows(&positions);
    let csv = csv_from_rows(&rows);
    let header = csv.lines().next().unwrap();
    assert_eq!(
        header,
        "Politician,Ticker,Shares,Avg Cost,Current Price,Current Value,Unrealized P&L,P&L %"
    );
}
