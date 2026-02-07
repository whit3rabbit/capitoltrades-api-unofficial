//! Trade-related types: the core `Trade` struct and supporting enums.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use super::{
    issuer::Sector,
    politician::{Politician, PoliticianID},
    Chamber, IssuerID,
};

extern crate serde_json;

/// Trade size bracket used by the API for filtering by dollar amount range.
///
/// Each variant's discriminant (1-10) is the value sent to the API.
#[derive(Copy, Clone)]
pub enum TradeSize {
    /// Less than $1,000.
    Less1K = 1,
    /// $1,000 to $15,000.
    From1Kto15K = 2,
    /// $15,000 to $50,000.
    From15Kto50K = 3,
    /// $50,000 to $100,000.
    From50Kto100K = 4,
    /// $100,000 to $250,000.
    From100Kto250K = 5,
    /// $250,000 to $500,000.
    From250Kto500K = 6,
    /// $500,000 to $1,000,000.
    From500Kto1M = 7,
    /// $1,000,000 to $5,000,000.
    From1Mto5M = 8,
    /// $5,000,000 to $25,000,000.
    From5Mto25M = 9,
    /// $25,000,000 to $50,000,000.
    From25Mto50M = 10,
}

/// A single financial trade disclosed by a member of Congress.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    /// Unique transaction identifier.
    #[serde(rename = "_txId")]
    pub tx_id: i64,

    /// ID of the politician who made the trade (e.g. "P000197").
    #[serde(rename = "_politicianId")]
    pub politician_id: PoliticianID,

    #[serde(rename = "_assetId")]
    asset_id: i64,

    /// Numeric ID of the issuer (company/fund) traded.
    #[serde(rename = "_issuerId")]
    pub issuer_id: IssuerID,

    /// Date the trade disclosure was published.
    pub pub_date: DateTime<Utc>,

    /// Date the financial disclosure was filed.
    pub filing_date: NaiveDate,

    /// Date the transaction occurred.
    pub tx_date: NaiveDate,

    /// Whether this was a buy, sell, exchange, or receive.
    pub tx_type: TxType,

    tx_type_extended: Option<serde_json::Value>,

    has_capital_gains: bool,

    owner: Owner,

    chamber: Chamber,

    /// Price per share at time of trade, if available.
    pub price: Option<f64>,

    /// Trade size bracket number (1-10), if reported.
    pub size: Option<i64>,

    size_range_high: Option<i64>,

    size_range_low: Option<i64>,

    /// Midpoint dollar value of the trade size bracket.
    pub value: i64,

    filing_id: i64,

    /// URL to the original financial disclosure filing.
    #[serde(rename = "filingURL")]
    pub filing_url: String,

    /// Days between the transaction date and the publication date.
    pub reporting_gap: i64,

    comment: Option<String>,

    committees: Vec<String>,

    /// Asset details (type, ticker, instrument).
    pub asset: Asset,

    /// Issuer summary embedded in the trade response.
    pub issuer: Issuer,

    /// Politician summary embedded in the trade response.
    pub politician: Politician,

    labels: Vec<String>,
}

/// Asset details associated with a trade.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// Type of financial instrument (e.g. "stock", "etf").
    pub asset_type: String,

    /// Ticker symbol, if applicable.
    pub asset_ticker: Option<String>,

    /// Instrument description, if applicable.
    pub instrument: Option<String>,
}

/// Issuer summary embedded within a [`Trade`] response.
///
/// This is a lighter representation than [`super::issuer::IssuerDetail`].
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issuer {
    #[serde(rename = "_stateId")]
    state_id: Option<String>,

    #[serde(rename = "c2iq")]
    c2_iq: Option<String>,

    country: Option<String>,

    /// Company or fund name.
    pub issuer_name: String,

    /// Ticker symbol, if publicly traded.
    pub issuer_ticker: Option<String>,

    sector: Option<Sector>,
}

