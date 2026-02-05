use anyhow::{bail, Result};
use clap::Args;
use capitoltraders_lib::{
    CachedClient, IssuerQuery, PoliticianQuery, Query, SortDirection, TradeQuery, TradeSortBy,
};
use capitoltraders_lib::validation;

use crate::output::{
    print_json, print_trades_csv, print_trades_markdown, print_trades_table, OutputFormat,
};

#[derive(Args)]
pub struct TradesArgs {
    /// Filter by issuer ID (numeric)
    #[arg(long)]
    pub issuer_id: Option<i64>,

    /// Search trades by politician name
    #[arg(long)]
    pub name: Option<String>,

    /// Search trades by issuer name/ticker (two-step lookup)
    #[arg(long)]
    pub issuer: Option<String>,

    /// Filter trades by politician name (two-step lookup)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by party: democrat (d), republican (r), other
    #[arg(long)]
    pub party: Option<String>,

    /// Filter by US state code (e.g. CA, TX, NY)
    #[arg(long)]
    pub state: Option<String>,

    /// Filter by committee name (e.g. "Senate - Finance")
    #[arg(long)]
    pub committee: Option<String>,

    /// Filter trades from last N days (by publication date)
    #[arg(long, conflicts_with_all = ["since", "until"])]
    pub days: Option<i64>,

    /// Filter trades from last N days (by trade date)
    #[arg(long, conflicts_with_all = ["tx_since", "tx_until"])]
    pub tx_days: Option<i64>,

    /// Filter trades published on/after this date (YYYY-MM-DD)
    #[arg(long, conflicts_with = "days")]
    pub since: Option<String>,

    /// Filter trades published on/before this date (YYYY-MM-DD)
    #[arg(long, conflicts_with = "days")]
    pub until: Option<String>,

    /// Filter by transaction date on/after (YYYY-MM-DD)
    #[arg(long, conflicts_with = "tx_days")]
    pub tx_since: Option<String>,

    /// Filter by transaction date on/before (YYYY-MM-DD)
    #[arg(long, conflicts_with = "tx_days")]
    pub tx_until: Option<String>,

    /// Filter by trade size (1-10, comma-separated)
    #[arg(long)]
    pub trade_size: Option<String>,

    /// Filter by gender: female (f), male (m) -- comma-separated
    #[arg(long)]
    pub gender: Option<String>,

    /// Filter by market cap: mega,large,mid,small,micro,nano or 1-6 -- comma-separated
    #[arg(long)]
    pub market_cap: Option<String>,

    /// Filter by asset type: stock,etf,cryptocurrency,... -- comma-separated
    #[arg(long)]
    pub asset_type: Option<String>,

    /// Filter by label: faang,crypto,memestock,spac -- comma-separated
    #[arg(long)]
    pub label: Option<String>,

    /// Filter by sector: energy,financials,... -- comma-separated
    #[arg(long)]
    pub sector: Option<String>,

    /// Filter by transaction type: buy,sell,exchange,receive -- comma-separated
    #[arg(long)]
    pub tx_type: Option<String>,

    /// Filter by chamber: house (h), senate (s) -- comma-separated
    #[arg(long)]
    pub chamber: Option<String>,

    /// Filter by politician ID: P000197 format -- comma-separated
    #[arg(long)]
    pub politician_id: Option<String>,

    /// Filter by issuer state: 2-letter code (lowercase) -- comma-separated
    #[arg(long)]
    pub issuer_state: Option<String>,

    /// Filter by country: 2-letter ISO code (lowercase) -- comma-separated
    #[arg(long)]
    pub country: Option<String>,

    /// Page number
    #[arg(long, default_value = "1")]
    pub page: i64,

    /// Results per page
    #[arg(long, default_value = "20")]
    pub page_size: i64,

    /// Sort field: pub-date, trade-date, reporting-gap
    #[arg(long, default_value = "pub-date")]
    pub sort_by: String,

