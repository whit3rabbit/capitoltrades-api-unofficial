use capitoltraders_lib::openfec::client::OpenFecClient;
use capitoltraders_lib::openfec::error::OpenFecError;
use capitoltraders_lib::openfec::types::{
    CandidateSearchQuery, CandidateSearchResponse, CommitteeResponse, ScheduleAQuery,
    ScheduleAResponse,
};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ============================================================================
// Deserialization Tests - Validate fixtures parse into typed structs
// ============================================================================

#[test]
fn deserialize_candidates_fixture() {
    let fixture = include_str!("fixtures/openfec_candidates.json");
    let response: CandidateSearchResponse = serde_json::from_str(fixture).unwrap();

    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].candidate_id, "H8CA05035");
    assert_eq!(response.results[0].name, "PELOSI, NANCY");
    assert_eq!(response.results[0].party.as_deref(), Some("DEM"));
    assert_eq!(response.results[0].office.as_deref(), Some("H"));
    assert_eq!(response.results[0].state.as_deref(), Some("CA"));
    assert_eq!(response.results[0].district.as_deref(), Some("11"));
    assert_eq!(response.results[0].cycles, vec![2020, 2022, 2024]);
    assert_eq!(response.pagination.count, 1);
}

#[test]
fn deserialize_committees_fixture() {
    let fixture = include_str!("fixtures/openfec_committees.json");
    let response: CommitteeResponse = serde_json::from_str(fixture).unwrap();

    assert_eq!(response.results.len(), 2);
    assert_eq!(response.results[0].committee_id, "C00345777");
    assert_eq!(response.results[0].name, "NANCY PELOSI FOR CONGRESS");
    assert_eq!(response.results[0].committee_type.as_deref(), Some("H"));
    assert_eq!(response.results[1].committee_id, "C00410118");
    assert_eq!(response.results[1].name, "PAC TO THE FUTURE");
    assert_eq!(response.pagination.count, 2);
}

#[test]
fn deserialize_schedule_a_page1_fixture() {
    let fixture = include_str!("fixtures/openfec_schedule_a.json");
    let response: ScheduleAResponse = serde_json::from_str(fixture).unwrap();

    assert_eq!(response.results.len(), 2);
    assert_eq!(
        response.results[0].contribution_receipt_amount,
        Some(2800.0)
    );
    assert_eq!(
        response.results[0].contributor_name.as_deref(),
        Some("SMITH, JOHN")
    );
    assert!(response.pagination.last_indexes.is_some());
    let cursor = response.pagination.last_indexes.as_ref().unwrap();
    assert_eq!(cursor.last_index, 230880619);
    assert_eq!(cursor.last_contribution_receipt_date, "2024-03-14");
}

#[test]
fn deserialize_schedule_a_page2_fixture() {
    let fixture = include_str!("fixtures/openfec_schedule_a_page2.json");
    let response: ScheduleAResponse = serde_json::from_str(fixture).unwrap();

    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].contribution_receipt_amount, Some(500.0));
    assert!(response.pagination.last_indexes.is_none());
}

// ============================================================================
// Candidate Search Tests
// ============================================================================

#[tokio::test]
async fn candidate_search_success() {
    let mock_server = MockServer::start().await;
    let fixture = include_str!("fixtures/openfec_candidates.json");

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default().with_name("Pelosi");
    let result = client.search_candidates(&query).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].candidate_id, "H8CA05035");
    assert_eq!(response.results[0].name, "PELOSI, NANCY");
}

#[tokio::test]
async fn candidate_search_rate_limited() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default().with_name("Pelosi");
    let result = client.search_candidates(&query).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::RateLimited));
}

#[tokio::test]
async fn candidate_search_invalid_key() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "invalid-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default().with_name("Pelosi");
    let result = client.search_candidates(&query).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::InvalidApiKey));
}

#[tokio::test]
async fn candidate_search_malformed_json() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not valid json}"))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default().with_name("Pelosi");
    let result = client.search_candidates(&query).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::ParseFailed(_)));
}

