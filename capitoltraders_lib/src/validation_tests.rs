use super::*;

// -- State validation --

#[test]
fn state_valid_uppercase() {
    assert_eq!(validate_state("CA").unwrap(), "CA");
}

#[test]
fn state_valid_lowercase() {
    assert_eq!(validate_state("ca").unwrap(), "CA");
}

#[test]
fn state_valid_dc() {
    assert_eq!(validate_state("DC").unwrap(), "DC");
}

#[test]
fn state_valid_territory() {
    assert_eq!(validate_state("pr").unwrap(), "PR");
    assert_eq!(validate_state("vi").unwrap(), "VI");
}

#[test]
fn state_invalid() {
    assert!(validate_state("XX").is_err());
}

#[test]
fn state_empty() {
    assert!(validate_state("").is_err());
}

#[test]
fn state_too_long() {
    assert!(validate_state("CALIFORNIA").is_err());
}

#[test]
fn state_numeric() {
    assert!(validate_state("12").is_err());
}

#[test]
fn state_unicode() {
    assert!(validate_state("\u{00C7}A").is_err());
}

// -- Party validation --

#[test]
fn party_democrat() {
    assert!(matches!(
        validate_party("democrat").unwrap(),
        Party::Democrat
    ));
}

#[test]
fn party_republican() {
    assert!(matches!(
        validate_party("republican").unwrap(),
        Party::Republican
    ));
}

#[test]
fn party_other() {
    assert!(matches!(validate_party("other").unwrap(), Party::Other));
}

#[test]
fn party_shorthand_d() {
    assert!(matches!(validate_party("d").unwrap(), Party::Democrat));
}

#[test]
fn party_shorthand_r() {
    assert!(matches!(validate_party("r").unwrap(), Party::Republican));
}

#[test]
fn party_mixed_case() {
    assert!(matches!(
        validate_party("Democrat").unwrap(),
        Party::Democrat
    ));
    assert!(matches!(
        validate_party("REPUBLICAN").unwrap(),
        Party::Republican
    ));
}

#[test]
fn party_invalid() {
    assert!(validate_party("libertarian").is_err());
}

#[test]
fn party_empty() {
    assert!(validate_party("").is_err());
}

// -- Committee validation --

#[test]
fn committee_by_code() {
    assert_eq!(validate_committee("ssfi").unwrap(), "ssfi");
}

#[test]
fn committee_by_code_uppercase() {
    assert_eq!(validate_committee("SSFI").unwrap(), "ssfi");
}

#[test]
fn committee_by_full_name() {
    assert_eq!(validate_committee("Senate - Finance").unwrap(), "ssfi");
}

#[test]
fn committee_by_full_name_case_insensitive() {
    assert_eq!(validate_committee("senate - finance").unwrap(), "ssfi");
}

#[test]
fn committee_house_by_code() {
    assert_eq!(validate_committee("hsag").unwrap(), "hsag");
}

#[test]
fn committee_house_by_name() {
    assert_eq!(validate_committee("House - Agriculture").unwrap(), "hsag");
}

#[test]
fn committee_invalid() {
    assert!(validate_committee("Senate - Fake Committee").is_err());
}

#[test]
fn committee_empty() {
    assert!(validate_committee("").is_err());
}

#[test]
fn committee_too_long() {
    let long = "x".repeat(MAX_COMMITTEE_LENGTH + 1);
    assert!(validate_committee(&long).is_err());
}

// -- Search/name sanitization --

#[test]
fn search_normal_text() {
    assert_eq!(validate_search("Pelosi").unwrap(), "Pelosi");
}

#[test]
fn search_control_chars_stripped() {
    assert_eq!(validate_search("Pel\x00osi\x01").unwrap(), "Pelosi");
}

#[test]
fn search_max_length_exceeded() {
    let long = "x".repeat(MAX_SEARCH_LENGTH + 1);
    assert!(validate_search(&long).is_err());
}

#[test]
fn search_empty_after_trim() {
    assert!(validate_search("   ").is_err());
}

#[test]
fn search_unicode_preserved() {
    assert_eq!(validate_search("Garc\u{00ED}a").unwrap(), "Garc\u{00ED}a");
}

#[test]
fn search_null_bytes_stripped() {
    assert_eq!(validate_search("te\x00st").unwrap(), "test");
}

#[test]
fn search_whitespace_trimmed() {
    assert_eq!(validate_search("  Pelosi  ").unwrap(), "Pelosi");
}

