//! Input validation for all CLI filter types.
//!
//! Every user-provided value passes through a validator before reaching the API layer.
//! Validators normalize casing, resolve aliases (e.g. "d" to "democrat"), and return
//! typed results or `CapitolTradesError::InvalidInput`.

use chrono::{NaiveDate, Utc};
use capitoltrades_api::types::{
    AssetType, Chamber, Gender, Label, MarketCap, Party, Sector, TradeSize, TxType,
};

use crate::error::CapitolTradesError;

/// Maximum byte length for free-text search inputs.
pub const MAX_SEARCH_LENGTH: usize = 100;

/// Maximum byte length for committee name/code inputs.
pub const MAX_COMMITTEE_LENGTH: usize = 80;

/// All recognized US state and territory codes (50 states + DC + 5 territories).
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
#[path = "validation_tests.rs"]
mod tests;
