//! Integration tests for CommitteeResolver with wiremock.

use capitoltraders_lib::openfec::{OpenFecClient, OpenFecError};
use capitoltraders_lib::{CommitteeClass, CommitteeResolver, Db};
use serde_json::json;
use std::sync::{Arc, Mutex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to set up test infrastructure with in-memory DB and wiremock.
async fn setup_resolver() -> (CommitteeResolver, MockServer, Arc<Mutex<Db>>) {
    // Create in-memory DB and initialize schema
    let db = Db::open_in_memory().expect("open db");
    db.init().expect("init db");

    // Insert test politician
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO politicians (politician_id, first_name, last_name, state_id, party, dob, gender, chamber)
             VALUES ('P000197', 'Nancy', 'Pelosi', 'CA', 'Democrat', '1940-03-26', 'F', 'house')",
            [],
        )
        .expect("insert politician");

        // Insert FEC mapping
        conn.execute(
            "INSERT INTO fec_mappings (politician_id, fec_candidate_id, bioguide_id, last_synced)
             VALUES ('P000197', 'H8CA05024', 'P000197', datetime('now'))",
            [],
        )
        .expect("insert fec_mapping");
    }

    let db = Arc::new(Mutex::new(db));

    // Create mock server and OpenFEC client
    let mock_server = MockServer::start().await;
    let base_url = format!("{}/v1", mock_server.uri());
    let client = OpenFecClient::with_base_url(&base_url, "test_api_key".to_string())
        .expect("create client");
    let client = Arc::new(client);

    // Create resolver
    let resolver = CommitteeResolver::new(client, db.clone());

    (resolver, mock_server, db)
}

#[tokio::test]
async fn test_resolve_from_api_stores_in_db() {
    let (resolver, mock_server, db) = setup_resolver().await;

    // Mount mock for get_candidate_committees
    let response_json = json!({
        "results": [
            {
                "committee_id": "C00481689",
                "name": "PELOSI FOR CONGRESS",
                "committee_type": "H",
                "designation": "A",
                "party": "DEM",
                "state": "CA",
                "cycles": [2020, 2022]
            },
            {
                "committee_id": "C00193433",
                "name": "PAC TO THE FUTURE",
                "committee_type": "N",
                "designation": "D",
                "party": "DEM",
                "state": "CA",
                "cycles": [2020, 2022]
            }
        ],
        "pagination": {
            "count": 2,
            "page": null,
            "pages": null,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05024/committees/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_json))
        .mount(&mock_server)
        .await;

    // Resolve committees
    let committees = resolver
        .resolve_committees("P000197")
        .await
        .expect("resolve_committees");

    // Verify we got 2 committees
    assert_eq!(committees.len(), 2);

    // Verify classifications
    let campaign = committees
        .iter()
        .find(|c| c.committee_id == "C00481689")
        .expect("find campaign committee");
    assert_eq!(campaign.classification, CommitteeClass::Campaign);
    assert_eq!(campaign.name, "PELOSI FOR CONGRESS");

    let leadership_pac = committees
        .iter()
        .find(|c| c.committee_id == "C00193433")
        .expect("find leadership PAC");
    assert_eq!(leadership_pac.classification, CommitteeClass::LeadershipPac);
    assert_eq!(leadership_pac.name, "PAC TO THE FUTURE");

    // Verify cache was populated
    assert_eq!(resolver.cache_len(), 1);

    // Verify DB was updated
    let db = db.lock().unwrap();
    let committee_ids = db
        .get_committees_for_politician("P000197")
        .expect("get_committees_for_politician")
        .expect("should have committees");
    assert_eq!(committee_ids.len(), 2);
    assert!(committee_ids.contains(&"C00481689".to_string()));
    assert!(committee_ids.contains(&"C00193433".to_string()));

    // Verify fec_committees table has both committees
    let count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM fec_committees", [], |row: &rusqlite::Row| {
            row.get(0)
        })
        .expect("count committees");
    assert_eq!(count, 2);
}

