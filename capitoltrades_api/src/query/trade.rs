use std::str::FromStr;

use url::Url;

use crate::types::{IssuerID, TradeSize};

use super::common::{Query, QueryCommon, SortDirection};

#[derive(Default)]
pub struct TradeQuery {
    pub common: QueryCommon,
    pub issuer_ids: Vec<IssuerID>,
    pub trade_sizes: Vec<TradeSize>,
    pub sort_by: TradeSortBy,
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
    pub fn with_sort_by(mut self, sort_by: TradeSortBy) -> Self {
        self.sort_by = sort_by;
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
