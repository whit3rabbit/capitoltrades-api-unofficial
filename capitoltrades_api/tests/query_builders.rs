use capitoltrades_api::{
    IssuerQuery, IssuerSortBy, PoliticianQuery, PoliticianSortBy, Query, SortDirection,
    TradeQuery, TradeSortBy,
};
use capitoltrades_api::types::{
    AssetType, Chamber, Gender, Label, MarketCap, Sector, TradeSize, TxType,
};
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

// -- New tests for filter params --

#[test]
fn trade_query_with_party_and_state() {
    use capitoltrades_api::types::Party;
    let url = TradeQuery::default()
        .with_party(&Party::Democrat)
        .with_state("CA")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("party=democrat"));
    assert!(query.contains("state=CA"));
}

#[test]
fn trade_query_with_committee() {
    let url = TradeQuery::default()
        .with_committee("Senate - Finance")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("committee=Senate+-+Finance") || query.contains("committee=Senate%20-%20Finance"));
}

#[test]
fn trade_query_with_search() {
    let url = TradeQuery::default()
        .with_search("pelosi")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("search=pelosi"));
}

#[test]
fn trade_query_combined_filters() {
    use capitoltrades_api::types::Party;
    let url = TradeQuery::default()
        .with_party(&Party::Republican)
        .with_state("TX")
        .with_committee("House - Armed Services")
        .with_search("cruz")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("party=republican"));
    assert!(query.contains("state=TX"));
    assert!(query.contains("search=cruz"));
    assert!(query.contains("committee="));
}

#[test]
fn trade_query_multiple_parties_and_states() {
    use capitoltrades_api::types::Party;
    let url = TradeQuery::default()
        .with_parties(&[Party::Democrat, Party::Republican])
        .with_states(&["CA".to_string(), "NY".to_string()])
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("party=democrat"));
    assert!(query.contains("party=republican"));
    assert!(query.contains("state=CA"));
    assert!(query.contains("state=NY"));
}

#[test]
fn politician_query_with_state() {
    let url = PoliticianQuery::default()
        .with_state("CA")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("state=CA"));
}

#[test]
fn politician_query_with_committee() {
    let url = PoliticianQuery::default()
        .with_committee("Senate - Finance")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("committee="));
}

#[test]
fn politician_query_combined_new_filters() {
    use capitoltrades_api::types::Party;
    let url = PoliticianQuery::default()
        .with_party(&Party::Democrat)
        .with_state("NY")
        .with_committee("House - Judiciary")
        .with_search("schumer")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("party=democrat"));
    assert!(query.contains("state=NY"));
    assert!(query.contains("committee="));
    assert!(query.contains("search=schumer"));
}

#[test]
fn issuer_query_with_state() {
    let url = IssuerQuery::default()
        .with_state("CA")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("state=CA"));
}

// -- New filter tests for TradeQuery --

#[test]
fn trade_query_with_gender() {
    let url = TradeQuery::default()
        .with_gender(Gender::Female)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("gender=female"));
}

#[test]
fn trade_query_with_market_cap() {
    let url = TradeQuery::default()
        .with_market_cap(MarketCap::Mega)
        .with_market_cap(MarketCap::Large)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("mcap=1"));
    assert!(query.contains("mcap=2"));
}

#[test]
fn trade_query_with_asset_type() {
    let url = TradeQuery::default()
        .with_asset_type(AssetType::Stock)
        .with_asset_type(AssetType::Etf)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("assetType=stock"));
    assert!(query.contains("assetType=etf"));
}

#[test]
fn trade_query_with_label() {
    let url = TradeQuery::default()
        .with_label(Label::Faang)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("label=faang"));
}

#[test]
fn trade_query_with_sector() {
    let url = TradeQuery::default()
        .with_sector(Sector::InformationTechnology)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("sector=information-technology"));
}

#[test]
fn trade_query_with_tx_type() {
    let url = TradeQuery::default()
        .with_tx_type(TxType::Buy)
        .with_tx_type(TxType::Sell)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("txType=buy"));
    assert!(query.contains("txType=sell"));
}

#[test]
fn trade_query_with_chamber() {
    let url = TradeQuery::default()
        .with_chamber(Chamber::Senate)
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("chamber=senate"));
}

#[test]
fn trade_query_with_politician_id() {
    let url = TradeQuery::default()
        .with_politician_id("P000197")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("politician=P000197"));
}

#[test]
fn trade_query_with_issuer_state() {
    let url = TradeQuery::default()
        .with_issuer_state("ca")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("issuerState=ca"));
}

#[test]
fn trade_query_with_country() {
    let url = TradeQuery::default()
        .with_country("us")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("country=us"));
}

#[test]
fn trade_query_combined_new_filters() {
    let url = TradeQuery::default()
        .with_gender(Gender::Female)
        .with_chamber(Chamber::Senate)
        .with_asset_type(AssetType::Stock)
        .with_tx_type(TxType::Buy)
        .with_label(Label::Faang)
        .with_sector(Sector::Financials)
        .with_market_cap(MarketCap::Mega)
        .with_politician_id("P000197")
        .with_issuer_state("ca")
        .with_country("us")
        .add_to_url(&base_url());
    let query = url.query().unwrap();
    assert!(query.contains("gender=female"));
    assert!(query.contains("chamber=senate"));
    assert!(query.contains("assetType=stock"));
    assert!(query.contains("txType=buy"));
    assert!(query.contains("label=faang"));
    assert!(query.contains("sector=financials"));
    assert!(query.contains("mcap=1"));
    assert!(query.contains("politician=P000197"));
    assert!(query.contains("issuerState=ca"));
    assert!(query.contains("country=us"));
}
