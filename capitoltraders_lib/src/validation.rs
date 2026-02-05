use chrono::{NaiveDate, Utc};
use capitoltrades_api::types::{
    AssetType, Chamber, Gender, Label, MarketCap, Party, Sector, TradeSize, TxType,
};

use crate::error::CapitolTradesError;

pub const MAX_SEARCH_LENGTH: usize = 100;
pub const MAX_COMMITTEE_LENGTH: usize = 80;

pub const VALID_STATES: &[&str] = &[
    "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA", "HI", "ID", "IL", "IN", "IA",
    "KS", "KY", "LA", "ME", "MD", "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ",
    "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC", "SD", "TN", "TX", "UT", "VT",
    "VA", "WA", "WV", "WI", "WY", "DC", "AS", "GU", "MP", "PR", "VI",
];

/// Committee code-to-name mapping. The API uses short abbreviation codes
/// (e.g., `hsag` for "House - Agriculture"). Users can pass either the code
/// or the full name; we always send the code to the API.
pub const COMMITTEE_MAP: &[(&str, &str)] = &[
    // House committees
    ("hsag", "House - Agriculture"),
    ("hsap", "House - Appropriations"),
    ("hsas", "House - Armed Services"),
    ("hsbu", "House - Budget"),
    ("hscn", "House - Climate Crisis"),
    ("hsvc", "House - Coronavirus Pandemic"),
    ("hsed", "House - Education & Labor"),
    ("hsif", "House - Energy & Commerce"),
    ("hsso", "House - Ethics"),
    ("hsba", "House - Financial Services"),
    ("hsfa", "House - Foreign Affairs"),
    ("hshm", "House - Homeland Security"),
    ("hsha", "House - House Administration"),
    ("hlig", "House - Intelligence"),
    ("hsig", "House - Intelligence (Previous)"),
    ("hsju", "House - Judiciary"),
    ("hsmh", "House - Modernization of Congress"),
    ("hsii", "House - Natural Resources"),
    ("hsgo", "House - Oversight and Reform"),
    ("hsru", "House - Rules"),
    ("hssy", "House - Science, Space and Technology"),
    ("hssm", "House - Small Business"),
    ("hszs", "House - Strategic Competition between US and CCP"),
    ("hspw", "House - Transportation & Infrastructure"),
    ("hsvr", "House - Veterans' Affairs"),
    ("hswm", "House - Ways & Means"),
    ("hsfd", "House - Weaponization of the Federal Government"),
    // Senate committees
    ("ssaf", "Senate - Agriculture, Nutrition & Forestry"),
    ("ssap", "Senate - Appropriations"),
    ("ssas", "Senate - Armed Services"),
    ("ssbk", "Senate - Banking, Housing & Urban Affairs"),
    ("ssbu", "Senate - Budget"),
    ("sscm", "Senate - Commerce, Science & Transportation"),
    ("sseg", "Senate - Energy & Natural Resources"),
    ("ssev", "Senate - Environment & Public Works"),
    ("slet", "Senate - Ethics"),
    ("ssfi", "Senate - Finance"),
    ("ssfr", "Senate - Foreign Relations"),
    ("sshr", "Senate - Health, Education, Labor & Pensions"),
    ("ssga", "Senate - Homeland Security & Gov. Affairs"),
    ("slia", "Senate - Indian Affairs"),
    ("slin", "Senate - Intelligence"),
    ("ssju", "Senate - Judiciary"),
    ("ssra", "Senate - Rules and Administration"),
    ("sssb", "Senate - Small Business & Entrepreneurship"),
    ("ssva", "Senate - Veterans' Affairs"),
    ("spag", "Senate - Aging"),
    ("scnc", "Senate - International Narcotics Control"),
];

/// Strip ASCII control characters (0x00-0x1F except space 0x20), trim whitespace,
/// and enforce a byte-length limit.
pub fn sanitize_text(input: &str, max_len: usize) -> Result<String, CapitolTradesError> {
    if input.len() > max_len {
        return Err(CapitolTradesError::InvalidInput(format!(
            "input exceeds maximum length of {} bytes",
            max_len
        )));
    }
    let sanitized: String = input
        .chars()
        .filter(|c| !c.is_ascii_control() || *c == ' ')
        .collect::<String>()
        .trim()
        .to_string();
    if sanitized.is_empty() {
        return Err(CapitolTradesError::InvalidInput(
            "input is empty after sanitization".to_string(),
        ));
    }
    Ok(sanitized)
}

/// Validate a search/name string: enforce length, strip control chars, trim.
pub fn validate_search(input: &str) -> Result<String, CapitolTradesError> {
    sanitize_text(input, MAX_SEARCH_LENGTH)
}

