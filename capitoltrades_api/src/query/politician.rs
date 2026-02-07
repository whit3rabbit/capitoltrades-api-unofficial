//! Query builder for the `/politicians` endpoint.

use std::str::FromStr;

use url::Url;

use crate::types::{IssuerID, Party};

use super::{
    common::{QueryCommon, SortDirection},
    Query,
};

/// Query builder for the `/politicians` endpoint.
///
/// Supports filtering by issuer, party, state, committee, and free-text search.
#[derive(Default)]
pub struct PoliticianQuery {
    /// Shared pagination and date filter fields.
    pub common: QueryCommon,
    /// Filter by issuer numeric IDs (shows politicians who traded these issuers).
    pub issuer_ids: Vec<IssuerID>,
    /// Filter by political party.
    pub parties: Vec<Party>,
    /// Filter by US state codes (uppercase 2-letter).
    pub states: Vec<String>,
    /// Filter by committee abbreviation codes.
    pub committees: Vec<String>,
    /// Free-text search by politician name.
    pub search: Option<String>,
    /// Field to sort results by.
    pub sort_by: PoliticianSortBy,
}

impl Query for PoliticianQuery {
    fn get_common(&mut self) -> &mut QueryCommon {
        &mut self.common
    }
    fn add_to_url(&self, url: &Url) -> Url {
        let mut url = self.common.add_to_url(url);
        for issuer_id in self.issuer_ids.iter() {
            url.query_pairs_mut()
                .append_pair("issuer", &issuer_id.to_string());
        }
        for party in self.parties.iter() {
            url.query_pairs_mut()
                .append_pair("party", party.to_string().as_str());
        }
        for state in self.states.iter() {
            url.query_pairs_mut().append_pair("state", state.as_str());
        }
        for committee in self.committees.iter() {
            url.query_pairs_mut()
                .append_pair("committee", committee.as_str());
        }
        if let Some(search) = &self.search {
            url.query_pairs_mut().append_pair("search", search.as_str());
        };

        url.query_pairs_mut().append_pair(
            "sortBy",
            format!(
                "{}{}",
                match self.common.sort_direction {
                    SortDirection::Asc => "",
                    SortDirection::Desc => "-",
                },
                self.sort_by.to_string().as_str()
            )
            .as_str(),
        );

        url
    }
}

impl PoliticianQuery {
    /// Adds a single issuer ID filter.
    pub fn with_issuer_id(mut self, issuer_id: IssuerID) -> Self {
        self.issuer_ids.push(issuer_id);
        self
    }
    /// Adds multiple issuer ID filters.
    pub fn with_issuer_ids(mut self, issuer_ids: &[IssuerID]) -> Self {
        self.issuer_ids.extend_from_slice(issuer_ids);
        self
    }

    /// Adds a single party filter.
    pub fn with_party(mut self, party: &Party) -> Self {
        self.parties.push(party.clone());
        self
    }
    /// Adds multiple party filters.
    pub fn with_parties(mut self, parties: &[Party]) -> Self {
        self.parties.extend_from_slice(parties);
        self
    }

    /// Adds a single state filter (uppercase 2-letter code).
    pub fn with_state(mut self, state: &str) -> Self {
        self.states.push(state.to_string());
        self
    }
    /// Adds multiple state filters.
    pub fn with_states(mut self, states: &[String]) -> Self {
        self.states.extend_from_slice(states);
        self
    }

    /// Adds a single committee filter (abbreviation code, e.g. "ssfi").
    pub fn with_committee(mut self, committee: &str) -> Self {
        self.committees.push(committee.to_string());
        self
    }
    /// Adds multiple committee filters.
    pub fn with_committees(mut self, committees: &[String]) -> Self {
        self.committees.extend_from_slice(committees);
        self
    }

    /// Sets the free-text search query (searches by politician name).
    pub fn with_search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    /// Sets the field to sort results by.
    pub fn with_sort_by(mut self, sort_by: PoliticianSortBy) -> Self {
        self.sort_by = sort_by;
        self
    }
}

/// Sort field for politician queries.
#[derive(Clone, Copy, Default)]
pub enum PoliticianSortBy {
    /// Sort by total traded dollar volume (default).
    #[default]
    TradedVolume = 0,
    /// Sort alphabetically by last name.
    LastName = 1,
    /// Sort by number of distinct issuers traded.
    TradedIssuersCount = 2,
    /// Sort by total number of trades.
    TotalTrades = 3,
    /// Sort by date of most recent trade.
    DateLastTraded = 4,
}
impl std::fmt::Display for PoliticianSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PoliticianSortBy::TradedVolume => "volume",
                PoliticianSortBy::LastName => "lastName",
                PoliticianSortBy::TradedIssuersCount => "countIssuers",
                PoliticianSortBy::TotalTrades => "countTrades",
                PoliticianSortBy::DateLastTraded => "dateLastTraded",
            }
        )?;
        Ok(())
    }
}
impl FromStr for PoliticianSortBy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(PoliticianSortBy::TradedVolume),
            "1" => Ok(PoliticianSortBy::LastName),
            "2" => Ok(PoliticianSortBy::TradedIssuersCount),
            "3" => Ok(PoliticianSortBy::TotalTrades),
            "4" => Ok(PoliticianSortBy::DateLastTraded),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::{
        query::{common::SortDirection, politician::PoliticianSortBy, PoliticianQuery, Query},
        types::Party,
    };

    #[test]
    fn test_politician_query() {
        let url = Url::parse("https://example.com").unwrap();

        insta::assert_yaml_snapshot!(PoliticianQuery::default().add_to_url(&url).to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_issuer_id(123)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_issuer_id(123)
            .with_issuer_id(124)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_issuer_id(123)
            .with_issuer_id(124)
            .with_page(1)
            .with_page_size(12)
            .with_party(&Party::Democrat)
            .with_party(&Party::Republican)
            .with_party(&Party::Other)
            .with_search("value")
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_issuer_ids(vec![1, 2, 3, 4].as_slice())
            .with_parties(vec![Party::Democrat, Party::Republican, Party::Other].as_slice(),)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_sort_by(PoliticianSortBy::LastName)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_sort_direction(SortDirection::Desc)
            .with_sort_by(PoliticianSortBy::LastName)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(PoliticianQuery::default()
            .with_sort_direction(SortDirection::Asc)
            .with_sort_by(PoliticianSortBy::LastName)
            .add_to_url(&url)
            .to_string());
    }
}
