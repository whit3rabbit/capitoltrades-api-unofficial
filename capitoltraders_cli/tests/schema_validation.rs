use serde_json::Value;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("CLI crate should be inside workspace")
        .to_path_buf()
}

fn load_fixture(name: &str) -> Value {
    let path = workspace_root()
        .join("capitoltrades_api/tests/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {}", path.display(), e));
    serde_json::from_str(&text).expect("fixture is valid JSON")
}

fn load_schema(name: &str) -> Value {
    let path = workspace_root().join("schema").join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read schema {}: {}", path.display(), e));
    serde_json::from_str(&text).expect("schema is valid JSON")
}

fn extract_data_array(fixture: &Value) -> Value {
    fixture["data"].clone()
}

// ---------------------------------------------------------------------------
// Positive validation: fixtures conform to their schemas
// ---------------------------------------------------------------------------

#[test]
fn test_trades_fixture_conforms_to_schema() {
    let fixture = load_fixture("trades.json");
    let schema = load_schema("trade.schema.json");
    let data = extract_data_array(&fixture);

    let validator = jsonschema::draft202012::new(&schema).expect("trade schema compiles");
    let result = validator.validate(&data);
    if let Err(e) = &result {
        panic!("trades fixture failed validation: {e}");
    }
}

#[test]
fn test_politicians_fixture_conforms_to_schema() {
    let fixture = load_fixture("politicians.json");
    let schema = load_schema("politician.schema.json");
    let data = extract_data_array(&fixture);

    let validator = jsonschema::draft202012::new(&schema).expect("politician schema compiles");
    let result = validator.validate(&data);
    if let Err(e) = &result {
        panic!("politicians fixture failed validation: {e}");
    }
}

#[test]
fn test_issuers_fixture_conforms_to_schema() {
    let fixture = load_fixture("issuers.json");
    let schema = load_schema("issuer.schema.json");
    let data = extract_data_array(&fixture);

    let validator = jsonschema::draft202012::new(&schema).expect("issuer schema compiles");
    let result = validator.validate(&data);
    if let Err(e) = &result {
        panic!("issuers fixture failed validation: {e}");
    }
}

// ---------------------------------------------------------------------------
// Negative validation: schemas reject invalid data
// ---------------------------------------------------------------------------

#[test]
fn test_trade_schema_rejects_missing_required_field() {
    let fixture = load_fixture("trades.json");
    let schema = load_schema("trade.schema.json");
    let mut data = extract_data_array(&fixture);

    // Remove _txId from the first trade
    data[0]
        .as_object_mut()
        .expect("trade is an object")
        .remove("_txId");

    let validator = jsonschema::draft202012::new(&schema).expect("schema compiles");
    assert!(
        validator.validate(&data).is_err(),
        "schema should reject trade missing _txId"
    );
}

#[test]
fn test_politician_schema_rejects_missing_required_field() {
    let fixture = load_fixture("politicians.json");
    let schema = load_schema("politician.schema.json");
    let mut data = extract_data_array(&fixture);

    data[0]
        .as_object_mut()
        .expect("politician is an object")
        .remove("_politicianId");

    let validator = jsonschema::draft202012::new(&schema).expect("schema compiles");
    assert!(
        validator.validate(&data).is_err(),
        "schema should reject politician missing _politicianId"
    );
}

#[test]
fn test_issuer_schema_rejects_missing_required_field() {
    let fixture = load_fixture("issuers.json");
    let schema = load_schema("issuer.schema.json");
    let mut data = extract_data_array(&fixture);

    data[0]
        .as_object_mut()
        .expect("issuer is an object")
        .remove("_issuerId");

    let validator = jsonschema::draft202012::new(&schema).expect("schema compiles");
    assert!(
        validator.validate(&data).is_err(),
        "schema should reject issuer missing _issuerId"
    );
}

#[test]
fn test_trade_schema_rejects_invalid_enum() {
    let fixture = load_fixture("trades.json");
    let schema = load_schema("trade.schema.json");
    let mut data = extract_data_array(&fixture);

    data[0]
        .as_object_mut()
        .expect("trade is an object")
        .insert("txType".to_string(), Value::String("bogus".to_string()));

    let validator = jsonschema::draft202012::new(&schema).expect("schema compiles");
    assert!(
        validator.validate(&data).is_err(),
        "schema should reject invalid txType enum value"
    );
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_trade_schema_rejects_additional_properties() {
    let fixture = load_fixture("trades.json");
    let schema = load_schema("trade.schema.json");
    let mut data = extract_data_array(&fixture);

    data[0]
        .as_object_mut()
        .expect("trade is an object")
        .insert("bogusField".to_string(), Value::Number(123.into()));

    let validator = jsonschema::draft202012::new(&schema).expect("schema compiles");
    assert!(
        validator.validate(&data).is_err(),
        "schema should reject additional properties"
    );
}

#[test]
fn test_empty_array_conforms_to_all_schemas() {
    let empty = serde_json::json!([]);

    for schema_name in ["trade.schema.json", "politician.schema.json", "issuer.schema.json"] {
        let schema = load_schema(schema_name);
        let validator =
            jsonschema::draft202012::new(&schema).unwrap_or_else(|e| panic!("{schema_name}: {e}"));
        let result = validator.validate(&empty);
        if let Err(e) = &result {
            panic!("empty array should conform to {schema_name}: {e}");
        }
    }
}
