//! HTML scraping utilities for CapitolTrades pages (no API).

use std::time::Duration;

use rand::Rng;
use regex::Regex;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::time::sleep;

use capitoltrades_api::user_agent::get_user_agent;

#[derive(thiserror::Error, Debug)]
pub enum ScrapeError {
    #[error("http client error: {0}")]
    HttpClient(#[source] reqwest::Error),
    #[error("http error for {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("unexpected status {status} for {url}: {body}")]
    HttpStatus {
        status: StatusCode,
        url: String,
        body: String,
        retry_after: Option<Duration>,
    },
    #[error("missing RSC payload")]
    MissingPayload,
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Clone)]
pub struct ScrapeClient {
    base_url: String,
    http: reqwest::Client,
}

pub struct ScrapePage<T> {
    pub data: Vec<T>,
    pub total_pages: Option<i64>,
    pub total_count: Option<i64>,
}

struct RetryConfig {
    max_retries: usize,
    base_delay_ms: u64,
    max_delay_ms: u64,
}

impl RetryConfig {
    fn from_env() -> Self {
        Self {
            max_retries: env_usize("CAPITOLTRADES_RETRY_MAX", 3),
            base_delay_ms: env_u64("CAPITOLTRADES_RETRY_BASE_MS", 2000),
            max_delay_ms: env_u64("CAPITOLTRADES_RETRY_MAX_MS", 30000),
        }
    }

    fn delay_for_attempt(&self, attempt: usize) -> Duration {
        let shift = (attempt.saturating_sub(1)).min(30) as u32;
        let exp = 1u64 << shift;
        let base = self
            .base_delay_ms
            .saturating_mul(exp)
            .min(self.max_delay_ms);
        let jitter = rand::thread_rng().gen_range(0.8..1.2);
        Duration::from_millis((base as f64 * jitter) as u64)
    }
}

