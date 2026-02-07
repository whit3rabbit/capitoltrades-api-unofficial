//! HTML scraping utilities for CapitolTrades pages (no API).

use std::time::Duration;

use reqwest::StatusCode;
use serde::Deserialize;
use regex::Regex;

use capitoltrades_api::user_agent::get_user_agent;

#[derive(thiserror::Error, Debug)]
pub enum ScrapeError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected status {status}")]
    HttpStatus { status: StatusCode },
    #[error("missing RSC payload")]
    MissingPayload,
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse error: {0}")]
    Parse(String),
}

pub struct ScrapeClient {
    base_url: String,
    http: reqwest::Client,
}

pub struct ScrapePage<T> {
    pub data: Vec<T>,
    pub total_pages: Option<i64>,
    pub total_count: Option<i64>,
}

#[derive(Debug, Default)]
pub struct ScrapedTradeDetail {
    pub filing_url: Option<String>,
    pub filing_id: Option<i64>,
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
            .build()?;
        Ok(Self {
            base_url: "https://www.capitoltrades.com".to_string(),
            http,
        })
    }

    pub fn with_base_url(base_url: &str) -> Result<Self, ScrapeError> {
        let http = reqwest::Client::builder()
            .user_agent(get_user_agent())
            .timeout(Duration::from_secs(30))
            .build()?;
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

    pub async fn issuer_detail(
        &self,
        issuer_id: i64,
    ) -> Result<ScrapedIssuerDetail, ScrapeError> {
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

    pub async fn politician_detail(
        &self,
        politician_id: &str,
    ) -> Result<Option<ScrapedPolitician>, ScrapeError> {
        let url = format!("{}/politicians/{}", self.base_url, politician_id);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        Ok(extract_politician_detail(&payload))
    }

    pub async fn trade_detail(
        &self,
        trade_id: i64,
    ) -> Result<ScrapedTradeDetail, ScrapeError> {
        let url = format!("{}/trades/{}", self.base_url, trade_id);
        let html = self.fetch_html(&url).await?;
        let payload = extract_rsc_payload(&html)?;
        Ok(extract_trade_detail(&payload, trade_id))
    }

    async fn fetch_html(&self, url: &str) -> Result<String, ScrapeError> {
        let resp = self
            .http
            .get(url)
            .header("accept", "text/html,application/xhtml+xml")
            .header("accept-language", "en-US,en;q=0.9")
            .header("upgrade-insecure-requests", "1")
            .header("cache-control", "no-cache")
            .header("pragma", "no-cache")
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(ScrapeError::HttpStatus {
                status: resp.status(),
            });
        }

        Ok(resp.text().await?)
    }
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
    let id_re =
        Regex::new(r#"href":"/politicians/([A-Z]\d{6})""#).map_err(|e| {
            ScrapeError::Parse(format!("regex compile error: {}", e))
        })?;
    let card_re = Regex::new(
        r#"(?s)href":"/politicians/(?P<id>[A-Z]\d{6})".*?cell--name.*?children":"(?P<name>[^"]+)".*?party--(?P<party>democrat|republican|other).*?us-state-full--(?P<state>[a-z]{2}).*?cell--count-trades.*?children":"Trades".*?children":"(?P<trades>[\d,]+)".*?cell--count-issuers.*?children":"Issuers".*?children":"(?P<issuers>[\d,]+)".*?cell--volume.*?children":"Volume".*?children":"(?P<volume>[^"]+)".*?cell--last-traded.*?children":"Last Traded".*?children":"(?P<last>\d{4}-\d{2}-\d{2})""#,
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
            ScrapeError::Parse(format!(
                "invalid volume for politician {}",
                politician_id
            ))
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
