use std::str::FromStr;

use url::Url;

pub trait Query {
    fn add_to_url(&self, url: &Url) -> Url;

    fn get_common(&mut self) -> &mut QueryCommon;

    fn with_page(mut self, page: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().page = page;
        self
    }

    fn with_page_size(mut self, page_size: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().page_size = Some(page_size);
        self
    }

    fn with_pub_date_relative(mut self, pub_date_relative: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().pub_date_relative = Some(pub_date_relative);
        self
    }

    fn with_tx_date_relative(mut self, tx_date_relative: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().tx_date_relative = Some(tx_date_relative);
        self
    }

    fn with_sort_direction(mut self, sort_direction: SortDirection) -> Self
    where
        Self: Sized,
    {
        self.get_common().sort_direction = sort_direction;
        self
    }
}

#[derive(Clone, Copy)]
pub enum SortDirection {
    Asc = 0,
    Desc = 1,
}
impl Default for SortDirection {
    fn default() -> Self {
        SortDirection::Desc
    }
}
impl FromStr for SortDirection {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "0" => Ok(SortDirection::Asc),
            "1" => Ok(SortDirection::Desc),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct QueryCommon {
    pub page: i64,
    pub page_size: Option<i64>,
    /// Filter by the relative date of the publication date.
    /// Example: 7 for the last 7 days.
    pub pub_date_relative: Option<i64>,
    /// Filter by the relative date of the publication date.
    /// Example: 7 for the last 7 days.
    pub tx_date_relative: Option<i64>,
    pub sort_direction: SortDirection,
}

impl Default for QueryCommon {
    fn default() -> QueryCommon {
        QueryCommon {
            page: 1,
            page_size: None,
            pub_date_relative: None,
            tx_date_relative: None,
            sort_direction: SortDirection::Desc,
        }
    }
}

impl QueryCommon {
    pub fn add_to_url(&self, url: &Url) -> Url {
        let mut url = url.clone();
        url.query_pairs_mut()
            .append_pair("page", &self.page.to_string());
        if let Some(page_size) = self.page_size {
            url.query_pairs_mut()
                .append_pair("pageSize", &page_size.to_string());
        };
        if let Some(pub_date_relative) = self.pub_date_relative {
            url.query_pairs_mut()
                .append_pair("pubDate", format!("{}d", pub_date_relative).as_str());
        };
        if let Some(tx_date_relative) = self.tx_date_relative {
            url.query_pairs_mut()
                .append_pair("txDate", format!("{}d", tx_date_relative).as_str());
        };
        url
    }
}
