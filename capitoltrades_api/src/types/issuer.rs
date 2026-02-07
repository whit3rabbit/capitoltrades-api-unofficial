//! Issuer-related types: companies and funds that politicians trade.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

extern crate serde_json;

/// Numeric identifier for an issuer (company or fund).
pub type IssuerID = i64;

/// Full issuer record returned by the `/issuers` endpoint.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuerDetail {
    /// Unique numeric issuer identifier.
    #[serde(rename = "_issuerId")]
    pub issuer_id: IssuerID,

    #[serde(rename = "_stateId")]
    state_id: Option<String>,

    #[serde(rename = "c2iq")]
    c2_iq: Option<String>,

    country: Option<String>,

    /// Company or fund name.
    pub issuer_name: String,

    /// Ticker symbol, if publicly traded.
    pub issuer_ticker: Option<String>,

    /// Market performance data. `None` for non-public issuers.
    pub performance: Option<Performance>,

    /// GICS sector classification, if available.
    pub sector: Option<Sector>,

    /// Aggregate trading statistics for this issuer.
    pub stats: Stats,
}

/// Market performance and price data for an issuer.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    /// End-of-day price history. Each inner vec contains date/price pairs.
    pub eod_prices: Vec<Vec<EodPrice>>,

    /// Market capitalization in dollars.
    pub mcap: i64,

    /// Trailing 1-day price.
    pub trailing1: f64,
    /// Trailing 1-day price change (absolute).
    pub trailing1_change: f64,

    /// Trailing 7-day price.
    pub trailing7: f64,
    /// Trailing 7-day price change (absolute).
    pub trailing7_change: f64,

    /// Trailing 30-day price.
    pub trailing30: f64,
    /// Trailing 30-day price change (absolute).
    pub trailing30_change: f64,

    /// Trailing 90-day price.
    pub trailing90: f64,
    /// Trailing 90-day price change (absolute).
    pub trailing90_change: f64,

    /// Trailing 365-day price.
    pub trailing365: f64,
    /// Trailing 365-day price change (absolute).
    pub trailing365_change: f64,

    /// Week-to-date price.
    pub wtd: f64,
    /// Week-to-date price change (absolute).
    pub wtd_change: f64,

    /// Month-to-date price.
    pub mtd: f64,
    /// Month-to-date price change (absolute).
    pub mtd_change: f64,

    /// Quarter-to-date price.
    pub qtd: f64,
    /// Quarter-to-date price change (absolute).
    pub qtd_change: f64,

    /// Year-to-date price.
    pub ytd: f64,
    /// Year-to-date price change (absolute).
    pub ytd_change: f64,
}
impl Performance {
    /// Returns the most recent end-of-day price, if available.
    pub fn last_price(&self) -> Option<f64> {
        EodPrice::last_price_from_vec(&self.eod_prices)
    }
}

/// Aggregate trading statistics for an issuer.
#[derive(Serialize, Deserialize)]
pub struct Stats {
    /// Total number of trades involving this issuer.
    #[serde(rename = "countTrades")]
    pub count_trades: i64,

    /// Number of distinct politicians who traded this issuer.
    #[serde(rename = "countPoliticians")]
    pub count_politicians: i64,

    /// Total estimated dollar volume of trades.
    #[serde(rename = "volume")]
    pub volume: i64,

    /// Date of the most recent trade for this issuer.
    #[serde(rename = "dateLastTraded")]
    pub date_last_traded: NaiveDate,
}

/// A single value in an end-of-day price array. Can be either a price or a date.
///
/// The API returns EOD data as untagged arrays mixing dates and floats.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum EodPrice {
    /// A numeric price value.
    Double(f64),

    /// A date associated with the price data.
    NaiveDate(NaiveDate),
}
impl EodPrice {
    /// Extracts the first numeric price from a nested EOD price array.
    pub fn last_price_from_vec(v: &[Vec<EodPrice>]) -> Option<f64> {
        if v.is_empty() {
            return None;
        }
        for item in v.first().unwrap() {
            match item {
                EodPrice::Double(price) => return Some(*price),
                _ => continue,
            }
        }
        None
    }
}

/// Market capitalization bracket. Discriminant values (1-6) are sent to the API.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum MarketCap {
    /// Greater than $200B.
    Mega = 1,
    /// $10B to $200B.
    Large = 2,
    /// $2B to $10B.
    Mid = 3,
    /// $300M to $2B.
    Small = 4,
    /// $50M to $300M.
    Micro = 5,
    /// Less than $50M.
    Nano = 6,
}

impl std::fmt::Display for MarketCap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self as u8)
    }
}

/// GICS sector classification for an issuer. Serialized as kebab-case.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Sector {
    /// Telecom, media, entertainment.
    CommunicationServices,
    /// Retail, automotive, apparel.
    ConsumerDiscretionary,
    /// Food, beverages, household products.
    ConsumerStaples,
    /// Oil, gas, renewables.
    Energy,
    /// Banks, insurance, capital markets.
    Financials,
    /// Pharma, biotech, medical devices.
    HealthCare,
    /// Aerospace, construction, logistics.
    Industrials,
    /// Software, hardware, semiconductors.
    InformationTechnology,
    /// Chemicals, metals, packaging.
    Materials,
    /// REITs, property management.
    RealEstate,
    /// Electric, gas, water utilities.
    Utilities,
    /// Uncategorized.
    Other,
}
impl std::fmt::Display for Sector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Sector::CommunicationServices => "communication-services",
                Sector::ConsumerDiscretionary => "consumer-discretionary",
                Sector::ConsumerStaples => "consumer-staples",
                Sector::Energy => "energy",
                Sector::Financials => "financials",
                Sector::HealthCare => "health-care",
                Sector::Industrials => "industrials",
                Sector::InformationTechnology => "information-technology",
                Sector::Materials => "materials",
                Sector::RealEstate => "real-estate",
                Sector::Utilities => "utilities",
                Sector::Other => "other",
            }
        )?;
        Ok(())
    }
}