/// Ownership category for the traded asset.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(clippy::enum_variant_names)]
pub enum Owner {
    /// Owned by the politician's child.
    Child,
    /// Jointly owned.
    Joint,
    /// Ownership not disclosed.
    NotDisclosed,
    /// Owned by the politician themselves.
    #[serde(rename = "self")]
    OwnerSelf,
    /// Owned by the politician's spouse.
    Spouse,
}

/// Transaction type for a trade.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum TxType {
    /// Purchase of an asset.
    Buy,
    /// Sale of an asset.
    Sell,
    /// Exchange of one asset for another.
    Exchange,
    /// Receipt of an asset (e.g. gift, inheritance).
    Receive,
}
impl std::fmt::Display for TxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TxType::Buy => "buy",
                TxType::Sell => "sell",
                TxType::Exchange => "exchange",
                TxType::Receive => "receive",
            }
        )
    }
}

/// Type of financial asset traded. Added in our fork; not present upstream.
///
/// Serialized as kebab-case strings for the API (e.g. `"stock-option"`).
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum AssetType {
    /// Common or preferred stock.
    Stock,
    /// Stock option contract.
    StockOption,
    /// Corporate bond.
    CorporateBond,
    /// Exchange-traded fund.
    Etf,
    /// Exchange-traded note.
    Etn,
    /// Mutual fund.
    MutualFund,
    /// Cryptocurrency.
    Cryptocurrency,
    /// Private debt fund.
    Pdf,
    /// Municipal security.
    MunicipalSecurity,
    /// Non-publicly traded stock.
    NonPublicStock,
    /// Other / uncategorized asset.
    Other,
    /// Real estate investment trust.
    Reit,
    /// Commodity.
    Commodity,
    /// Hedge fund.
    Hedge,
    /// Variable insurance product.
    VariableInsurance,
    /// Private equity.
    PrivateEquity,
    /// Closed-end fund.
    ClosedEndFund,
    /// Venture capital fund.
    Venture,
    /// Index fund.
    IndexFund,
    /// Government bond.
    GovernmentBond,
    /// Money market fund.
    MoneyMarketFund,
    /// Brokered CD or similar.
    Brokered,
}
impl std::fmt::Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AssetType::Stock => "stock",
                AssetType::StockOption => "stock-option",
                AssetType::CorporateBond => "corporate-bond",
                AssetType::Etf => "etf",
                AssetType::Etn => "etn",
                AssetType::MutualFund => "mutual-fund",
                AssetType::Cryptocurrency => "cryptocurrency",
                AssetType::Pdf => "pdf",
                AssetType::MunicipalSecurity => "municipal-security",
                AssetType::NonPublicStock => "non-public-stock",
                AssetType::Other => "other",
                AssetType::Reit => "reit",
                AssetType::Commodity => "commodity",
                AssetType::Hedge => "hedge",
                AssetType::VariableInsurance => "variable-insurance",
                AssetType::PrivateEquity => "private-equity",
                AssetType::ClosedEndFund => "closed-end-fund",
                AssetType::Venture => "venture",
                AssetType::IndexFund => "index-fund",
                AssetType::GovernmentBond => "government-bond",
                AssetType::MoneyMarketFund => "money-market-fund",
                AssetType::Brokered => "brokered",
            }
        )
    }
}

/// Curated label applied to issuers by CapitolTrades. Added in our fork.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum Label {
    /// Facebook/Apple/Amazon/Netflix/Google mega-cap tech.
    Faang,
    /// Cryptocurrency-related issuers.
    Crypto,
    /// Meme stock (e.g. GME, AMC).
    Memestock,
    /// Special purpose acquisition company.
    Spac,
}
impl std::fmt::Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Label::Faang => "faang",
                Label::Crypto => "crypto",
                Label::Memestock => "memestock",
                Label::Spac => "spac",
            }
        )
    }
}
