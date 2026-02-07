//! Politician-related types returned by the API.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Unique identifier for a politician (e.g. "P000197").
pub type PoliticianID = String;

/// Summary representation of a politician, embedded in trade responses.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Politician {
    /// Two-letter US state code (uppercase).
    #[serde(rename = "_stateId")]
    pub state_id: String,

    /// House or Senate.
    pub chamber: Chamber,

    dob: String,

    /// Politician's first name.
    pub first_name: String,

    gender: Gender,

    /// Politician's last name.
    pub last_name: String,

    nickname: Option<String>,

    /// Political party affiliation.
    pub party: Party,
}

/// Full politician record returned by the `/politicians` endpoint.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoliticianDetail {
    /// Unique politician identifier (e.g. "P000197").
    #[serde(rename = "_politicianId")]
    pub politician_id: PoliticianID,

    /// Two-letter US state code (uppercase).
    #[serde(rename = "_stateId")]
    pub state_id: String,

    /// Political party affiliation.
    pub party: Party,

    party_other: Option<serde_json::Value>,

    district: Option<String>,

    /// Politician's first name.
    pub first_name: String,

    /// Politician's last name.
    pub last_name: String,

    nickname: Option<String>,

    middle_name: Option<String>,

    full_name: String,

    dob: String,

    gender: Gender,

    social_facebook: Option<String>,

    social_twitter: Option<String>,

    social_youtube: Option<String>,

    website: Option<String>,

    /// House or Senate.
    pub chamber: Chamber,

    committees: Vec<String>,

    /// Aggregate trading statistics for this politician.
    pub stats: Stats,
}
impl From<PoliticianDetail> for Politician {
    fn from(val: PoliticianDetail) -> Self {
        Politician {
            state_id: val.state_id,
            chamber: val.chamber,
            dob: val.dob,
            first_name: val.first_name,
            gender: val.gender,
            last_name: val.last_name,
            nickname: val.nickname,
            party: val.party,
        }
    }
}

/// Aggregate trading statistics for a politician.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Date of the most recent trade, if any.
    pub date_last_traded: Option<NaiveDate>,

    /// Total number of trades disclosed.
    pub count_trades: i64,

    /// Number of distinct issuers traded.
    pub count_issuers: i64,

    /// Total estimated dollar volume across all trades.
    pub volume: i64,
}

/// Congressional chamber.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum Chamber {
    /// US House of Representatives.
    #[serde(rename = "house")]
    House,

    /// US Senate.
    #[serde(rename = "senate")]
    Senate,
}
impl std::fmt::Display for Chamber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Chamber::House => "house",
                Chamber::Senate => "senate",
            }
        )
    }
}

/// Politician gender as reported by the API.
#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum Gender {
    /// Female.
    #[serde(rename = "female")]
    Female,

    /// Male.
    #[serde(rename = "male")]
    Male,
}
impl std::fmt::Display for Gender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Gender::Female => "female",
                Gender::Male => "male",
            }
        )
    }
}

/// Political party affiliation.
#[derive(Serialize, Deserialize, Clone)]
pub enum Party {
    /// Democratic Party.
    #[serde(rename = "democrat")]
    Democrat,

    /// Republican Party.
    #[serde(rename = "republican")]
    Republican,

    /// Independent or third party.
    #[serde(rename = "other")]
    Other,
}
impl std::fmt::Display for Party {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Party::Democrat => "democrat",
                Party::Republican => "republican",
                Party::Other => "other",
            }
        )?;
        Ok(())
    }
}