    /// Sort ascending instead of descending
    #[arg(long)]
    pub asc: bool,
}

pub async fn run(args: &TradesArgs, client: &CachedClient, format: &OutputFormat) -> Result<()> {
    let mut query = TradeQuery::default()
        .with_page(args.page)
        .with_page_size(args.page_size);

    if let Some(issuer_id) = args.issuer_id {
        query = query.with_issuer_id(issuer_id);
    }

    if let Some(ref name) = args.name {
        let sanitized = validation::validate_search(name)?;
        query = query.with_search(&sanitized);
    }

    if let Some(ref issuer) = args.issuer {
        let sanitized = validation::validate_search(issuer)?;
        let issuer_query = IssuerQuery::default().with_search(&sanitized);
        let issuer_resp = client.get_issuers(&issuer_query).await?;
        if issuer_resp.data.is_empty() {
            bail!("no issuers found matching '{}'", sanitized);
        }
        let ids: Vec<i64> = issuer_resp.data.iter().map(|i| i.issuer_id).collect();
        query = query.with_issuer_ids(&ids);
    }

    if let Some(ref politician) = args.politician {
        let sanitized = validation::validate_search(politician)?;
        let pol_query = PoliticianQuery::default().with_search(&sanitized);
        let pol_resp = client.get_politicians(&pol_query).await?;
        if pol_resp.data.is_empty() {
            bail!("no politicians found matching '{}'", sanitized);
        }
        let ids: Vec<String> = pol_resp.data.iter().map(|p| p.politician_id.clone()).collect();
        query = query.with_politician_ids(&ids);
    }

    if let Some(ref party) = args.party {
        let p = validation::validate_party(party)?;
        query = query.with_party(&p);
    }

    if let Some(ref state) = args.state {
        let validated = validation::validate_state(state)?;
        query = query.with_state(&validated);
    }

    if let Some(ref committee) = args.committee {
        let validated = validation::validate_committee(committee)?;
        query = query.with_committee(&validated);
    }

    // Parse absolute date filters
    let since_date = args.since.as_ref().map(|s| validation::validate_date(s)).transpose()?;
    let until_date = args.until.as_ref().map(|s| validation::validate_date(s)).transpose()?;
    let tx_since_date = args.tx_since.as_ref().map(|s| validation::validate_date(s)).transpose()?;
    let tx_until_date = args.tx_until.as_ref().map(|s| validation::validate_date(s)).transpose()?;

    // Validate since <= until when both provided
    if let (Some(s), Some(u)) = (since_date, until_date) {
        if s > u {
            bail!("--since ({}) must be on or before --until ({})", s, u);
        }
    }
    if let (Some(s), Some(u)) = (tx_since_date, tx_until_date) {
        if s > u {
            bail!("--tx-since ({}) must be on or before --tx-until ({})", s, u);
        }
    }

    if let Some(days) = args.days {
        let validated = validation::validate_days(days)?;
        query = query.with_pub_date_relative(validated);
    }

    if let Some(tx_days) = args.tx_days {
        let validated = validation::validate_days(tx_days)?;
        query = query.with_tx_date_relative(validated);
    }

    // Convert --since to relative days for the API (reduces response size)
    if let Some(since) = since_date {
        match validation::date_to_relative_days(since) {
            Some(days) => {
                // Add 1 day of padding to ensure we don't miss edge-case trades
                query = query.with_pub_date_relative(days + 1);
            }
            None => bail!("--since date {} is in the future", since),
        }
    }

    if let Some(tx_since) = tx_since_date {
        match validation::date_to_relative_days(tx_since) {
            Some(days) => {
                query = query.with_tx_date_relative(days + 1);
            }
            None => bail!("--tx-since date {} is in the future", tx_since),
        }
    }

    if let Some(ref val) = args.trade_size {
        for item in val.split(',') {
            let validated = validation::validate_trade_size(item.trim())?;
            query = query.with_trade_size(validated);
        }
    }

    if let Some(ref val) = args.gender {
        for item in val.split(',') {
            let validated = validation::validate_gender(item.trim())?;
            query = query.with_gender(validated);
        }
    }

    if let Some(ref val) = args.market_cap {
        for item in val.split(',') {
            let validated = validation::validate_market_cap(item.trim())?;
            query = query.with_market_cap(validated);
        }
    }

    if let Some(ref val) = args.asset_type {
        for item in val.split(',') {
            let validated = validation::validate_asset_type(item.trim())?;
            query = query.with_asset_type(validated);
        }
    }

    if let Some(ref val) = args.label {
        for item in val.split(',') {
            let validated = validation::validate_label(item.trim())?;
            query = query.with_label(validated);
        }
    }

    if let Some(ref val) = args.sector {
        for item in val.split(',') {
            let validated = validation::validate_sector(item.trim())?;
            query = query.with_sector(validated);
        }
    }

    if let Some(ref val) = args.tx_type {
        for item in val.split(',') {
            let validated = validation::validate_tx_type(item.trim())?;
            query = query.with_tx_type(validated);
        }
    }

    if let Some(ref val) = args.chamber {
        for item in val.split(',') {
            let validated = validation::validate_chamber(item.trim())?;
            query = query.with_chamber(validated);
        }
    }

    if let Some(ref val) = args.politician_id {
        for item in val.split(',') {
            let validated = validation::validate_politician_id(item.trim())?;
            query = query.with_politician_id(&validated);
        }
    }

    if let Some(ref val) = args.issuer_state {
        for item in val.split(',') {
            let validated = validation::validate_issuer_state(item.trim())?;
            query = query.with_issuer_state(&validated);
        }
    }

    if let Some(ref val) = args.country {
        for item in val.split(',') {
            let validated = validation::validate_country(item.trim())?;
            query = query.with_country(&validated);
        }
    }

    let sort_by = match args.sort_by.as_str() {
        "trade-date" => TradeSortBy::TradeDate,
        "reporting-gap" => TradeSortBy::ReportingGap,
        _ => TradeSortBy::PublicationDate,
    };
    query = query.with_sort_by(sort_by);

    if args.asc {
        query = query.with_sort_direction(SortDirection::Asc);
    }

    let resp = client.get_trades(&query).await?;

    let needs_filtering = since_date.is_some()
        || until_date.is_some()
        || tx_since_date.is_some()
        || tx_until_date.is_some();

    let trades = if needs_filtering {
        let mut filtered: Vec<_> = resp.data.into_iter().collect();
        filtered.retain(|t| {
            let pub_date = t.pub_date.date_naive();
            if let Some(s) = since_date {
                if pub_date < s {
                    return false;
                }
            }
            if let Some(u) = until_date {
                if pub_date > u {
                    return false;
                }
            }
            if let Some(s) = tx_since_date {
                if t.tx_date < s {
                    return false;
                }
            }
            if let Some(u) = tx_until_date {
                if t.tx_date > u {
                    return false;
                }
            }
            true
        });
        filtered
    } else {
        resp.data
    };

    if needs_filtering {
        eprintln!(
            "Page {}/{} ({} API results, {} after date filtering)",
            resp.meta.paging.page,
            resp.meta.paging.total_pages,
            resp.meta.paging.total_items,
            trades.len()
        );
    } else {
        eprintln!(
            "Page {}/{} ({} total trades)",
            resp.meta.paging.page, resp.meta.paging.total_pages, resp.meta.paging.total_items
        );
    }

    match format {
        OutputFormat::Table => print_trades_table(&trades),
        OutputFormat::Json => print_json(&trades),
        OutputFormat::Csv => print_trades_csv(&trades)?,
        OutputFormat::Markdown => print_trades_markdown(&trades),
    }

    Ok(())
}