// -- Page bounds --

#[test]
fn page_valid() {
    assert_eq!(validate_page(1).unwrap(), 1);
    assert_eq!(validate_page(100).unwrap(), 100);
}

#[test]
fn page_zero_rejected() {
    assert!(validate_page(0).is_err());
}

#[test]
fn page_negative_rejected() {
    assert!(validate_page(-1).is_err());
}

#[test]
fn page_size_valid() {
    assert_eq!(validate_page_size(1).unwrap(), 1);
    assert_eq!(validate_page_size(100).unwrap(), 100);
}

#[test]
fn page_size_zero_rejected() {
    assert!(validate_page_size(0).is_err());
}

#[test]
fn page_size_over_100_rejected() {
    assert!(validate_page_size(101).is_err());
}

// -- Gender validation --

#[test]
fn gender_female() {
    assert!(matches!(validate_gender("female").unwrap(), Gender::Female));
}

#[test]
fn gender_male() {
    assert!(matches!(validate_gender("male").unwrap(), Gender::Male));
}

#[test]
fn gender_shorthand() {
    assert!(matches!(validate_gender("f").unwrap(), Gender::Female));
    assert!(matches!(validate_gender("m").unwrap(), Gender::Male));
}

#[test]
fn gender_mixed_case() {
    assert!(matches!(validate_gender("Female").unwrap(), Gender::Female));
}

#[test]
fn gender_invalid() {
    assert!(validate_gender("unknown").is_err());
}

// -- Market cap validation --

#[test]
fn market_cap_by_name() {
    assert!(matches!(
        validate_market_cap("mega").unwrap(),
        MarketCap::Mega
    ));
    assert!(matches!(
        validate_market_cap("nano").unwrap(),
        MarketCap::Nano
    ));
}

#[test]
fn market_cap_by_number() {
    assert!(matches!(validate_market_cap("1").unwrap(), MarketCap::Mega));
    assert!(matches!(validate_market_cap("6").unwrap(), MarketCap::Nano));
}

#[test]
fn market_cap_invalid() {
    assert!(validate_market_cap("huge").is_err());
    assert!(validate_market_cap("0").is_err());
    assert!(validate_market_cap("7").is_err());
}

// -- Asset type validation --

#[test]
fn asset_type_stock() {
    assert!(matches!(
        validate_asset_type("stock").unwrap(),
        AssetType::Stock
    ));
}

#[test]
fn asset_type_kebab() {
    assert!(matches!(
        validate_asset_type("stock-option").unwrap(),
        AssetType::StockOption
    ));
    assert!(matches!(
        validate_asset_type("mutual-fund").unwrap(),
        AssetType::MutualFund
    ));
}

#[test]
fn asset_type_invalid() {
    assert!(validate_asset_type("bonds").is_err());
}

// -- Label validation --

#[test]
fn label_valid() {
    assert!(matches!(validate_label("faang").unwrap(), Label::Faang));
    assert!(matches!(validate_label("crypto").unwrap(), Label::Crypto));
    assert!(matches!(
        validate_label("memestock").unwrap(),
        Label::Memestock
    ));
    assert!(matches!(validate_label("spac").unwrap(), Label::Spac));
}

#[test]
fn label_invalid() {
    assert!(validate_label("growth").is_err());
}

// -- Sector validation --

#[test]
fn sector_valid() {
    assert!(matches!(validate_sector("energy").unwrap(), Sector::Energy));
    assert!(matches!(
        validate_sector("information-technology").unwrap(),
        Sector::InformationTechnology
    ));
}

#[test]
fn sector_invalid() {
    assert!(validate_sector("tech").is_err());
}

// -- Tx type validation --

#[test]
fn tx_type_valid() {
    assert!(matches!(validate_tx_type("buy").unwrap(), TxType::Buy));
    assert!(matches!(validate_tx_type("sell").unwrap(), TxType::Sell));
    assert!(matches!(
        validate_tx_type("exchange").unwrap(),
        TxType::Exchange
    ));
    assert!(matches!(
        validate_tx_type("receive").unwrap(),
        TxType::Receive
    ));
}

#[test]
fn tx_type_invalid() {
    assert!(validate_tx_type("transfer").is_err());
}

// -- Chamber validation --

