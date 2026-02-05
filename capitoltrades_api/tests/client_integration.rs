use capitoltrades_api::{Client, TradeQuery};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{}", name)).unwrap()
}

#[tokio::test]
async fn get_trades_success() {
    let mock_server = MockServer::start().await;
    let body = load_fixture("trades.json");

    Mock::given(method("GET"))
        .and(path("/trades"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].tx_id, 12345);
}

#[tokio::test]
async fn get_trades_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trades"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_trades_malformed_json() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trades"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("{not valid json}"),
        )
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let result = client.get_trades(&TradeQuery::default()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_politicians_success() {
    let mock_server = MockServer::start().await;
    let body = load_fixture("politicians.json");

    Mock::given(method("GET"))
        .and(path("/politicians"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let query = capitoltrades_api::PoliticianQuery::default();
    let result = client.get_politicians(&query).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].first_name, "Nancy");
}

#[tokio::test]
async fn get_issuers_success() {
    let mock_server = MockServer::start().await;
    let body = load_fixture("issuers.json");

    Mock::given(method("GET"))
        .and(path("/issuers"))
        .respond_with(ResponseTemplate::new(200).set_body_string(&body))
        .mount(&mock_server)
        .await;

    let client = Client::with_base_url(&mock_server.uri());
    let query = capitoltrades_api::IssuerQuery::default();
    let result = client.get_issuers(&query).await;
    assert!(result.is_ok());

    let resp = result.unwrap();
    assert_eq!(resp.data.len(), 1);
    assert_eq!(resp.data[0].issuer_name, "Apple Inc");
}
