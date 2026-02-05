use capitoltrades_api::types::{IssuerDetail, PaginatedResponse, PoliticianDetail, Response, Trade};
use capitoltrades_api::{
    Client, IssuerQuery, PoliticianQuery, TradeQuery,
};

use crate::cache::MemoryCache;
use crate::error::CapitolTradesError;

pub struct CachedClient {
    inner: Client,
    cache: MemoryCache,
}

impl CachedClient {
    pub fn new(cache: MemoryCache) -> Self {
        Self {
            inner: Client::new(),
            cache,
        }
    }

    pub fn with_base_url(base_url: &str, cache: MemoryCache) -> Self {
        Self {
            inner: Client::with_base_url(base_url),
            cache,
        }
    }

    pub async fn get_trades(
        &self,
        query: &TradeQuery,
    ) -> Result<PaginatedResponse<Trade>, CapitolTradesError> {
        let cache_key = format!("trades:{:?}", query_to_cache_key(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<Trade> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self.inner.get_trades(query).await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    pub async fn get_politicians(
        &self,
        query: &PoliticianQuery,
    ) -> Result<PaginatedResponse<PoliticianDetail>, CapitolTradesError> {
        let cache_key = format!("politicians:{:?}", query_to_cache_key_politician(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<PoliticianDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self.inner.get_politicians(query).await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    pub async fn get_issuer(
        &self,
        issuer_id: i64,
    ) -> Result<Response<IssuerDetail>, CapitolTradesError> {
        let cache_key = format!("issuer:{}", issuer_id);

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: Response<IssuerDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self.inner.get_issuer(issuer_id).await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    pub async fn get_issuers(
        &self,
        query: &IssuerQuery,
    ) -> Result<PaginatedResponse<IssuerDetail>, CapitolTradesError> {
        let cache_key = format!("issuers:{:?}", query_to_cache_key_issuer(query));

        if let Some(cached) = self.cache.get(&cache_key) {
            let resp: PaginatedResponse<IssuerDetail> = serde_json::from_str(&cached)?;
            return Ok(resp);
        }

        let resp = self.inner.get_issuers(query).await?;
        if let Ok(json) = serde_json::to_string(&resp) {
            self.cache.set(cache_key, json);
        }
        Ok(resp)
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

fn parties_cache_key(parties: &[capitoltrades_api::types::Party]) -> String {
    parties
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn query_to_cache_key(query: &TradeQuery) -> String {
    format!(
        "p{}:s{:?}:i{:?}:ts{:?}:pa[{}]:st{:?}:co{:?}:q{:?}:\
         pdr{:?}:tdr{:?}:sb{}:sd{}:\
         ge{:?}:mc{:?}:at{:?}:la{:?}:se{:?}:tt{:?}:ch{:?}:\
         pi{:?}:is{:?}:cn{:?}",
        query.common.page,
        query.common.page_size,
        query.issuer_ids,
        query.trade_sizes.iter().map(|t| *t as u8).collect::<Vec<_>>(),
        parties_cache_key(&query.parties),
        query.states,
        query.committees,
        query.search,
        query.common.pub_date_relative,
        query.common.tx_date_relative,
        query.sort_by,
        query.common.sort_direction as u8,
        query.genders.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
        query.market_caps.iter().map(|m| *m as u8).collect::<Vec<_>>(),
        query.asset_types.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        query.labels.iter().map(|l| l.to_string()).collect::<Vec<_>>(),
        query.sectors.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        query.tx_types.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
        query.chambers.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
        query.politician_ids,
        query.issuer_states,
        query.countries,
    )
}

fn query_to_cache_key_politician(query: &PoliticianQuery) -> String {
    format!(
        "p{}:s{:?}:search{:?}:pa[{}]:st{:?}:co{:?}",
        query.common.page,
        query.common.page_size,
        query.search,
        parties_cache_key(&query.parties),
        query.states,
        query.committees,
    )
}

fn query_to_cache_key_issuer(query: &IssuerQuery) -> String {
    format!(
        "p{}:s{:?}:search{:?}:st{:?}",
        query.common.page,
        query.common.page_size,
        query.search,
        query.states,
    )
}