/// Validate a US state code: uppercase, check against known states + territories.
pub fn validate_state(input: &str) -> Result<String, CapitolTradesError> {
    let upper = input.trim().to_uppercase();
    if VALID_STATES.contains(&upper.as_str()) {
        Ok(upper)
    } else {
        Err(CapitolTradesError::InvalidInput(format!(
            "unknown state code '{}'. Valid codes: AL, AK, AZ, ... DC, PR, VI (50 states + DC + territories)",
            input
        )))
    }
}

/// Validate a party string: case-insensitive, supports shorthand d/r.
pub fn validate_party(input: &str) -> Result<Party, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "democrat" | "d" => Ok(Party::Democrat),
        "republican" | "r" => Ok(Party::Republican),
        "other" => Ok(Party::Other),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown party '{}'. Valid values: democrat (d), republican (r), other",
            input
        ))),
    }
}

/// Validate a committee input: accepts either an abbreviation code (e.g., `ssfi`)
/// or a full name (e.g., `Senate - Finance`), case-insensitive.
/// Returns the abbreviation code that the API expects.
pub fn validate_committee(input: &str) -> Result<String, CapitolTradesError> {
    if input.len() > MAX_COMMITTEE_LENGTH {
        return Err(CapitolTradesError::InvalidInput(format!(
            "committee input exceeds maximum length of {} bytes",
            MAX_COMMITTEE_LENGTH
        )));
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CapitolTradesError::InvalidInput(
            "committee name is empty".to_string(),
        ));
    }
    let lower = trimmed.to_lowercase();

    // First, check if input matches a code directly
    for &(code, _name) in COMMITTEE_MAP {
        if code == lower {
            return Ok(code.to_string());
        }
    }

    // Second, check if input matches a full name (case-insensitive)
    for &(code, name) in COMMITTEE_MAP {
        if name.to_lowercase() == lower {
            return Ok(code.to_string());
        }
    }

    // Build a helpful error listing some codes
    let codes: Vec<&str> = COMMITTEE_MAP.iter().map(|(code, _)| *code).collect();
    Err(CapitolTradesError::InvalidInput(format!(
        "unknown committee '{}'. Use a code (e.g., ssfi, hsag) or full name (e.g., 'Senate - Finance'). \
         Valid codes: {}",
        trimmed,
        codes.join(", ")
    )))
}

/// Validate page number (must be >= 1).
pub fn validate_page(page: i64) -> Result<i64, CapitolTradesError> {
    if page < 1 {
        return Err(CapitolTradesError::InvalidInput(
            "page must be >= 1".to_string(),
        ));
    }
    Ok(page)
}

/// Validate page size (must be 1..=100).
pub fn validate_page_size(page_size: i64) -> Result<i64, CapitolTradesError> {
    if !(1..=100).contains(&page_size) {
        return Err(CapitolTradesError::InvalidInput(
            "page_size must be between 1 and 100".to_string(),
        ));
    }
    Ok(page_size)
}

/// Validate a gender string: case-insensitive, supports shorthand f/m.
pub fn validate_gender(input: &str) -> Result<Gender, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "female" | "f" => Ok(Gender::Female),
        "male" | "m" => Ok(Gender::Male),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown gender '{}'. Valid values: female (f), male (m)",
            input
        ))),
    }
}

/// Validate a market cap string: accepts name or numeric value 1-6.
pub fn validate_market_cap(input: &str) -> Result<MarketCap, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "mega" | "1" => Ok(MarketCap::Mega),
        "large" | "2" => Ok(MarketCap::Large),
        "mid" | "3" => Ok(MarketCap::Mid),
        "small" | "4" => Ok(MarketCap::Small),
        "micro" | "5" => Ok(MarketCap::Micro),
        "nano" | "6" => Ok(MarketCap::Nano),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown market cap '{}'. Valid values: mega (1), large (2), mid (3), small (4), micro (5), nano (6)",
            input
        ))),
    }
}