#[tokio::test]
async fn candidate_search_sends_query_params() {
    let mock_server = MockServer::start().await;
    let fixture = include_str!("fixtures/openfec_candidates.json");

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .and(query_param("name", "Pelosi"))
        .and(query_param("state", "CA"))
        .and(query_param("office", "H"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default()
        .with_name("Pelosi")
        .with_state("CA")
        .with_office("H");
    let result = client.search_candidates(&query).await;

    assert!(result.is_ok());
}

// ============================================================================
// Committee Lookup Tests
// ============================================================================

#[tokio::test]
async fn get_committees_success() {
    let mock_server = MockServer::start().await;
    let fixture = include_str!("fixtures/openfec_committees.json");

    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05035/committees/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let result = client.get_candidate_committees("H8CA05035").await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.results.len(), 2);
    assert_eq!(response.results[0].committee_id, "C00345777");
    assert_eq!(response.results[0].name, "NANCY PELOSI FOR CONGRESS");
    assert_eq!(response.results[1].committee_id, "C00410118");
}

#[tokio::test]
async fn get_committees_rate_limited() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05035/committees/"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let result = client.get_candidate_committees("H8CA05035").await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::RateLimited));
}

// ============================================================================
// Schedule A Tests
// ============================================================================

#[tokio::test]
async fn schedule_a_success() {
    let mock_server = MockServer::start().await;
    let fixture = include_str!("fixtures/openfec_schedule_a.json");

    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = ScheduleAQuery::default().with_committee_id("C00345777");
    let result = client.get_schedule_a(&query).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.results.len(), 2);
    assert!(response.pagination.last_indexes.is_some());
    let cursor = response.pagination.last_indexes.as_ref().unwrap();
    assert_eq!(cursor.last_index, 230880619);
    assert_eq!(cursor.last_contribution_receipt_date, "2024-03-14");
}

#[tokio::test]
async fn schedule_a_keyset_pagination() {
    let mock_server = MockServer::start().await;
    let fixture_page1 = include_str!("fixtures/openfec_schedule_a.json");
    let fixture_page2 = include_str!("fixtures/openfec_schedule_a_page2.json");

    // Mount more specific mock first: with cursor parameters - returns page 2 with null cursor
    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a/"))
        .and(query_param("committee_id", "C00345777"))
        .and(query_param("last_index", "230880619"))
        .and(query_param("last_contribution_receipt_date", "2024-03-14"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture_page2))
        .mount(&mock_server)
        .await;

    // Then mount less specific mock: no cursor parameters - returns page 1 with cursor
    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a/"))
        .and(query_param("committee_id", "C00345777"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture_page1))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();

    // Fetch page 1
    let query1 = ScheduleAQuery::default().with_committee_id("C00345777");
    let result1 = client.get_schedule_a(&query1).await;
    assert!(result1.is_ok());
    let response1 = result1.unwrap();
    assert_eq!(response1.results.len(), 2);
    assert!(response1.pagination.last_indexes.is_some());

    // Extract cursor from page 1
    let cursor = response1.pagination.last_indexes.as_ref().unwrap();
    assert_eq!(cursor.last_index, 230880619);
    assert_eq!(cursor.last_contribution_receipt_date, "2024-03-14");

    // Fetch page 2 with cursor
    let query2 = ScheduleAQuery::default()
        .with_committee_id("C00345777")
        .with_last_index(cursor.last_index)
        .with_last_contribution_receipt_date(&cursor.last_contribution_receipt_date);
    let result2 = client.get_schedule_a(&query2).await;
    assert!(result2.is_ok());
    let response2 = result2.unwrap();
    assert_eq!(response2.results.len(), 1);
    assert!(response2.pagination.last_indexes.is_none());
}

#[tokio::test]
async fn schedule_a_rate_limited() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/schedules/schedule_a/"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = ScheduleAQuery::default().with_committee_id("C00345777");
    let result = client.get_schedule_a(&query).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), OpenFecError::RateLimited));
}

// ============================================================================
// API Key Verification Test
// ============================================================================

#[tokio::test]
async fn api_key_sent_as_query_param() {
    let mock_server = MockServer::start().await;
    let fixture = include_str!("fixtures/openfec_candidates.json");

    // Mock will only match if api_key query parameter is present with correct value
    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .and(query_param("api_key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture))
        .mount(&mock_server)
        .await;

    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test-key".to_string()).unwrap();
    let query = CandidateSearchQuery::default().with_name("Pelosi");
    let result = client.search_candidates(&query).await;

    // If the api_key wasn't sent as query param, mock wouldn't match and request would fail
    assert!(result.is_ok());
}
