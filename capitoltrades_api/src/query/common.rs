//! Shared query infrastructure: the [`Query`] trait, [`QueryCommon`] fields, and [`SortDirection`].

use std::str::FromStr;

use url::Url;

/// Trait implemented by all query builders. Provides URL serialization and
/// shared builder methods for pagination, date filtering, and sort direction.
pub trait Query {
    /// Appends this query's parameters to the given URL, returning the modified URL.
    fn add_to_url(&self, url: &Url) -> Url;

    /// Returns a mutable reference to the common query fields.
    fn get_common(&mut self) -> &mut QueryCommon;

    /// Sets the page number (1-indexed).
    fn with_page(mut self, page: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().page = page;
        self
    }

    /// Sets the number of results per page.
    fn with_page_size(mut self, page_size: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().page_size = Some(page_size);
        self
    }

    /// Filters by publication date, relative to today (e.g. 7 = last 7 days).
    fn with_pub_date_relative(mut self, pub_date_relative: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().pub_date_relative = Some(pub_date_relative);
        self
    }

    /// Filters by transaction date, relative to today (e.g. 30 = last 30 days).
    fn with_tx_date_relative(mut self, tx_date_relative: i64) -> Self
    where
        Self: Sized,
    {
        self.get_common().tx_date_relative = Some(tx_date_relative);
        self
    }

    /// Sets the sort direction (ascending or descending).
    fn with_sort_direction(mut self, sort_direction: SortDirection) -> Self
    where
        Self: Sized,
    {
        self.get_common().sort_direction = sort_direction;
        self
    }
}

/// Sort order for API results.
#[derive(Clone, Copy, Default)]
pub enum SortDirection {
    /// Ascending order (oldest/smallest first).
    Asc = 0,
    /// Descending order (newest/largest first). This is the default.
    #[default]
    Desc = 1,
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

/// Fields shared by all query types: pagination, date filters, and sort direction.
#[derive(Clone, Copy)]
pub struct QueryCommon {
    /// Page number (1-indexed). Defaults to 1.
    pub page: i64,
    /// Results per page. `None` uses the API default.
    pub page_size: Option<i64>,
    /// Filter by publication date, relative days from today (e.g. 7 = last 7 days).
    pub pub_date_relative: Option<i64>,
    /// Filter by transaction date, relative days from today (e.g. 30 = last 30 days).
    pub tx_date_relative: Option<i64>,
    /// Sort direction. Defaults to descending.
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
    /// Appends the common pagination and date parameters to the URL.
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
