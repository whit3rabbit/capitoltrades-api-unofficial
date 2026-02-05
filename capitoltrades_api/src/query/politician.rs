use std::str::FromStr;

use url::Url;

use crate::types::{IssuerID, Party};

use super::{
    common::{QueryCommon, SortDirection},
    Query,
};

#[derive(Default)]
pub struct PoliticianQuery {
    pub common: QueryCommon,
    pub issuer_ids: Vec<IssuerID>,
    pub parties: Vec<Party>,
    pub search: Option<String>,
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
    pub fn with_issuer_id(mut self, issuer_id: IssuerID) -> Self {
        self.issuer_ids.push(issuer_id);
        self
    }
    pub fn with_issuer_ids(mut self, issuer_ids: &[IssuerID]) -> Self {
        self.issuer_ids.extend_from_slice(issuer_ids);
        self
    }

    pub fn with_party(mut self, party: &Party) -> Self {
        self.parties.push(party.clone());
        self
    }
    pub fn with_parties(mut self, parties: &[Party]) -> Self {
        self.parties.extend_from_slice(parties);
        self
    }

    pub fn with_search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    pub fn with_sort_by(mut self, sort_by: PoliticianSortBy) -> Self {
        self.sort_by = sort_by;
        self
    }
}

#[derive(Clone, Copy)]
pub enum PoliticianSortBy {
    TradedVolume = 0,
    LastName = 1,
    TradedIssuersCount = 2,
    TotalTrades = 3,
    DateLastTraded = 4,
}
impl Default for PoliticianSortBy {
    fn default() -> Self {
        PoliticianSortBy::TradedVolume
    }
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
