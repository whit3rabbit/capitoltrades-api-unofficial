use capitoltrades_api::types::{IssuerDetail, PaginatedResponse, PoliticianDetail, Trade};

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

#[test]
fn deserialize_trades_full() {
    let json = load_fixture("trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.meta.paging.page, 1);
    assert_eq!(resp.meta.paging.total_items, 150);

    let trade = &resp.data[0];
    assert_eq!(trade.tx_id, 12345);
    assert_eq!(trade.politician_id, "P000197");
    assert_eq!(trade.issuer_id, 5678);
    assert_eq!(trade.price, Some(150.25));
    assert_eq!(trade.size, Some(50000));
    assert_eq!(trade.value, 50000);
    assert_eq!(trade.reporting_gap, 14);
    assert_eq!(trade.filing_url, "https://efts.sec.gov/LATEST/search-index?q=12345");
    assert_eq!(trade.asset.asset_type, "stock");
    assert_eq!(trade.asset.asset_ticker.as_deref(), Some("AAPL"));
    assert_eq!(trade.issuer.issuer_name, "Apple Inc");
    assert_eq!(trade.politician.first_name, "Jane");
    assert_eq!(trade.politician.last_name, "Smith");
}

#[test]
fn deserialize_trades_empty() {
    let json = load_fixture("trades_minimal.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();
    assert!(resp.data.is_empty());
    assert_eq!(resp.meta.paging.total_items, 0);
    assert_eq!(resp.meta.paging.total_pages, 0);
}

#[test]
fn deserialize_politicians() {
    let json = load_fixture("politicians.json");
    let resp: PaginatedResponse<PoliticianDetail> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.meta.paging.total_items, 535);

    let pelosi = &resp.data[0];
    assert_eq!(pelosi.politician_id, "P000197");
    assert_eq!(pelosi.first_name, "Nancy");
    assert_eq!(pelosi.last_name, "Pelosi");
    assert_eq!(pelosi.state_id, "CA");
    assert_eq!(pelosi.stats.count_trades, 250);
    assert_eq!(pelosi.stats.volume, 15000000);

    let mchenry = &resp.data[1];
    assert_eq!(mchenry.politician_id, "T000250");
    assert_eq!(mchenry.stats.count_issuers, 30);
}

#[test]
fn deserialize_issuers() {
    let json = load_fixture("issuers.json");
    let resp: PaginatedResponse<IssuerDetail> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.data.len(), 1);

    let apple = &resp.data[0];
    assert_eq!(apple.issuer_id, 5678);
    assert_eq!(apple.issuer_name, "Apple Inc");
    assert_eq!(apple.issuer_ticker.as_deref(), Some("AAPL"));
    assert!(apple.performance.is_some());

    let perf = apple.performance.as_ref().unwrap();
    assert_eq!(perf.mcap, 2800000000000);
    assert!(perf.trailing1 > 0.0);
    assert!(perf.last_price().is_some());

    assert_eq!(apple.stats.count_trades, 500);
    assert_eq!(apple.stats.count_politicians, 85);
}

#[test]
fn deserialize_paginated_meta() {
    let json = load_fixture("trades.json");
    let resp: PaginatedResponse<Trade> = serde_json::from_str(&json).unwrap();
    assert_eq!(resp.meta.paging.page, 1);
    assert_eq!(resp.meta.paging.size, 10);
    assert_eq!(resp.meta.paging.total_items, 150);
    assert_eq!(resp.meta.paging.total_pages, 15);
}

#[test]
fn deserialize_malformed_json_returns_error() {
    let bad_json = r#"{"data": not valid json}"#;
    let result = serde_json::from_str::<PaginatedResponse<Trade>>(bad_json);
    assert!(result.is_err());
}

#[test]
fn deserialize_missing_required_fields_returns_error() {
    let json = r#"{"meta": {"paging": {"page": 1}}}"#;
    let result = serde_json::from_str::<PaginatedResponse<Trade>>(json);
    assert!(result.is_err());
}
