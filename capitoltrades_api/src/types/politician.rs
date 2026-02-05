use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub type PoliticianID = String;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Politician {
    #[serde(rename = "_stateId")]
    pub state_id: String,

    pub chamber: Chamber,

    dob: String,

    pub first_name: String,

    gender: Gender,

    pub last_name: String,

    nickname: Option<String>,

    pub party: Party,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoliticianDetail {
    #[serde(rename = "_politicianId")]
    pub politician_id: PoliticianID,

    #[serde(rename = "_stateId")]
    pub state_id: String,

    pub party: Party,

    party_other: Option<serde_json::Value>,

    district: Option<String>,

    pub first_name: String,

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

    pub chamber: Chamber,

    committees: Vec<String>,

    pub stats: Stats,
}
impl Into<Politician> for PoliticianDetail {
    fn into(self) -> Politician {
        Politician {
            state_id: self.state_id,
            chamber: self.chamber,
            dob: self.dob,
            first_name: self.first_name,
            gender: self.gender,
            last_name: self.last_name,
            nickname: self.nickname,
            party: self.party,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub date_last_traded: Option<NaiveDate>,

    pub count_trades: i64,

    pub count_issuers: i64,

    pub volume: i64,
}

#[derive(Serialize, Deserialize)]
pub enum Chamber {
    #[serde(rename = "house")]
    House,

    #[serde(rename = "senate")]
    Senate,
}

#[derive(Serialize, Deserialize)]
pub enum Gender {
    #[serde(rename = "female")]
    Female,

    #[serde(rename = "male")]
    Male,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Party {
    #[serde(rename = "democrat")]
    Democrat,

    #[serde(rename = "republican")]
    Republican,

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