#[test]
fn chamber_valid() {
    assert!(matches!(validate_chamber("house").unwrap(), Chamber::House));
    assert!(matches!(
        validate_chamber("senate").unwrap(),
        Chamber::Senate
    ));
}

#[test]
fn chamber_shorthand() {
    assert!(matches!(validate_chamber("h").unwrap(), Chamber::House));
    assert!(matches!(validate_chamber("s").unwrap(), Chamber::Senate));
}

#[test]
fn chamber_invalid() {
    assert!(validate_chamber("congress").is_err());
}

// -- Politician ID validation --

#[test]
fn politician_id_valid() {
    assert_eq!(validate_politician_id("P000197").unwrap(), "P000197");
}

#[test]
fn politician_id_invalid_prefix() {
    assert!(validate_politician_id("X000197").is_err());
}

#[test]
fn politician_id_too_short() {
    assert!(validate_politician_id("P0001").is_err());
}

#[test]
fn politician_id_non_digit() {
    assert!(validate_politician_id("P00019a").is_err());
}

// -- Country validation --

#[test]
fn country_valid() {
    assert_eq!(validate_country("US").unwrap(), "us");
    assert_eq!(validate_country("uk").unwrap(), "uk");
}

#[test]
fn country_invalid_length() {
    assert!(validate_country("USA").is_err());
    assert!(validate_country("U").is_err());
}

#[test]
fn country_invalid_chars() {
    assert!(validate_country("12").is_err());
}

// -- Issuer state validation --

#[test]
fn issuer_state_valid() {
    assert_eq!(validate_issuer_state("CA").unwrap(), "ca");
    assert_eq!(validate_issuer_state("ny").unwrap(), "ny");
}

#[test]
fn issuer_state_invalid() {
    assert!(validate_issuer_state("123").is_err());
    assert!(validate_issuer_state("C").is_err());
}

// -- Trade size validation --

#[test]
fn trade_size_valid() {
    assert!(matches!(
        validate_trade_size("1").unwrap(),
        TradeSize::Less1K
    ));
    assert!(matches!(
        validate_trade_size("10").unwrap(),
        TradeSize::From25Mto50M
    ));
}

#[test]
fn trade_size_invalid() {
    assert!(validate_trade_size("0").is_err());
    assert!(validate_trade_size("11").is_err());
    assert!(validate_trade_size("abc").is_err());
}

// -- Date validation --

#[test]
fn date_valid() {
    let d = validate_date("2024-06-01").unwrap();
    assert_eq!(d, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap());
}

#[test]
fn date_with_whitespace() {
    let d = validate_date("  2024-01-15  ").unwrap();
    assert_eq!(d, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
}

#[test]
fn date_invalid_format() {
    assert!(validate_date("06/01/2024").is_err());
    assert!(validate_date("not-a-date").is_err());
}

#[test]
fn date_single_digit_accepted() {
    // chrono's %m/%d accepts single-digit values
    let d = validate_date("2024-6-1").unwrap();
    assert_eq!(d, NaiveDate::from_ymd_opt(2024, 6, 1).unwrap());
}

#[test]
fn date_invalid_values() {
    assert!(validate_date("2024-13-01").is_err());
    assert!(validate_date("2024-02-30").is_err());
}

#[test]
fn date_empty() {
    assert!(validate_date("").is_err());
    assert!(validate_date("   ").is_err());
}

// -- Days validation --

#[test]
fn days_valid() {
    assert_eq!(validate_days(1).unwrap(), 1);
    assert_eq!(validate_days(30).unwrap(), 30);
    assert_eq!(validate_days(3650).unwrap(), 3650);
}

#[test]
fn days_zero_rejected() {
    assert!(validate_days(0).is_err());
}

#[test]
fn days_negative_rejected() {
    assert!(validate_days(-1).is_err());
}

#[test]
fn days_over_max_rejected() {
    assert!(validate_days(3651).is_err());
}

// -- date_to_relative_days --

#[test]
fn relative_days_past_date() {
    let yesterday = Utc::now().date_naive() - chrono::Duration::days(5);
    assert_eq!(date_to_relative_days(yesterday), Some(5));
}

#[test]
fn relative_days_today() {
    let today = Utc::now().date_naive();
    assert_eq!(date_to_relative_days(today), Some(0));
}

#[test]
fn relative_days_future_returns_none() {
    let tomorrow = Utc::now().date_naive() + chrono::Duration::days(1);
    assert_eq!(date_to_relative_days(tomorrow), None);
}