/// Validate an asset type string (kebab-case).
pub fn validate_asset_type(input: &str) -> Result<AssetType, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "stock" => Ok(AssetType::Stock),
        "stock-option" => Ok(AssetType::StockOption),
        "corporate-bond" => Ok(AssetType::CorporateBond),
        "etf" => Ok(AssetType::Etf),
        "etn" => Ok(AssetType::Etn),
        "mutual-fund" => Ok(AssetType::MutualFund),
        "cryptocurrency" => Ok(AssetType::Cryptocurrency),
        "pdf" => Ok(AssetType::Pdf),
        "municipal-security" => Ok(AssetType::MunicipalSecurity),
        "non-public-stock" => Ok(AssetType::NonPublicStock),
        "other" => Ok(AssetType::Other),
        "reit" => Ok(AssetType::Reit),
        "commodity" => Ok(AssetType::Commodity),
        "hedge" => Ok(AssetType::Hedge),
        "variable-insurance" => Ok(AssetType::VariableInsurance),
        "private-equity" => Ok(AssetType::PrivateEquity),
        "closed-end-fund" => Ok(AssetType::ClosedEndFund),
        "venture" => Ok(AssetType::Venture),
        "index-fund" => Ok(AssetType::IndexFund),
        "government-bond" => Ok(AssetType::GovernmentBond),
        "money-market-fund" => Ok(AssetType::MoneyMarketFund),
        "brokered" => Ok(AssetType::Brokered),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown asset type '{}'. Valid values: stock, stock-option, corporate-bond, etf, etn, \
             mutual-fund, cryptocurrency, pdf, municipal-security, non-public-stock, other, reit, \
             commodity, hedge, variable-insurance, private-equity, closed-end-fund, venture, \
             index-fund, government-bond, money-market-fund, brokered",
            input
        ))),
    }
}

/// Validate a label string.
pub fn validate_label(input: &str) -> Result<Label, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "faang" => Ok(Label::Faang),
        "crypto" => Ok(Label::Crypto),
        "memestock" => Ok(Label::Memestock),
        "spac" => Ok(Label::Spac),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown label '{}'. Valid values: faang, crypto, memestock, spac",
            input
        ))),
    }
}

/// Validate a sector string (kebab-case).
pub fn validate_sector(input: &str) -> Result<Sector, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "communication-services" => Ok(Sector::CommunicationServices),
        "consumer-discretionary" => Ok(Sector::ConsumerDiscretionary),
        "consumer-staples" => Ok(Sector::ConsumerStaples),
        "energy" => Ok(Sector::Energy),
        "financials" => Ok(Sector::Financials),
        "health-care" => Ok(Sector::HealthCare),
        "industrials" => Ok(Sector::Industrials),
        "information-technology" => Ok(Sector::InformationTechnology),
        "materials" => Ok(Sector::Materials),
        "real-estate" => Ok(Sector::RealEstate),
        "utilities" => Ok(Sector::Utilities),
        "other" => Ok(Sector::Other),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown sector '{}'. Valid values: communication-services, consumer-discretionary, \
             consumer-staples, energy, financials, health-care, industrials, \
             information-technology, materials, real-estate, utilities, other",
            input
        ))),
    }
}

/// Validate a transaction type string.
pub fn validate_tx_type(input: &str) -> Result<TxType, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "buy" => Ok(TxType::Buy),
        "sell" => Ok(TxType::Sell),
        "exchange" => Ok(TxType::Exchange),
        "receive" => Ok(TxType::Receive),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown tx type '{}'. Valid values: buy, sell, exchange, receive",
            input
        ))),
    }
}

/// Validate a chamber string: case-insensitive, supports shorthand h/s.
pub fn validate_chamber(input: &str) -> Result<Chamber, CapitolTradesError> {
    match input.trim().to_lowercase().as_str() {
        "house" | "h" => Ok(Chamber::House),
        "senate" | "s" => Ok(Chamber::Senate),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "unknown chamber '{}'. Valid values: house (h), senate (s)",
            input
        ))),
    }
}

/// Validate a politician ID: must match P followed by 6 digits (e.g., P000197).
pub fn validate_politician_id(input: &str) -> Result<String, CapitolTradesError> {
    let trimmed = input.trim();
    if trimmed.len() == 7
        && trimmed.starts_with('P')
        && trimmed[1..].chars().all(|c| c.is_ascii_digit())
    {
        Ok(trimmed.to_string())
    } else {
        Err(CapitolTradesError::InvalidInput(format!(
            "invalid politician ID '{}'. Expected format: P followed by 6 digits (e.g., P000197)",
            input
        )))
    }
}

/// Validate a country code: 2-letter ISO code, normalized to lowercase.
pub fn validate_country(input: &str) -> Result<String, CapitolTradesError> {
    let trimmed = input.trim();
    if trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(trimmed.to_lowercase())
    } else {
        Err(CapitolTradesError::InvalidInput(format!(
            "invalid country code '{}'. Expected 2-letter ISO code (e.g., us, uk)",
            input
        )))
    }
}

/// Validate an issuer state code: 2-letter code, normalized to lowercase.
pub fn validate_issuer_state(input: &str) -> Result<String, CapitolTradesError> {
    let trimmed = input.trim();
    if trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(trimmed.to_lowercase())
    } else {
        Err(CapitolTradesError::InvalidInput(format!(
            "invalid issuer state '{}'. Expected 2-letter code (e.g., ca, ny)",
            input
        )))
    }
}

