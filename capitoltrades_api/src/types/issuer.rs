use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

extern crate serde_json;

pub type IssuerID = i64;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IssuerDetail {
    #[serde(rename = "_issuerId")]
    pub issuer_id: IssuerID,

    #[serde(rename = "_stateId")]
    state_id: Option<String>,

    #[serde(rename = "c2iq")]
    c2_iq: Option<String>,

    country: Option<String>,

    pub issuer_name: String,

    pub issuer_ticker: Option<String>,

    pub performance: Option<Performance>,

    pub sector: Option<Sector>,

    pub stats: Stats,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Performance {
    pub eod_prices: Vec<Vec<EodPrice>>,

    pub mcap: i64,

    pub trailing1: f64,

    pub trailing1_change: f64,

    pub trailing7: f64,

    pub trailing7_change: f64,

    pub trailing30: f64,

    pub trailing30_change: f64,

    pub trailing90: f64,

    pub trailing90_change: f64,

    pub trailing365: f64,

    pub trailing365_change: f64,

    pub wtd: f64,

    pub wtd_change: f64,

    pub mtd: f64,

    pub mtd_change: f64,

    pub qtd: f64,

    pub qtd_change: f64,

    pub ytd: f64,

    pub ytd_change: f64,
}
impl Performance {
    pub fn last_price(&self) -> Option<f64> {
        EodPrice::last_price_from_vec(&self.eod_prices)
    }
}

#[derive(Serialize, Deserialize)]
pub struct Stats {
    #[serde(rename = "countTrades")]
    pub count_trades: i64,

    #[serde(rename = "countPoliticians")]
    pub count_politicians: i64,

    #[serde(rename = "volume")]
    pub volume: i64,

    #[serde(rename = "dateLastTraded")]
    pub date_last_traded: NaiveDate,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum EodPrice {
    Double(f64),

    NaiveDate(NaiveDate),
}
impl EodPrice {
    pub fn last_price_from_vec(v: &[Vec<EodPrice>]) -> Option<f64> {
        if v.is_empty() {
            return None;
        }
        for item in v.get(0).unwrap() {
            match item {
                EodPrice::Double(price) => return Some(*price),
                _ => continue,
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum MarketCap {
    /// >200B
    Mega = 1,
    /// 10B-200B
    Large = 2,
    /// 2B-10B
    Mid = 3,
    /// 300M-2B
    Small = 4,
    /// 50M-300M
    Micro = 5,
    /// <50M
    Nano = 6,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Sector {
    CommunicationServices,
    ConsumerDiscretionary,
    ConsumerStaples,
    Energy,
    Financials,
    HealthCare,
    Industrials,
    InformationTechnology,
    Materials,
    RealEstate,
    Utilities,
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
