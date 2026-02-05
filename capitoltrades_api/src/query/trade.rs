use std::str::FromStr;

use url::Url;

use crate::types::{
    AssetType, Chamber, Gender, IssuerID, Label, MarketCap, Party, PoliticianID, Sector, TradeSize,
    TxType,
};

use super::common::{Query, QueryCommon, SortDirection};

#[derive(Default)]
pub struct TradeQuery {
    pub common: QueryCommon,
    pub issuer_ids: Vec<IssuerID>,
    pub trade_sizes: Vec<TradeSize>,
    pub parties: Vec<Party>,
    pub states: Vec<String>,
    pub committees: Vec<String>,
    pub search: Option<String>,
    pub sort_by: TradeSortBy,
    pub genders: Vec<Gender>,
    pub market_caps: Vec<MarketCap>,
    pub asset_types: Vec<AssetType>,
    pub labels: Vec<Label>,
    pub sectors: Vec<Sector>,
    pub tx_types: Vec<TxType>,
    pub chambers: Vec<Chamber>,
    pub politician_ids: Vec<PoliticianID>,
    pub issuer_states: Vec<String>,
    pub countries: Vec<String>,
}

impl Query for TradeQuery {
    fn get_common(&mut self) -> &mut QueryCommon {
        &mut self.common
    }
    fn add_to_url(&self, url: &Url) -> Url {
        let mut url = self.common.add_to_url(url);
        for issuer_id in self.issuer_ids.iter() {
            url.query_pairs_mut()
                .append_pair("issuer", &issuer_id.to_string());
        }
        for trade_size in self.trade_sizes.iter() {
            url.query_pairs_mut()
                .append_pair("tradeSize", (*trade_size as u8).to_string().as_str());
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
        }
        for gender in self.genders.iter() {
            url.query_pairs_mut()
                .append_pair("gender", gender.to_string().as_str());
        }
        for mcap in self.market_caps.iter() {
            url.query_pairs_mut()
                .append_pair("mcap", &mcap.to_string());
        }
        for asset_type in self.asset_types.iter() {
            url.query_pairs_mut()
                .append_pair("assetType", asset_type.to_string().as_str());
        }
        for label in self.labels.iter() {
            url.query_pairs_mut()
                .append_pair("label", label.to_string().as_str());
        }
        for sector in self.sectors.iter() {
            url.query_pairs_mut()
                .append_pair("sector", sector.to_string().as_str());
        }
        for tx_type in self.tx_types.iter() {
            url.query_pairs_mut()
                .append_pair("txType", tx_type.to_string().as_str());
        }
        for chamber in self.chambers.iter() {
            url.query_pairs_mut()
                .append_pair("chamber", chamber.to_string().as_str());
        }
        for politician_id in self.politician_ids.iter() {
            url.query_pairs_mut()
                .append_pair("politician", politician_id.as_str());
        }
        for issuer_state in self.issuer_states.iter() {
            url.query_pairs_mut()
                .append_pair("issuerState", issuer_state.as_str());
        }
        for country in self.countries.iter() {
            url.query_pairs_mut()
                .append_pair("country", country.as_str());
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

impl TradeQuery {
    pub fn with_issuer_id(mut self, issuer_id: IssuerID) -> Self {
        self.issuer_ids.push(issuer_id);
        self
    }
    pub fn with_issuer_ids(mut self, issuer_ids: &[IssuerID]) -> Self {
        self.issuer_ids.extend_from_slice(issuer_ids);
        self
    }
    pub fn with_trade_size(mut self, trade_size: TradeSize) -> Self {
        self.trade_sizes.push(trade_size);
        self
    }
    pub fn with_trade_sizes(mut self, trade_sizes: &[TradeSize]) -> Self {
        self.trade_sizes.extend_from_slice(trade_sizes);
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

    pub fn with_state(mut self, state: &str) -> Self {
        self.states.push(state.to_string());
        self
    }
    pub fn with_states(mut self, states: &[String]) -> Self {
        self.states.extend_from_slice(states);
        self
    }

    pub fn with_committee(mut self, committee: &str) -> Self {
        self.committees.push(committee.to_string());
        self
    }
    pub fn with_committees(mut self, committees: &[String]) -> Self {
        self.committees.extend_from_slice(committees);
        self
    }

    pub fn with_search(mut self, search: &str) -> Self {
        self.search = Some(search.to_string());
        self
    }

    pub fn with_sort_by(mut self, sort_by: TradeSortBy) -> Self {
        self.sort_by = sort_by;
        self
    }

    pub fn with_gender(mut self, gender: Gender) -> Self {
        self.genders.push(gender);
        self
    }
    pub fn with_genders(mut self, genders: &[Gender]) -> Self {
        self.genders.extend_from_slice(genders);
        self
    }

    pub fn with_market_cap(mut self, mcap: MarketCap) -> Self {
        self.market_caps.push(mcap);
        self
    }
    pub fn with_market_caps(mut self, mcaps: &[MarketCap]) -> Self {
        self.market_caps.extend_from_slice(mcaps);
        self
    }

    pub fn with_asset_type(mut self, asset_type: AssetType) -> Self {
        self.asset_types.push(asset_type);
        self
    }
    pub fn with_asset_types(mut self, asset_types: &[AssetType]) -> Self {
        self.asset_types.extend_from_slice(asset_types);
        self
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }
    pub fn with_labels(mut self, labels: &[Label]) -> Self {
        self.labels.extend_from_slice(labels);
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

    pub fn with_tx_type(mut self, tx_type: TxType) -> Self {
        self.tx_types.push(tx_type);
        self
    }
    pub fn with_tx_types(mut self, tx_types: &[TxType]) -> Self {
        self.tx_types.extend_from_slice(tx_types);
        self
    }

    pub fn with_chamber(mut self, chamber: Chamber) -> Self {
        self.chambers.push(chamber);
        self
    }
    pub fn with_chambers(mut self, chambers: &[Chamber]) -> Self {
        self.chambers.extend_from_slice(chambers);
        self
    }

    pub fn with_politician_id(mut self, politician_id: &str) -> Self {
        self.politician_ids.push(politician_id.to_string());
        self
    }
    pub fn with_politician_ids(mut self, politician_ids: &[String]) -> Self {
        self.politician_ids.extend_from_slice(politician_ids);
        self
    }

    pub fn with_issuer_state(mut self, issuer_state: &str) -> Self {
        self.issuer_states.push(issuer_state.to_string());
        self
    }
    pub fn with_issuer_states(mut self, issuer_states: &[String]) -> Self {
        self.issuer_states.extend_from_slice(issuer_states);
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
}

#[derive(Clone, Copy)]
pub enum TradeSortBy {
    PublicationDate = 0,
    TradeDate = 1,
    ReportingGap = 2,
}
impl Default for TradeSortBy {
    fn default() -> Self {
        TradeSortBy::PublicationDate
    }
}
impl std::fmt::Display for TradeSortBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TradeSortBy::PublicationDate => "pubDate",
                TradeSortBy::TradeDate => "txDate",
                TradeSortBy::ReportingGap => "reportingGap",
            }
        )?;
        Ok(())
    }
}
impl FromStr for TradeSortBy {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(TradeSortBy::PublicationDate),
            "1" => Ok(TradeSortBy::TradeDate),
            "2" => Ok(TradeSortBy::ReportingGap),
            _ => Err(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use crate::{
        query::{common::SortDirection, trade::TradeSortBy, Query, TradeQuery},
        types::TradeSize,
    };

    #[test]
    fn test_trade_query() {
        let url = Url::parse("https://example.com").unwrap();

        insta::assert_yaml_snapshot!(TradeQuery::default().add_to_url(&url).to_string());

        insta::assert_yaml_snapshot!(TradeQuery::default()
            .with_issuer_id(123)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(TradeQuery::default()
            .with_issuer_id(123)
            .with_issuer_id(124)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(TradeQuery::default()
            .with_issuer_id(123)
            .with_issuer_id(124)
            .with_page(1)
            .with_page_size(12)
            .with_pub_date_relative(10)
            .with_tx_date_relative(30)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(TradeQuery::default()
            .with_issuer_ids(vec![1, 2, 3, 4].as_slice())
            .with_trade_sizes(vec![TradeSize::From250Kto500K, TradeSize::From1Mto5M].as_slice(),)
            .add_to_url(&url)
            .to_string());

        insta::assert_yaml_snapshot!(TradeQuery::default()
            .with_sort_direction(SortDirection::Asc)
            .with_sort_by(TradeSortBy::ReportingGap)
            .add_to_url(&url)
            .to_string());
    }
}