/// Validate a trade size: must be 1-10.
pub fn validate_trade_size(input: &str) -> Result<TradeSize, CapitolTradesError> {
    match input.trim() {
        "1" => Ok(TradeSize::Less1K),
        "2" => Ok(TradeSize::From1Kto15K),
        "3" => Ok(TradeSize::From15Kto50K),
        "4" => Ok(TradeSize::From50Kto100K),
        "5" => Ok(TradeSize::From100Kto250K),
        "6" => Ok(TradeSize::From250Kto500K),
        "7" => Ok(TradeSize::From500Kto1M),
        "8" => Ok(TradeSize::From1Mto5M),
        "9" => Ok(TradeSize::From5Mto25M),
        "10" => Ok(TradeSize::From25Mto50M),
        _ => Err(CapitolTradesError::InvalidInput(format!(
            "invalid trade size '{}'. Valid values: 1 (<$1K), 2 ($1K-$15K), 3 ($15K-$50K), \
             4 ($50K-$100K), 5 ($100K-$250K), 6 ($250K-$500K), 7 ($500K-$1M), 8 ($1M-$5M), \
             9 ($5M-$25M), 10 ($25M-$50M)",
            input
        ))),
    }
}

/// Validate a YYYY-MM-DD date string.
pub fn validate_date(input: &str) -> Result<NaiveDate, CapitolTradesError> {
    let trimmed = input.trim();
    NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").map_err(|_| {
        CapitolTradesError::InvalidInput(format!(
            "invalid date '{}'. Expected format: YYYY-MM-DD (e.g., 2024-06-01)",
            trimmed
        ))
    })
}

/// Validate relative days: must be 1..=3650 (approx 10 years).
pub fn validate_days(days: i64) -> Result<i64, CapitolTradesError> {
    if !(1..=3650).contains(&days) {
        return Err(CapitolTradesError::InvalidInput(format!(
            "days must be between 1 and 3650, got {}",
            days
        )));
    }
    Ok(days)
}

/// Convert an absolute date to relative days from today.
/// Returns None if the date is in the future.
pub fn date_to_relative_days(date: NaiveDate) -> Option<i64> {
    let today = Utc::now().date_naive();
    let diff = (today - date).num_days();
    if diff < 0 {
        None
    } else {
        Some(diff)
    }
}

#[cfg(test)]
mod tests {
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
        assert!(matches!(validate_party("democrat").unwrap(), Party::Democrat));
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
        assert!(matches!(validate_party("Democrat").unwrap(), Party::Democrat));
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
        assert_eq!(
            validate_committee("Senate - Finance").unwrap(),
            "ssfi"
        );
    }

    #[test]
    fn committee_by_full_name_case_insensitive() {
        assert_eq!(
            validate_committee("senate - finance").unwrap(),
            "ssfi"
        );
    }

    #[test]
    fn committee_house_by_code() {
        assert_eq!(validate_committee("hsag").unwrap(), "hsag");
    }

    #[test]
    fn committee_house_by_name() {
        assert_eq!(
            validate_committee("House - Agriculture").unwrap(),
            "hsag"
        );
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
        assert!(matches!(validate_market_cap("mega").unwrap(), MarketCap::Mega));
        assert!(matches!(validate_market_cap("nano").unwrap(), MarketCap::Nano));
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
        assert!(matches!(validate_asset_type("stock").unwrap(), AssetType::Stock));
    }

    #[test]
    fn asset_type_kebab() {
        assert!(matches!(validate_asset_type("stock-option").unwrap(), AssetType::StockOption));
        assert!(matches!(validate_asset_type("mutual-fund").unwrap(), AssetType::MutualFund));
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
        assert!(matches!(validate_label("memestock").unwrap(), Label::Memestock));
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
        assert!(matches!(validate_tx_type("exchange").unwrap(), TxType::Exchange));
        assert!(matches!(validate_tx_type("receive").unwrap(), TxType::Receive));
    }

    #[test]
    fn tx_type_invalid() {
        assert!(validate_tx_type("transfer").is_err());
    }

    // -- Chamber validation --

    #[test]
    fn chamber_valid() {
        assert!(matches!(validate_chamber("house").unwrap(), Chamber::House));
        assert!(matches!(validate_chamber("senate").unwrap(), Chamber::Senate));
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
        assert!(matches!(validate_trade_size("1").unwrap(), TradeSize::Less1K));
        assert!(matches!(validate_trade_size("10").unwrap(), TradeSize::From25Mto50M));
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
}
