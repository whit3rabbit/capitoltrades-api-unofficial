use capitoltrades_api::{
    IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy, Query, SortDirection,
    TradeQuery, TradeSortBy,
};
use capitoltrades_api::types::TradeSize;
use url::Url;

fn base_url() -> Url {
    Url::parse("https://example.com").unwrap()
}

#[test]
fn trade_query_defaults() {
    let url = TradeQuery::default().add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("page=1"));
    assert!(query.contains("sortBy=-pubDate"));
}

#[test]
fn trade_query_with_issuer_ids_and_sizes() {
    let url = TradeQuery::default()
        .with_issuer_id(100)
        .with_issuer_id(200)
        .with_trade_size(TradeSize::From100Kto250K)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("issuer=100"));
    assert!(query.contains("issuer=200"));
    assert!(query.contains("tradeSize=5"));
}

#[test]
fn trade_query_sort_variants() {
    let url = TradeQuery::default()
        .with_sort_by(TradeSortBy::TradeDate)
        .with_sort_direction(SortDirection::Asc)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sortBy=txDate"));

    let url = TradeQuery::default()
        .with_sort_by(TradeSortBy::ReportingGap)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sortBy=-reportingGap"));
}

#[test]
fn trade_query_with_date_filters() {
    let url = TradeQuery::default()
        .with_pub_date_relative(7)
        .with_tx_date_relative(30)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("pubDate=7d"));
    assert!(query.contains("txDate=30d"));
}

#[test]
fn trade_query_with_page_and_size() {
    let url = TradeQuery::default()
        .with_page(3)
        .with_page_size(50)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("page=3"));
    assert!(query.contains("pageSize=50"));
}

#[test]
fn politician_query_defaults() {
    let url = PoliticianQuery::default().add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("page=1"));
    assert!(query.contains("sortBy=-volume"));
}

#[test]
fn politician_query_with_parties_and_search() {
    use capitoltrades_api::types::Party;
    let url = PoliticianQuery::default()
        .with_party(&Party::Democrat)
        .with_party(&Party::Republican)
        .with_search("pelosi")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("party=democrat"));
    assert!(query.contains("party=republican"));
    assert!(query.contains("search=pelosi"));
}

#[test]
fn politician_query_sort_variants() {
    let url = PoliticianQuery::default()
        .with_sort_by(PoliticianSortBy::LastName)
        .with_sort_direction(SortDirection::Asc)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sortBy=lastName"));
}

#[test]
fn issuer_query_defaults() {
    let url = IssuerQuery::default().add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sortBy=-volume"));
}

#[test]
fn issuer_query_with_filters() {
    use capitoltrades_api::types::{MarketCap, Sector};
    let url = IssuerQuery::default()
        .with_search("apple")
        .with_sector(Sector::InformationTechnology)
        .with_market_cap(MarketCap::Mega)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("search=apple"));
    assert!(query.contains("sector=information-technology"));
    assert!(query.contains("mcap=1"));
}

#[test]
fn issuer_query_sort_variants() {
    let url = IssuerQuery::default()
        .with_sort_by(IssuerSortBy::MarketCap)
        .with_sort_direction(SortDirection::Asc)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sortBy=mcap"));
}