#[derive(Debug, Default)]
pub struct ScrapedTradeDetail {
    // Existing fields
    pub filing_url: Option<String>,
    pub filing_id: Option<i64>,
    // TRADE-01: asset type (e.g. "stock", "stock-option", "etf")
    pub asset_type: Option<String>,
    // TRADE-02: trade sizing
    pub size: Option<i64>,
    pub size_range_high: Option<i64>,
    pub size_range_low: Option<i64>,
    // TRADE-03: price per share at time of trade
    pub price: Option<f64>,
    // Additional enrichment
    pub has_capital_gains: Option<bool>,
    // TRADE-05: committees (may be empty if not in RSC payload)
    pub committees: Vec<String>,
    // TRADE-06: labels (may be empty if not in RSC payload)
    pub labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedTrade {
    #[serde(rename = "_txId")]
    pub tx_id: i64,
    #[serde(rename = "_politicianId")]
    pub politician_id: String,
    #[serde(rename = "_issuerId")]
    pub issuer_id: i64,
    pub chamber: String,
    pub comment: Option<String>,
    pub issuer: ScrapedIssuer,
    pub owner: String,
    pub politician: ScrapedPolitician,
    pub price: Option<f64>,
    pub pub_date: String,
    pub reporting_gap: i64,
    pub tx_date: String,
    pub tx_type: String,
    pub tx_type_extended: Option<serde_json::Value>,
    pub value: i64,
    #[serde(default)]
    pub filing_url: Option<String>,
    #[serde(default)]
    pub filing_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedIssuer {
    #[serde(rename = "_stateId")]
    pub state_id: Option<String>,
    #[serde(rename = "c2iq")]
    pub c2iq: Option<String>,
    pub country: Option<String>,
    pub issuer_name: String,
    pub issuer_ticker: Option<String>,
    pub sector: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedPolitician {
    #[serde(rename = "_stateId")]
    pub state_id: String,
    pub chamber: String,
    pub dob: String,
    pub first_name: String,
    pub gender: String,
    pub last_name: String,
    pub nickname: Option<String>,
    pub party: String,
}

#[derive(Debug)]
pub struct ScrapedPoliticianCard {
    pub politician_id: String,
    pub name: String,
    pub party: String,
    pub state: String,
    pub trades: i64,
    pub issuers: i64,
    pub volume: i64,
    pub last_traded: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedIssuerList {
    #[serde(rename = "_issuerId")]
    pub issuer_id: i64,
    pub issuer_name: String,
    pub issuer_ticker: Option<String>,
    pub sector: Option<String>,
    pub stats: ScrapedIssuerStats,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedIssuerDetail {
    #[serde(rename = "_issuerId")]
    pub issuer_id: i64,
    #[serde(rename = "_stateId")]
    pub state_id: Option<String>,
    #[serde(rename = "c2iq")]
    pub c2iq: Option<String>,
    pub country: Option<String>,
    pub issuer_name: String,
    pub issuer_ticker: Option<String>,
    pub performance: Option<serde_json::Value>,
    pub sector: Option<String>,
    pub stats: ScrapedIssuerStats,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrapedIssuerStats {
    #[serde(rename = "countTrades")]
    pub count_trades: i64,
    #[serde(rename = "countPoliticians")]
    pub count_politicians: i64,
    pub volume: i64,
    #[serde(rename = "dateLastTraded")]
    pub date_last_traded: String,
}

impl ScrapeClient {
    pub fn new() -> Result<Self, ScrapeError> {
        let http = reqwest::Client::builder()
            .user_agent(get_user_agent())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ScrapeError::HttpClient)?;
        Ok(Self {
            base_url: "https://www.capitoltrades.com".to_string(),
            http,
        })
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, ScrapeError> {
        let http = reqwest::Client::builder()
            .user_agent(get_user_agent())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(ScrapeError::HttpClient)?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    pub async fn trades_page(&self, page: i64) -> Result<ScrapePage<ScrapedTrade>, ScrapeError> {
        let url = format!("{}/trades?page={}", self.base_url, page);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        let data = extract_array_with_key(&payload, "_txId")
            .ok_or_else(|| ScrapeError::Parse("missing trades data array".into()))?;
        let trades: Vec<ScrapedTrade> = serde_json::from_value(data)?;

        let total_pages = extract_number(&payload, "\"totalPages\":");
        let total_count = extract_number(&payload, "\"totalCount\":");

        Ok(ScrapePage {
            data: trades,
            total_pages,
            total_count,
        })
    }

    pub async fn issuers_page(
        &self,
        page: i64,
    ) -> Result<ScrapePage<ScrapedIssuerList>, ScrapeError> {
        let url = format!("{}/issuers?page={}", self.base_url, page);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        let data = extract_array_with_key(&payload, "_issuerId")
            .ok_or_else(|| ScrapeError::Parse("missing issuers data array".into()))?;
        let issuers: Vec<ScrapedIssuerList> = serde_json::from_value(data)?;

        let total_pages = extract_number(&payload, "\"totalPages\":");
        let total_count = extract_number(&payload, "\"totalCount\":");

        Ok(ScrapePage {
            data: issuers,
            total_pages,
            total_count,
        })
    }

    pub async fn issuer_detail(&self, issuer_id: i64) -> Result<ScrapedIssuerDetail, ScrapeError> {
        let url = format!("{}/issuers/{}", self.base_url, issuer_id);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        let obj = extract_json_object_after(&payload, "\"issuerData\":")
            .ok_or_else(|| ScrapeError::Parse("missing issuerData payload".into()))?;
        let detail: ScrapedIssuerDetail = serde_json::from_value(obj)?;
        Ok(detail)
    }

    pub async fn politicians_page(
        &self,
        page: i64,
    ) -> Result<ScrapePage<ScrapedPoliticianCard>, ScrapeError> {
        let url = format!("{}/politicians?page={}", self.base_url, page);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        let total_count = extract_number(&payload, "\"totalCount\":");

        let cards = parse_politician_cards(&payload)?;
        let total_pages = total_count.and_then(|count| {
            let page_size = cards.len() as i64;
            if page_size > 0 {
                Some((count + page_size - 1) / page_size)
            } else {
                None
            }
        });

        Ok(ScrapePage {
            data: cards,
            total_pages,
            total_count,
        })
    }

    /// Fetch politicians filtered by committee code from the listing page.
    ///
    /// URL format: /politicians?committee={committee_code}&page={page}
    /// Reuses parse_politician_cards for card extraction, identical to
    /// politicians_page except the URL includes the committee query parameter.
    pub async fn politicians_by_committee(
        &self,
        committee_code: &str,
        page: i64,
    ) -> Result<ScrapePage<ScrapedPoliticianCard>, ScrapeError> {
        let url = format!(
            "{}/politicians?committee={}&page={}",
            self.base_url, committee_code, page
        );
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        let total_count = extract_number(&payload, "\"totalCount\":");

        let cards = parse_politician_cards(&payload)?;
        let total_pages = total_count.and_then(|count| {
            let page_size = cards.len() as i64;
            if page_size > 0 {
                Some((count + page_size - 1) / page_size)
            } else {
                None
            }
        });

        Ok(ScrapePage {
            data: cards,
            total_pages,
            total_count,
        })
    }

    pub async fn politician_detail(
        &self,
        politician_id: &str,
    ) -> Result<Option<ScrapedPolitician>, ScrapeError> {
        let url = format!("{}/politicians/{}", self.base_url, politician_id);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        Ok(extract_politician_detail(&payload))
    }

    pub async fn trade_detail(&self, trade_id: i64) -> Result<ScrapedTradeDetail, ScrapeError> {
        let url = format!("{}/trades/{}", self.base_url, trade_id);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        Ok(extract_trade_detail(&payload, trade_id))
    }

    async fn fetch_html(&self, url: &str) -> Result<String, ScrapeError> {
        self.with_retry(url, || async { self.fetch_html_once(url).await })
            .await
    }

    async fn fetch_html_once(&self, url: &str) -> Result<String, ScrapeError> {
        let resp = self
            .http
            .get(url)
            .header("accept", "text/html,application/xhtml+xml")
            .header("accept-language", "en-US,en;q=0.9")
            .header("upgrade-insecure-requests", "1")
            .header("cache-control", "no-cache")
            .header("pragma", "no-cache")
            .send()
            .await
            .map_err(|err| ScrapeError::Http {
                url: url.to_string(),
                source: err,
            })?;

        let status = resp.status();
        let retry_after = parse_retry_after(resp.headers());
        let body = resp.text().await.map_err(|err| ScrapeError::Http {
            url: url.to_string(),
            source: err,
        })?;

        if !status.is_success() {
            return Err(ScrapeError::HttpStatus {
                status,
                url: url.to_string(),
                body: truncate_body(&body),
                retry_after,
            });
        }

        Ok(body)
    }

    async fn with_retry<T, F, Fut>(&self, url: &str, mut f: F) -> Result<T, ScrapeError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, ScrapeError>>,
    {
        let cfg = RetryConfig::from_env();
        let mut attempt = 0usize;
        loop {
            match f().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    attempt += 1;
                    if attempt > cfg.max_retries || !is_retryable(&err) {
                        return Err(err);
                    }
                    let delay = match retry_after_hint(&err) {
                        Some(hint) => hint.min(Duration::from_millis(cfg.max_delay_ms)),
                        None => cfg.delay_for_attempt(attempt),
                    };
                    tracing::warn!(
                        "scrape request failed (attempt {}/{}), retrying in {:.1}s: {}",
                        attempt,
                        cfg.max_retries,
                        delay.as_secs_f64(),
                        url
                    );
                    sleep(delay).await;
                }
            }
        }
    }
}

fn is_retryable(err: &ScrapeError) -> bool {
    match err {
        ScrapeError::HttpStatus { status, .. } => {
            *status == StatusCode::TOO_MANY_REQUESTS
                || *status == StatusCode::REQUEST_TIMEOUT
                || status.is_server_error()
        }
        ScrapeError::Http { source, .. } => source.is_timeout() || source.is_connect(),
        _ => false,
    }
}

fn retry_after_hint(err: &ScrapeError) -> Option<Duration> {
    match err {
        ScrapeError::HttpStatus { retry_after, .. } => *retry_after,
        _ => None,
    }
}

fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let raw = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = raw.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    None
}

fn truncate_body(body: &str) -> String {
    const MAX: usize = 2000;
    if body.len() <= MAX {
        body.to_string()
    } else {
        format!("{}...[truncated]", &body[..MAX])
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(default)
}

fn extract_rsc_payload(html: &str) -> Result<String, ScrapeError> {
    let needle = "self.__next_f.push([1,\"";
    let mut out = String::new();
    let mut search = html;

    while let Some(start) = search.find(needle) {
        let after = &search[start + needle.len()..];
        let mut escaped = false;
        let mut end_idx = None;
        for (i, ch) in after.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                end_idx = Some(i);
                break;
            }
        }
        let Some(end) = end_idx else {
            break;
        };
        let raw = &after[..end];
        let decoded: String = serde_json::from_str(&format!("\"{}\"", raw))?;
        out.push_str(&decoded);
        search = &after[end + 1..];
    }

    if out.is_empty() {
        return Err(ScrapeError::MissingPayload);
    }

    Ok(out)
}

fn extract_trade_detail(payload: &str, trade_id: i64) -> ScrapedTradeDetail {
    let mut detail = ScrapedTradeDetail::default();
    let trade_needle = format!("\"tradeId\":{}", trade_id);
    let mut cursor = 0;
    while let Some(pos) = payload[cursor..].find(&trade_needle) {
        let idx = cursor + pos;

        // Strategy: walk backwards from the match to find the enclosing JSON object,
        // then use extract_json_object to get the complete object and parse all fields.
        if let Some(obj_start) = payload[..idx].rfind('{') {
            if let Some(obj_str) = extract_json_object(payload, obj_start) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&obj_str) {
                    // Verify this object actually contains our tradeId (not a parent object)
                    if parsed.get("tradeId").and_then(|v| v.as_i64()) == Some(trade_id) {
                        return extract_fields_from_trade_object(&parsed);
                    }
                }
            }
        }

        // Fallback: try the old window-based approach for filing_url only
        let window_start = idx.saturating_sub(500);
        let window_end = (idx + 500).min(payload.len());
        let window = &payload[window_start..window_end];
        if let Some(url) = extract_json_string(window, "\"filingUrl\":\"") {
            detail.filing_id = filing_id_from_url(&url);
            detail.filing_url = Some(url);
            return detail;
        }
        cursor = idx + trade_needle.len();
    }
    detail
}

/// Extract all enrichable fields from a parsed trade JSON object.
fn extract_fields_from_trade_object(parsed: &serde_json::Value) -> ScrapedTradeDetail {
    // Helper to extract string arrays (for committees/labels)
    let extract_string_vec = |key: &str| -> Vec<String> {
        parsed
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    // Filing URL: try "filingUrl" (RSC style) then "filingURL" (BFF API style)
    let filing_url = parsed
        .get("filingUrl")
        .or_else(|| parsed.get("filingURL"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let filing_id = filing_url
        .as_ref()
        .and_then(|url| filing_id_from_url(url));

    // TRADE-01: asset type from nested "asset" object, fallback to direct key
    let asset_type = parsed
        .get("asset")
        .and_then(|a| a.get("assetType"))
        .or_else(|| parsed.get("assetType"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ScrapedTradeDetail {
        filing_url,
        filing_id,
        asset_type,
        // TRADE-02: trade sizing
        size: parsed.get("size").and_then(|v| v.as_i64()),
        size_range_high: parsed.get("sizeRangeHigh").and_then(|v| v.as_i64()),
        size_range_low: parsed.get("sizeRangeLow").and_then(|v| v.as_i64()),
        // TRADE-03: price
        price: parsed.get("price").and_then(|v| v.as_f64()),
        // has_capital_gains
        has_capital_gains: parsed.get("hasCapitalGains").and_then(|v| v.as_bool()),
        // TRADE-05: committees
        committees: extract_string_vec("committees"),
        // TRADE-06: labels
        labels: extract_string_vec("labels"),
    }
}

fn extract_array_with_key(payload: &str, key: &str) -> Option<serde_json::Value> {
    let mut cursor = 0;
    let needle = "\"data\"";
    while let Some(pos) = payload[cursor..].find(needle) {
        let start = cursor + pos;
        let array_start = payload[start..].find('[').map(|i| start + i)?;
        if let Some(array_text) = extract_json_array(payload, array_start) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&array_text) {
                if let serde_json::Value::Array(items) = &value {
                    if let Some(first) = items.first() {
                        if first.get(key).is_some() {
                            return Some(value);
                        }
                    }
                }
            }
        }
        cursor = start + needle.len();
    }
    None
}

fn extract_politician_detail(payload: &str) -> Option<ScrapedPolitician> {
    let needle = "\"politician\":{";
    let idx = payload.find(needle)?;
    let start = idx + needle.len() - 1;
    let obj = extract_json_object(payload, start)?;
    serde_json::from_str(&obj).ok()
}

fn extract_json_array(payload: &str, start: usize) -> Option<String> {
    let mut depth = 0;
    let mut in_str = false;
    let mut escape = false;
    for (offset, ch) in payload[start..].char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + offset + 1;
                    return Some(payload[start..end].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_json_object(payload: &str, start: usize) -> Option<String> {
    let mut depth = 0;
    let mut in_str = false;
    let mut escape = false;
    for (offset, ch) in payload[start..].char_indices() {
        if in_str {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + offset + 1;
                    return Some(payload[start..end].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_json_object_after(payload: &str, key: &str) -> Option<serde_json::Value> {
    let idx = payload.find(key)?;
    let after = idx + key.len();
    let start = payload[after..].find('{').map(|i| after + i)?;
    let obj = extract_json_object(payload, start)?;
    serde_json::from_str(&obj).ok()
}

fn extract_json_string(haystack: &str, key: &str) -> Option<String> {
    let idx = haystack.find(key)?;
    let start = idx + key.len();
    let mut escaped = false;
    for (offset, ch) in haystack[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            let raw = &haystack[start..start + offset];
            let decoded: String = serde_json::from_str(&format!("\"{}\"", raw)).ok()?;
            return Some(decoded);
        }
    }
    None
}

fn filing_id_from_url(url: &str) -> Option<i64> {
    let trimmed = url.split('?').next().unwrap_or(url);
    let last = trimmed.rsplit('/').next()?;
    let last = last.trim_end_matches(".pdf");
    if last.is_empty() {
        return None;
    }
    if last.chars().all(|c| c.is_ascii_digit()) {
        last.parse().ok()
    } else {
        None
    }
}

fn extract_number(payload: &str, key: &str) -> Option<i64> {
    let idx = payload.find(key)?;
    let mut i = idx + key.len();
    let bytes = payload.as_bytes();
    while i < bytes.len() && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    payload[start..i].parse().ok()
}

fn parse_politician_cards(payload: &str) -> Result<Vec<ScrapedPoliticianCard>, ScrapeError> {
    let id_re = Regex::new(r#"href":"/politicians/([A-Z]\d{6})""#)
        .map_err(|e| ScrapeError::Parse(format!("regex compile error: {}", e)))?;
    // The live site uses singular labels ("Trade", "Issuer") when count == 1
    // and plural labels ("Trades", "Issuers") when count > 1. Accept both.
    let card_re = Regex::new(
        r#"(?s)href":"/politicians/(?P<id>[A-Z]\d{6})".*?cell--name.*?children":"(?P<name>[^"]+)".*?party--(?P<party>democrat|republican|other).*?us-state-full--(?P<state>[a-z]{2}).*?cell--count-trades.*?children":"Trades?".*?children":"(?P<trades>[\d,]+)".*?cell--count-issuers.*?children":"Issuers?".*?children":"(?P<issuers>[\d,]+)".*?cell--volume.*?children":"Volume".*?children":"(?P<volume>[^"]+)".*?cell--last-traded.*?children":"Last Traded".*?children":"(?P<last>\d{4}-\d{2}-\d{2})""#,
    )
    .map_err(|e| ScrapeError::Parse(format!("regex compile error: {}", e)))?;

    let ids: Vec<String> = id_re
        .captures_iter(payload)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    let mut cards = Vec::new();
    for cap in card_re.captures_iter(payload) {
        let politician_id = cap["id"].to_string();
        let name = cap["name"].to_string();
        let party = cap["party"].to_string();
        let state = cap["state"].to_ascii_uppercase();
        let trades = parse_int(&cap["trades"]).ok_or_else(|| {
            ScrapeError::Parse(format!(
                "invalid trade count for politician {}",
                politician_id
            ))
        })?;
        let issuers = parse_int(&cap["issuers"]).ok_or_else(|| {
            ScrapeError::Parse(format!(
                "invalid issuer count for politician {}",
                politician_id
            ))
        })?;
        let volume = parse_compact_number(&cap["volume"]).ok_or_else(|| {
            ScrapeError::Parse(format!("invalid volume for politician {}", politician_id))
        })?;
        let last_traded = Some(cap["last"].to_string());

        cards.push(ScrapedPoliticianCard {
            politician_id,
            name,
            party,
            state,
            trades,
            issuers,
            volume,
            last_traded,
        });
    }

    if ids.is_empty() && cards.is_empty() {
        // Legitimate empty result (e.g. defunct committee with no members).
        return Ok(cards);
    }

    if cards.is_empty() {
        return Err(ScrapeError::Parse(
            "no politician cards found in payload".into(),
        ));
    }

    if ids.len() != cards.len() {
        return Err(ScrapeError::Parse(format!(
            "politician card count mismatch: expected {}, parsed {}",
            ids.len(),
            cards.len()
        )));
    }

    Ok(cards)
}

fn parse_int(raw: &str) -> Option<i64> {
    let cleaned = raw.trim().replace(',', "");
    if cleaned.is_empty() {
        return None;
    }
    cleaned.parse().ok()
}

fn parse_compact_number(raw: &str) -> Option<i64> {
    let mut cleaned = raw.trim().replace(',', "");
    if cleaned.is_empty() || cleaned == "-" || cleaned == "—" {
        return None;
    }
    if cleaned.starts_with('$') {
        cleaned = cleaned.trim_start_matches('$').to_string();
    }
    let (num_str, mult) = match cleaned.chars().last()? {
        'K' | 'k' => (&cleaned[..cleaned.len() - 1], 1_000.0),
        'M' | 'm' => (&cleaned[..cleaned.len() - 1], 1_000_000.0),
        'B' | 'b' => (&cleaned[..cleaned.len() - 1], 1_000_000_000.0),
        _ => (cleaned.as_str(), 1.0),
    };
    let num: f64 = num_str.parse().ok()?;
    Some((num * mult).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Fixture loading helpers ----

    fn load_stock_fixture() -> (String, i64) {
        let html = include_str!("../tests/fixtures/trade_detail_stock.html");
        let payload = extract_rsc_payload(html).expect("stock fixture should have RSC payload");
        (payload, 172000)
    }

    fn load_option_fixture() -> (String, i64) {
        let html = include_str!("../tests/fixtures/trade_detail_option.html");
        let payload = extract_rsc_payload(html).expect("option fixture should have RSC payload");
        (payload, 171500)
    }

    fn load_minimal_fixture() -> (String, i64) {
        let html = include_str!("../tests/fixtures/trade_detail_minimal.html");
        let payload = extract_rsc_payload(html).expect("minimal fixture should have RSC payload");
        (payload, 3000)
    }

    // ---- TRADE-01: Asset type extraction ----

    #[test]
    fn test_extract_trade_detail_asset_type_stock() {
        // TRADE-01: Verify asset_type is extracted from the nested asset object
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.asset_type.as_deref(),
            Some("stock"),
            "stock fixture should yield asset_type = 'stock'"
        );
    }

    #[test]
    fn test_extract_trade_detail_asset_type_option() {
        // TRADE-01: Verify non-stock asset types are extracted correctly
        let (payload, trade_id) = load_option_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.asset_type.as_deref(),
            Some("stock-option"),
            "option fixture should yield asset_type = 'stock-option'"
        );
    }

    #[test]
    fn test_extract_trade_detail_asset_type_minimal() {
        // TRADE-01: Verify asset_type extraction works even for uncommon types
        let (payload, trade_id) = load_minimal_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.asset_type.as_deref(),
            Some("mutual-fund"),
            "minimal fixture should yield asset_type = 'mutual-fund'"
        );
    }

    // ---- TRADE-02: Trade sizing ----

    #[test]
    fn test_extract_trade_detail_size_fields() {
        // TRADE-02: Verify size, sizeRangeHigh, sizeRangeLow extraction
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(detail.size, Some(4), "stock fixture should have size = 4");
        assert_eq!(
            detail.size_range_high,
            Some(100000),
            "stock fixture should have sizeRangeHigh = 100000"
        );
        assert_eq!(
            detail.size_range_low,
            Some(50001),
            "stock fixture should have sizeRangeLow = 50001"
        );
    }

    #[test]
    fn test_extract_trade_detail_size_fields_null() {
        // TRADE-02: Verify null size fields are handled gracefully
        let (payload, trade_id) = load_minimal_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.size, None,
            "minimal fixture should have null size"
        );
        assert_eq!(
            detail.size_range_high, None,
            "minimal fixture should have null sizeRangeHigh"
        );
        assert_eq!(
            detail.size_range_low, None,
            "minimal fixture should have null sizeRangeLow"
        );
    }

    // ---- TRADE-03: Price extraction ----

    #[test]
    fn test_extract_trade_detail_price() {
        // TRADE-03: Verify price extraction as f64
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert!(detail.price.is_some(), "stock fixture should have a price");
        let price = detail.price.unwrap();
        assert!(
            (price - 185.5).abs() < 0.01,
            "stock fixture price should be ~185.50, got {}",
            price
        );
    }

    #[test]
    fn test_extract_trade_detail_price_null() {
        // TRADE-03: Verify null price is handled gracefully
        let (payload, trade_id) = load_minimal_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.price, None,
            "minimal fixture should have null price"
        );
    }

    // ---- Filing URL and filing ID regression ----

    #[test]
    fn test_extract_trade_detail_filing_url_regression() {
        // Regression test: filing_url and filing_id must still be extracted
        // (this functionality existed before the rewrite)
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert!(
            detail.filing_url.is_some(),
            "stock fixture should have a filing_url"
        );
        let url = detail.filing_url.as_deref().unwrap();
        assert!(
            url.contains("efts.sec.gov"),
            "filing URL should point to SEC EFTS, got: {}",
            url
        );
    }

    #[test]
    fn test_extract_trade_detail_filing_id_from_path() {
        // Regression: filing_id derived from URL path segment
        let (payload, trade_id) = load_option_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.filing_id,
            Some(88002),
            "option fixture filing_id should be 88002 (extracted from path)"
        );
    }

    #[test]
    fn test_extract_trade_detail_filing_empty() {
        // Regression: empty filing_url yields None for both url and id
        let (payload, trade_id) = load_minimal_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.filing_url, None,
            "minimal fixture with empty filingUrl should yield None"
        );
        assert_eq!(
            detail.filing_id, None,
            "minimal fixture with empty filingUrl should yield None filing_id"
        );
    }

    // ---- has_capital_gains ----

    #[test]
    fn test_extract_trade_detail_has_capital_gains_false() {
        // Verify hasCapitalGains extraction when false
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.has_capital_gains,
            Some(false),
            "stock fixture should have hasCapitalGains = false"
        );
    }

    #[test]
    fn test_extract_trade_detail_has_capital_gains_true() {
        // Verify hasCapitalGains extraction when true
        let (payload, trade_id) = load_option_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert_eq!(
            detail.has_capital_gains,
            Some(true),
            "option fixture should have hasCapitalGains = true"
        );
    }

    // ---- TRADE-05: Committees availability ----

    #[test]
    fn test_extract_trade_detail_committees_availability() {
        // TRADE-05 finding: Committees ARE present in the synthetic trade detail fixture
        // as a Vec<String> on the trade object under the "committees" key.
        //
        // IMPORTANT: These are SYNTHETIC fixtures. The live capitoltrades.com trade detail
        // pages could NOT be fetched with full RSC data (they return loading states via curl).
        // The BFF API Trade struct includes a committees field, and the synthetic fixtures
        // model that structure. Whether the actual live RSC payload includes committees
        // remains UNCONFIRMED and should be verified when live site access is available.
        //
        // If committees are NOT present in the live RSC payload, they should be obtained
        // through politician enrichment (Phase 4) instead.
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        eprintln!(
            "TRADE-05: committees from stock fixture: {:?} (count: {})",
            detail.committees,
            detail.committees.len()
        );
        assert_eq!(
            detail.committees.len(),
            2,
            "stock fixture has 2 committees: Finance, Banking"
        );
        assert_eq!(detail.committees[0], "Finance");
        assert_eq!(detail.committees[1], "Banking");

        // Option fixture has empty committees
        let (payload, trade_id) = load_option_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert!(
            detail.committees.is_empty(),
            "option fixture should have empty committees"
        );
    }

    // ---- TRADE-06: Labels availability ----

    #[test]
    fn test_extract_trade_detail_labels_availability() {
        // TRADE-06 finding: Labels ARE present in the synthetic trade detail fixture
        // as a Vec<String> on the trade object under the "labels" key.
        //
        // IMPORTANT: Same caveat as TRADE-05. These are SYNTHETIC fixtures. The live
        // RSC payload structure is UNCONFIRMED. The BFF API Trade struct includes a
        // labels field with values like "faang", "crypto", "memestock", "spac".
        //
        // If labels are NOT present in the live RSC payload, they may need to be
        // sourced from the issuer entity or a separate enrichment pass.
        let (payload, trade_id) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        eprintln!(
            "TRADE-06: labels from stock fixture: {:?} (count: {})",
            detail.labels,
            detail.labels.len()
        );
        assert_eq!(
            detail.labels.len(),
            1,
            "stock fixture has 1 label: faang"
        );
        assert_eq!(detail.labels[0], "faang");

        // Minimal fixture has empty labels
        let (payload, trade_id) = load_minimal_fixture();
        let detail = extract_trade_detail(&payload, trade_id);
        assert!(
            detail.labels.is_empty(),
            "minimal fixture should have empty labels"
        );
    }

    // ---- Graceful degradation ----

    #[test]
    fn test_extract_trade_detail_nonexistent_id() {
        // Verify graceful degradation: a trade_id that does not appear in the
        // fixture payload returns a default ScrapedTradeDetail with no panic.
        let (payload, _) = load_stock_fixture();
        let detail = extract_trade_detail(&payload, 999999);
        assert_eq!(detail.filing_url, None);
        assert_eq!(detail.filing_id, None);
        assert_eq!(detail.asset_type, None);
        assert_eq!(detail.size, None);
        assert_eq!(detail.size_range_high, None);
        assert_eq!(detail.size_range_low, None);
        assert_eq!(detail.price, None);
        assert_eq!(detail.has_capital_gains, None);
        assert!(detail.committees.is_empty());
        assert!(detail.labels.is_empty());
    }

    #[test]
    fn test_extract_trade_detail_empty_payload() {
        // Verify graceful degradation with completely empty payload
        let detail = extract_trade_detail("", 172000);
        assert_eq!(detail.filing_url, None);
        assert_eq!(detail.asset_type, None);
    }

    // ---- Temporary verification test for committee-filtered page ----

    // ---- Committee-filtered politician listing fixture tests ----

    // ---- Issuer detail fixture tests ----

    fn load_issuer_perf_fixture() -> String {
        let html = include_str!("../tests/fixtures/issuer_detail_with_performance.html");
        extract_rsc_payload(html).expect("issuer perf fixture should have RSC payload")
    }

    fn load_issuer_no_perf_fixture() -> String {
        let html = include_str!("../tests/fixtures/issuer_detail_no_performance.html");
        extract_rsc_payload(html).expect("issuer no-perf fixture should have RSC payload")
    }

    #[test]
    fn test_issuer_detail_with_performance() {
        let payload = load_issuer_perf_fixture();
        let obj = extract_json_object_after(&payload, "\"issuerData\":")
            .expect("issuerData should be extractable from perf fixture");
        let detail: ScrapedIssuerDetail =
            serde_json::from_value(obj).expect("should deserialize to ScrapedIssuerDetail");

        assert_eq!(detail.issuer_name, "Apple Inc.");
        assert_eq!(detail.issuer_ticker.as_deref(), Some("AAPL"));
        assert_eq!(detail.sector.as_deref(), Some("information-technology"));
        assert!(
            detail.performance.is_some(),
            "performance should be Some for the perf fixture"
        );
        assert_eq!(detail.stats.count_trades, 450);
        assert_eq!(detail.stats.count_politicians, 85);
        assert_eq!(detail.stats.volume, 25000000);
        assert_eq!(detail.issuer_id, 12345);
        assert_eq!(detail.state_id.as_deref(), Some("ca"));
        assert_eq!(detail.c2iq.as_deref(), Some("AAPL:US"));
        assert_eq!(detail.country.as_deref(), Some("us"));
    }

    #[test]
    fn test_issuer_detail_no_performance() {
        let payload = load_issuer_no_perf_fixture();
        let obj = extract_json_object_after(&payload, "\"issuerData\":")
            .expect("issuerData should be extractable from no-perf fixture");
        let detail: ScrapedIssuerDetail =
            serde_json::from_value(obj).expect("should deserialize to ScrapedIssuerDetail");

        assert!(
            detail.performance.is_none(),
            "performance should be None for the no-perf fixture"
        );
        assert_eq!(detail.issuer_name, "PrivateCo Holdings");
        assert_eq!(detail.issuer_ticker, None);
        assert_eq!(detail.sector, None);
        assert_eq!(detail.issuer_id, 99999);
        assert_eq!(detail.stats.count_trades, 5);
    }

    #[test]
    fn test_issuer_detail_performance_eod_prices() {
        let payload = load_issuer_perf_fixture();
        let obj = extract_json_object_after(&payload, "\"issuerData\":")
            .expect("issuerData should be extractable");
        let detail: ScrapedIssuerDetail =
            serde_json::from_value(obj).expect("should deserialize");

        let perf = detail.performance.expect("performance should be Some");
        let eod = perf
            .get("eodPrices")
            .expect("eodPrices key should exist")
            .as_array()
            .expect("eodPrices should be an array");
        assert_eq!(eod.len(), 3, "eodPrices should have 3 entries");

        // Verify first entry: ["2026-01-15", 225.5]
        let first = eod[0].as_array().expect("first entry should be an array");
        assert_eq!(first[0].as_str(), Some("2026-01-15"));
        assert!((first[1].as_f64().unwrap() - 225.5).abs() < f64::EPSILON);

        // Verify mcap
        let mcap = perf.get("mcap").and_then(|v| v.as_i64());
        assert_eq!(mcap, Some(3500000000000_i64), "mcap should be 3.5T");
    }

    // ---- Committee-filtered politician listing fixture tests ----

    #[test]
    fn test_politicians_by_committee_fixture() {
        // Fixture is real HTML from /politicians?committee=ssfi (Senate Finance).
        // Verified that parse_politician_cards works after fixing singular/plural
        // label bug ("Trade"/"Trades", "Issuer"/"Issuers").
        let html = include_str!("../tests/fixtures/politicians_committee_filtered.html");
        let payload = extract_rsc_payload(html).expect("fixture should have RSC payload");

        let total_count = extract_number(&payload, "\"totalCount\":");
        assert_eq!(total_count, Some(5), "ssfi fixture should report totalCount=5");

        let cards = parse_politician_cards(&payload).expect("should parse all politician cards");
        assert_eq!(cards.len(), 5, "ssfi fixture should contain 5 politician cards");

        // Verify all cards have valid politician_ids
        for card in &cards {
            assert!(
                card.politician_id.len() == 7,
                "politician_id should be 7 chars: {}",
                card.politician_id
            );
            assert!(
                card.politician_id.starts_with(|c: char| c.is_ascii_uppercase()),
                "politician_id should start with uppercase letter: {}",
                card.politician_id
            );
            assert!(!card.name.is_empty(), "name should not be empty");
            assert!(
                ["democrat", "republican", "other"].contains(&card.party.as_str()),
                "unexpected party: {}",
                card.party
            );
            assert_eq!(card.state.len(), 2, "state should be 2 chars: {}", card.state);
            assert!(card.trades > 0, "trades should be > 0 for {}", card.politician_id);
        }

        // Verify known politicians are present (Senate Finance committee)
        let ids: Vec<&str> = cards.iter().map(|c| c.politician_id.as_str()).collect();
        assert!(ids.contains(&"W000802"), "Sheldon Whitehouse should be in ssfi");
        assert!(ids.contains(&"C000174"), "Tom Carper should be in ssfi");
    }
}
