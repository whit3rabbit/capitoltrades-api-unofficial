use std::str::FromStr;

use url::Url;

use crate::types::{MarketCap, PoliticianID, Sector};

use super::{
    common::{QueryCommon, SortDirection},
    Query,
};

#[derive(Default)]
pub struct IssuerQuery {
    pub common: QueryCommon,
    pub search: Option<String>,
    pub politician_ids: Vec<PoliticianID>,
    pub market_caps: Vec<MarketCap>,
    pub sectors: Vec<Sector>,
    pub countries: Vec<String>,
    pub states: Vec<String>,
    pub sort_by: IssuerSortBy,
}

impl Query for IssuerQuery {
    fn get_common(&mut self) -> &mut QueryCommon {
        &mut self.common
    }
    fn add_to_url(&self, url: &Url) -> Url {
        let mut url = url.clone();
        if let Some(search) = &self.search {
            url.query_pairs_mut().append_pair("search", search.as_str());
        };
        for politician_id in self.politician_ids.iter() {
            url.query_pairs_mut()
                .append_pair("politician", &politician_id.to_string());
        }
        for market_cap in self.market_caps.iter() {
            url.query_pairs_mut()
                .append_pair("mcap", (*market_cap as u8).to_string().as_str());
        }
        for sector in self.sectors.iter() {
            url.query_pairs_mut()
                .append_pair("sector", sector.to_string().as_str());
        }
        for country in self.countries.iter() {
            url.query_pairs_mut()
                .append_pair("country", country.as_str());
        }
        for state in self.states.iter() {
            url.query_pairs_mut().append_pair("state", state.as_str());
        }

        url.query_pairs_mut().append_pair(
            "sortBy",
            format!(
                "{}{}",
                match self.common.sort_direction {
                    SortDirection::Asc => "",
                    SortDirection::Desc => "-",
                },
                &self.sort_by.to_string().as_str()
            )
            .as_str(),
        );

        url
    }
}

impl IssuerQuery {
    pub fn with_search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    pub fn with_politician_id(mut self, politician_id: PoliticianID) -> Self {
        self.politician_ids.push(politician_id);
        self
    }
    pub fn with_politician_ids(mut self, politician_ids: &[PoliticianID]) -> Self {
        self.politician_ids.extend_from_slice(politician_ids);
        self
    }

    pub fn with_market_cap(mut self, market_cap: MarketCap) -> Self {
        self.market_caps.push(market_cap);
        self
    }
    pub fn with_market_caps(mut self, market_caps: &[MarketCap]) -> Self {
        self.market_caps.extend_from_slice(market_caps);
        self
    }

    pub fn with_sector(mut self, sector: Sector) -> Self {
        self.sectors.push(sector);
        self
    }
    pub fn with_sectors(mut self, sectors: &[Sector]) -> Self {
        self.sectors.extend_from_slice(sectors);
        self
    }

    pub fn with_country(mut self, country: &str) -> Self {
        self.countries.push(country.to_string());
        self
    }
    pub fn with_countries(mut self, countries: &[String]) -> Self {
        self.countries.extend_from_slice(countries);
        self
    }

    pub fn with_state(mut self, state: &str) -> Self {
        self.states.push(state.to_string());
        self
    }
    pub fn with_states(mut self, states: &[String]) -> Self {
        self.states.extend_from_slice(states);
        self
    }

    pub fn with_sort_by(mut self, sort_by: IssuerSortBy) -> Self {
        self.sort_by = sort_by;
        self
    }
}

#[derive(Clone, Copy)]
pub enum IssuerSortBy {
    TradedVolume,
    PoliticiansCount,
    TotalTrades,
    DateLastTraded,
    MarketCap,
}
impl Default for IssuerSortBy {
    fn default() -> Self {
        IssuerSortBy::TradedVolume
    }
}
impl std::fmt::Display for IssuerSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                IssuerSortBy::TradedVolume => "volume",
                IssuerSortBy::PoliticiansCount => "countPoliticians",
                IssuerSortBy::TotalTrades => "countTrades",
                IssuerSortBy::DateLastTraded => "dateLastTraded",
                IssuerSortBy::MarketCap => "mcap",
            }
        )?;
        Ok(())
    }
}
impl FromStr for IssuerSortBy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(IssuerSortBy::TradedVolume),
            "1" => Ok(IssuerSortBy::PoliticiansCount),
            "2" => Ok(IssuerSortBy::TotalTrades),
            "3" => Ok(IssuerSortBy::DateLastTraded),
            "4" => Ok(IssuerSortBy::MarketCap),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::{
        query::{common::SortDirection, issuer::IssuerSortBy, IssuerQuery, Query},
        types::{MarketCap, Sector},
    };

    #[test]
    fn test_issuer_query() {
        let url = Url::parse("https://example.com").unwrap();

        insta::assert_yaml_snapshot!(IssuerQuery::default()
            .with_search("search")
            .with_page(1)
            .with_page_size(10)
            .with_tx_date_relative(10)
            .with_pub_date_relative(10)
            .with_country("IT")
            .with_countries(&["US".to_string(), "CA".to_string()])
            .with_state("CA")
            .with_states(&["NY".to_string(), "TX".to_string()])
            .with_market_cap(MarketCap::Small)
            .with_market_caps(&[MarketCap::Mid, MarketCap::Large])
            .with_politician_id(1.to_string())
            .with_politician_ids(&[2.to_string(), 3.to_string()])
            .with_sector(Sector::InformationTechnology)
            .with_sectors(&[Sector::HealthCare, Sector::ConsumerDiscretionary])
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(IssuerQuery::default()
            .with_sort_direction(SortDirection::Asc)
            .with_sort_by(IssuerSortBy::MarketCap)
            .add_to_url(&url)
            .to_string());
    }
}