#[tokio::test]
async fn test_resolve_from_cache_no_api_call() {
    let (resolver, mock_server, _db) = setup_resolver().await;

    // Mount mock with expect(1) to verify it's only called once
    let response_json = json!({
        "results": [
            {
                "committee_id": "C00481689",
                "name": "PELOSI FOR CONGRESS",
                "committee_type": "H",
                "designation": "A",
                "party": "DEM",
                "state": "CA",
                "cycles": [2020, 2022]
            }
        ],
        "pagination": {
            "count": 1,
            "page": null,
            "pages": null,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05024/committees/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_json))
        .expect(1) // Should only be called once
        .mount(&mock_server)
        .await;

    // First call - hits API
    let committees1 = resolver
        .resolve_committees("P000197")
        .await
        .expect("resolve_committees first call");
    assert_eq!(committees1.len(), 1);

    // Second call - should hit cache, not API
    let committees2 = resolver
        .resolve_committees("P000197")
        .await
        .expect("resolve_committees second call");
    assert_eq!(committees2.len(), 1);

    // Verify same result
    assert_eq!(committees1[0].committee_id, committees2[0].committee_id);
}

#[tokio::test]
async fn test_resolve_from_sqlite_tier() {
    let (resolver, mock_server, _db) = setup_resolver().await;

    // Mount mock for initial API call
    let response_json = json!({
        "results": [
            {
                "committee_id": "C00481689",
                "name": "PELOSI FOR CONGRESS",
                "committee_type": "H",
                "designation": "A",
                "party": "DEM",
                "state": "CA",
                "cycles": [2020, 2022]
            }
        ],
        "pagination": {
            "count": 1,
            "page": null,
            "pages": null,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05024/committees/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(response_json))
        .expect(1) // Should only be called once
        .mount(&mock_server)
        .await;

    // First call - populates DB via API
    let committees1 = resolver
        .resolve_committees("P000197")
        .await
        .expect("resolve_committees first call");
    assert_eq!(committees1.len(), 1);

    // Clear cache to force SQLite lookup
    resolver.clear_cache();
    assert_eq!(resolver.cache_len(), 0);

    // Second call - should read from SQLite tier 2, not API tier 3
    let committees2 = resolver
        .resolve_committees("P000197")
        .await
        .expect("resolve_committees second call");
    assert_eq!(committees2.len(), 1);

    // Verify same result
    assert_eq!(committees1[0].committee_id, committees2[0].committee_id);

    // Verify cache was repopulated from SQLite
    assert_eq!(resolver.cache_len(), 1);
}

#[tokio::test]
async fn test_resolve_no_fec_ids_searches_by_name() {
    let (resolver, mock_server, db) = setup_resolver().await;

    // Create politician without fec_mapping
    {
        let db = db.lock().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO politicians (politician_id, first_name, last_name, state_id, party, dob, gender, chamber)
             VALUES ('P000999', 'John', 'Doe', 'TX', 'Republican', '1960-01-01', 'M', 'senate')",
            [],
        )
        .expect("insert politician");
    }

    // Mount mock for candidate search
    let search_response = json!({
        "results": [
            {
                "candidate_id": "S4TX00123",
                "name": "DOE, JOHN",
                "party": "REP",
                "office": "S",
                "state": "TX",
                "district": null,
                "cycles": [2020, 2022],
                "candidate_status": "C",
                "incumbent_challenge": null
            }
        ],
        "pagination": {
            "count": 1,
            "page": 1,
            "pages": 1,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response))
        .mount(&mock_server)
        .await;

    // Mount mock for committees
    let committees_response = json!({
        "results": [
            {
                "committee_id": "C00987654",
                "name": "DOE FOR SENATE",
                "committee_type": "S",
                "designation": "P",
                "party": "REP",
                "state": "TX",
                "cycles": [2020, 2022]
            }
        ],
        "pagination": {
            "count": 1,
            "page": null,
            "pages": null,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidate/S4TX00123/committees/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(committees_response))
        .mount(&mock_server)
        .await;

    // Resolve committees - should use name search fallback
    let committees = resolver
        .resolve_committees("P000999")
        .await
        .expect("resolve_committees");

    assert_eq!(committees.len(), 1);
    assert_eq!(committees[0].committee_id, "C00987654");
    assert_eq!(committees[0].classification, CommitteeClass::Campaign);
}

#[tokio::test]
async fn test_resolve_not_found_returns_empty() {
    let (resolver, mock_server, db) = setup_resolver().await;

    // Create politician without fec_mapping
    {
        let db = db.lock().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO politicians (politician_id, first_name, last_name, state_id, party, dob, gender, chamber)
             VALUES ('P000888', 'Unknown', 'Person', 'NY', 'Democrat', '1970-01-01', 'F', 'house')",
            [],
        )
        .expect("insert politician");
    }

    // Mount mock for candidate search returning empty results
    let search_response = json!({
        "results": [],
        "pagination": {
            "count": 0,
            "page": 1,
            "pages": 0,
            "per_page": 20
        }
    });

    Mock::given(method("GET"))
        .and(path("/v1/candidates/search/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(search_response))
        .mount(&mock_server)
        .await;

    // Resolve committees - should return empty
    let committees = resolver
        .resolve_committees("P000888")
        .await
        .expect("resolve_committees");

    assert_eq!(committees.len(), 0);

    // Verify empty result was cached to prevent repeated API calls
    assert_eq!(resolver.cache_len(), 1);

    // Second call should not hit API again (cached empty result)
    let committees2 = resolver
        .resolve_committees("P000888")
        .await
        .expect("resolve_committees second call");

    assert_eq!(committees2.len(), 0);
}

#[tokio::test]
async fn test_resolve_api_error_propagates() {
    let (resolver, mock_server, _db) = setup_resolver().await;

    // Mount mock returning 429 rate limited
    Mock::given(method("GET"))
        .and(path("/v1/candidate/H8CA05024/committees/"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server)
        .await;

    // Resolve committees - should propagate error
    let result = resolver.resolve_committees("P000197").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    // Check that it's an OpenFecError::RateLimited wrapped in CommitteeError
    assert!(matches!(
        err,
        capitoltraders_lib::CommitteeError::OpenFec(OpenFecError::RateLimited)
    ));
}
