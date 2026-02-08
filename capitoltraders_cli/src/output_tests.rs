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
