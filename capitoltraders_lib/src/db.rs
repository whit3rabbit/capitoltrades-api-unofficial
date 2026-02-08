//! SQLite storage for Capitol Traders data.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::scrape::{ScrapedTrade, ScrapedTradeDetail};
use crate::types::{IssuerDetail, PoliticianDetail, Trade};

#[derive(thiserror::Error, Debug)]
pub enum DbError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("date parse error: {0}")]
    Date(#[from] chrono::ParseError),
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(Self { conn })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(Self { conn })
    }

    pub fn init(&self) -> Result<(), DbError> {
        // Check schema version before applying DDL so migrations can add
        // columns that new indexes reference.
        let version: i32 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version < 1 {
            self.migrate_v1()?;
            self.conn.pragma_update(None, "user_version", 1)?;
        }

        let schema = include_str!("../../schema/sqlite.sql");
        self.conn.execute_batch(schema)?;

        Ok(())
    }

    fn migrate_v1(&self) -> Result<(), DbError> {
        for sql in &[
            "ALTER TABLE trades ADD COLUMN enriched_at TEXT",
            "ALTER TABLE politicians ADD COLUMN enriched_at TEXT",
            "ALTER TABLE issuers ADD COLUMN enriched_at TEXT",
        ] {
            match self.conn.execute(sql, []) {
                Ok(_) => {}
                Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                    if msg.contains("duplicate column name")
                        || msg.contains("no such table") => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT value FROM ingest_meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(DbError::from)
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO ingest_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn max_trade_pub_date(&self) -> Result<Option<NaiveDate>, DbError> {
        let max_pub: Option<String> = self
            .conn
            .query_row("SELECT MAX(pub_date) FROM trades", [], |row| row.get(0))
            .optional()?;
        let Some(value) = max_pub else {
            return Ok(None);
        };
        let date_part = value.split('T').next().unwrap_or(&value);
        Ok(Some(NaiveDate::parse_from_str(date_part, "%Y-%m-%d")?))
    }

    pub fn trade_count(&self) -> Result<i64, DbError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(1) FROM trades", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn upsert_trades(&mut self, trades: &[Trade]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;

        {
            let mut stmt_asset = tx.prepare(
                "INSERT INTO assets (asset_id, asset_type, asset_ticker, instrument)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(asset_id) DO UPDATE SET
               asset_type = CASE
                 WHEN excluded.asset_type != 'unknown' THEN excluded.asset_type
                 ELSE assets.asset_type
               END,
               asset_ticker = COALESCE(excluded.asset_ticker, assets.asset_ticker),
               instrument = COALESCE(excluded.instrument, assets.instrument)",
            )?;
            let mut stmt_issuer = tx.prepare(
            "INSERT INTO issuers (issuer_id, state_id, c2iq, country, issuer_name, issuer_ticker, sector)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(issuer_id) DO UPDATE SET
               issuer_name = excluded.issuer_name,
               issuer_ticker = COALESCE(excluded.issuer_ticker, issuers.issuer_ticker),
               sector = COALESCE(excluded.sector, issuers.sector),
               state_id = COALESCE(excluded.state_id, issuers.state_id),
               c2iq = COALESCE(excluded.c2iq, issuers.c2iq),
               country = COALESCE(excluded.country, issuers.country),
               enriched_at = issuers.enriched_at",
            )?;
            let mut stmt_politician = tx.prepare(
            "INSERT INTO politicians (
               politician_id,
               state_id,
               party,
               party_other,
               district,
               first_name,
               last_name,
               nickname,
               middle_name,
               full_name,
               dob,
               gender,
               social_facebook,
               social_twitter,
               social_youtube,
               website,
               chamber
             )
             VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, ?6, NULL, NULL, ?7, ?8, NULL, NULL, NULL, NULL, ?9)
             ON CONFLICT(politician_id) DO UPDATE SET
               state_id = excluded.state_id,
               party = excluded.party,
               first_name = excluded.first_name,
               last_name = excluded.last_name,
               nickname = COALESCE(excluded.nickname, politicians.nickname),
               dob = excluded.dob,
               gender = excluded.gender,
               chamber = excluded.chamber,
               enriched_at = politicians.enriched_at",
            )?;
            let mut stmt_trade = tx.prepare(
                "INSERT INTO trades (
               tx_id,
               politician_id,
               asset_id,
               issuer_id,
               pub_date,
               filing_date,
               tx_date,
               tx_type,
               tx_type_extended,
               has_capital_gains,
               owner,
               chamber,
               price,
               size,
               size_range_high,
               size_range_low,
               value,
               filing_id,
               filing_url,
               reporting_gap,
               comment
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
               ?20, ?21
             )
             ON CONFLICT(tx_id) DO UPDATE SET
               politician_id = excluded.politician_id,
               asset_id = excluded.asset_id,
               issuer_id = excluded.issuer_id,
               pub_date = excluded.pub_date,
               filing_date = excluded.filing_date,
               tx_date = excluded.tx_date,
               tx_type = excluded.tx_type,
               tx_type_extended = excluded.tx_type_extended,
               has_capital_gains = CASE
                 WHEN excluded.has_capital_gains = 1 THEN excluded.has_capital_gains
                 ELSE trades.has_capital_gains
               END,
               owner = excluded.owner,
               chamber = excluded.chamber,
               price = COALESCE(excluded.price, trades.price),
               size = COALESCE(excluded.size, trades.size),
               size_range_high = COALESCE(excluded.size_range_high, trades.size_range_high),
               size_range_low = COALESCE(excluded.size_range_low, trades.size_range_low),
               value = excluded.value,
               filing_id = CASE
                 WHEN excluded.filing_id > 0 THEN excluded.filing_id
                 ELSE trades.filing_id
               END,
               filing_url = CASE
                 WHEN excluded.filing_url != '' THEN excluded.filing_url
                 ELSE trades.filing_url
               END,
               reporting_gap = excluded.reporting_gap,
               comment = excluded.comment,
               enriched_at = trades.enriched_at",
            )?;
            let mut stmt_trade_committees =
                tx.prepare("INSERT INTO trade_committees (tx_id, committee) VALUES (?1, ?2)")?;
            let mut stmt_trade_labels =
                tx.prepare("INSERT INTO trade_labels (tx_id, label) VALUES (?1, ?2)")?;

            for trade in trades {
                let db_trade: DbTrade = serde_json::from_value(serde_json::to_value(trade)?)?;

                stmt_asset.execute(params![
                    db_trade.asset_id,
                    db_trade.asset.asset_type,
                    db_trade.asset.asset_ticker,
                    db_trade.asset.instrument
                ])?;

                stmt_issuer.execute(params![
                    db_trade.issuer_id,
                    db_trade.issuer.state_id,
                    db_trade.issuer.c2iq,
                    db_trade.issuer.country,
                    db_trade.issuer.issuer_name,
                    db_trade.issuer.issuer_ticker,
                    db_trade.issuer.sector
                ])?;

                stmt_politician.execute(params![
                    db_trade.politician_id,
                    db_trade.politician.state_id,
                    db_trade.politician.party,
                    db_trade.politician.first_name,
                    db_trade.politician.last_name,
                    db_trade.politician.nickname,
                    db_trade.politician.dob,
                    db_trade.politician.gender,
                    db_trade.politician.chamber
                ])?;

                stmt_trade.execute(params![
                    db_trade.tx_id,
                    db_trade.politician_id,
                    db_trade.asset_id,
                    db_trade.issuer_id,
                    db_trade.pub_date,
                    db_trade.filing_date,
                    db_trade.tx_date,
                    db_trade.tx_type,
                    db_trade
                        .tx_type_extended
                        .as_ref()
                        .map(|val| val.to_string()),
                    if db_trade.has_capital_gains { 1 } else { 0 },
                    db_trade.owner,
                    db_trade.chamber,
                    db_trade.price,
                    db_trade.size,
                    db_trade.size_range_high,
                    db_trade.size_range_low,
                    db_trade.value,
                    db_trade.filing_id,
                    db_trade.filing_url,
                    db_trade.reporting_gap,
                    db_trade.comment
                ])?;

                tx.execute(
                    "DELETE FROM trade_committees WHERE tx_id = ?1",
                    params![db_trade.tx_id],
                )?;
                for committee in db_trade.committees {
                    stmt_trade_committees.execute(params![db_trade.tx_id, committee])?;
                }

                tx.execute(
                    "DELETE FROM trade_labels WHERE tx_id = ?1",
                    params![db_trade.tx_id],
                )?;
                for label in db_trade.labels {
                    stmt_trade_labels.execute(params![db_trade.tx_id, label])?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_scraped_trades(&mut self, trades: &[ScrapedTrade]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;

        {
            let mut stmt_asset = tx.prepare(
                "INSERT INTO assets (asset_id, asset_type, asset_ticker, instrument)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(asset_id) DO UPDATE SET
                   asset_type = CASE
                     WHEN excluded.asset_type != 'unknown' THEN excluded.asset_type
                     ELSE assets.asset_type
                   END,
                   asset_ticker = COALESCE(excluded.asset_ticker, assets.asset_ticker),
                   instrument = COALESCE(excluded.instrument, assets.instrument)",
            )?;
            let mut stmt_issuer = tx.prepare(
                "INSERT INTO issuers (issuer_id, state_id, c2iq, country, issuer_name, issuer_ticker, sector)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(issuer_id) DO UPDATE SET
                   issuer_name = excluded.issuer_name,
                   issuer_ticker = COALESCE(excluded.issuer_ticker, issuers.issuer_ticker),
                   sector = COALESCE(excluded.sector, issuers.sector),
                   state_id = COALESCE(excluded.state_id, issuers.state_id),
                   c2iq = COALESCE(excluded.c2iq, issuers.c2iq),
                   country = COALESCE(excluded.country, issuers.country),
                   enriched_at = issuers.enriched_at",
            )?;
            let mut stmt_politician = tx.prepare(
                "INSERT INTO politicians (
                   politician_id,
                   state_id,
                   party,
                   party_other,
                   district,
                   first_name,
                   last_name,
                   nickname,
                   middle_name,
                   full_name,
                   dob,
                   gender,
                   social_facebook,
                   social_twitter,
                   social_youtube,
                   website,
                   chamber
                 )
                 VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, ?6, NULL, ?7, ?8, ?9, NULL, NULL, NULL, NULL, ?10)
                 ON CONFLICT(politician_id) DO UPDATE SET
                   state_id = excluded.state_id,
                   party = excluded.party,
                   first_name = excluded.first_name,
                   last_name = excluded.last_name,
                   nickname = COALESCE(excluded.nickname, politicians.nickname),
                   full_name = COALESCE(excluded.full_name, politicians.full_name),
                   dob = excluded.dob,
                   gender = excluded.gender,
                   chamber = excluded.chamber,
                   enriched_at = politicians.enriched_at",
            )?;
            let mut stmt_trade = tx.prepare(
                "INSERT INTO trades (
                   tx_id,
                   politician_id,
                   asset_id,
                   issuer_id,
                   pub_date,
                   filing_date,
                   tx_date,
                   tx_type,
                   tx_type_extended,
                   has_capital_gains,
                   owner,
                   chamber,
                   price,
                   size,
                   size_range_high,
                   size_range_low,
                   value,
                   filing_id,
                   filing_url,
                   reporting_gap,
                   comment
                 )
                 VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19,
                   ?20, ?21
                 )
                 ON CONFLICT(tx_id) DO UPDATE SET
                   politician_id = excluded.politician_id,
                   asset_id = excluded.asset_id,
                   issuer_id = excluded.issuer_id,
                   pub_date = excluded.pub_date,
                   filing_date = excluded.filing_date,
                   tx_date = excluded.tx_date,
                   tx_type = excluded.tx_type,
                   tx_type_extended = excluded.tx_type_extended,
                   has_capital_gains = CASE
                     WHEN excluded.has_capital_gains = 1 THEN excluded.has_capital_gains
                     ELSE trades.has_capital_gains
                   END,
                   owner = excluded.owner,
                   chamber = excluded.chamber,
                   price = COALESCE(excluded.price, trades.price),
                   size = COALESCE(excluded.size, trades.size),
                   size_range_high = COALESCE(excluded.size_range_high, trades.size_range_high),
                   size_range_low = COALESCE(excluded.size_range_low, trades.size_range_low),
                   value = excluded.value,
                   filing_id = CASE
                     WHEN excluded.filing_id > 0 THEN excluded.filing_id
                     ELSE trades.filing_id
                   END,
                   filing_url = CASE
                     WHEN excluded.filing_url != '' THEN excluded.filing_url
                     ELSE trades.filing_url
                   END,
                   reporting_gap = excluded.reporting_gap,
                   comment = excluded.comment,
                   enriched_at = trades.enriched_at",
            )?;

            for trade in trades {
                let asset_id = trade.tx_id;
                let filing_date = trade.pub_date.split('T').next().unwrap_or(&trade.pub_date);
                let full_name = format!(
                    "{} {}",
                    trade.politician.first_name, trade.politician.last_name
                );
                let filing_id = trade.filing_id.unwrap_or(0);
                let filing_url = trade.filing_url.as_deref().unwrap_or("");

                stmt_asset.execute(params![asset_id, "unknown", None::<String>, None::<String>])?;

                stmt_issuer.execute(params![
                    trade.issuer_id,
                    trade.issuer.state_id,
                    trade.issuer.c2iq,
                    trade.issuer.country,
                    trade.issuer.issuer_name,
                    normalize_empty(trade.issuer.issuer_ticker.as_deref()),
                    trade.issuer.sector
                ])?;

                stmt_politician.execute(params![
                    trade.politician_id,
                    trade.politician.state_id,
                    trade.politician.party,
                    trade.politician.first_name,
                    trade.politician.last_name,
                    trade.politician.nickname,
                    full_name,
                    trade.politician.dob,
                    trade.politician.gender,
                    trade.politician.chamber
                ])?;

                stmt_trade.execute(params![
                    trade.tx_id,
                    trade.politician_id,
                    asset_id,
                    trade.issuer_id,
                    trade.pub_date,
                    filing_date,
                    trade.tx_date,
                    trade.tx_type,
                    trade.tx_type_extended.as_ref().map(|val| val.to_string()),
                    0,
                    trade.owner,
                    trade.chamber,
                    trade.price,
                    None::<i64>,
                    None::<i64>,
                    None::<i64>,
                    trade.value,
                    filing_id,
                    filing_url,
                    trade.reporting_gap,
                    trade.comment
                ])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_politicians(&mut self, politicians: &[PoliticianDetail]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;

        {
            let mut stmt_politician = tx.prepare(
                "INSERT INTO politicians (
               politician_id,
               state_id,
               party,
               party_other,
               district,
               first_name,
               last_name,
               nickname,
               middle_name,
               full_name,
               dob,
               gender,
               social_facebook,
               social_twitter,
               social_youtube,
               website,
               chamber
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(politician_id) DO UPDATE SET
               state_id = excluded.state_id,
               party = excluded.party,
               party_other = excluded.party_other,
               district = excluded.district,
               first_name = excluded.first_name,
               last_name = excluded.last_name,
               nickname = excluded.nickname,
               middle_name = excluded.middle_name,
               full_name = excluded.full_name,
               dob = excluded.dob,
               gender = excluded.gender,
               social_facebook = excluded.social_facebook,
               social_twitter = excluded.social_twitter,
               social_youtube = excluded.social_youtube,
               website = excluded.website,
               chamber = excluded.chamber,
               enriched_at = politicians.enriched_at",
            )?;

            let mut stmt_stats = tx.prepare(
                "INSERT INTO politician_stats (
               politician_id,
               date_last_traded,
               count_trades,
               count_issuers,
               volume
             )
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(politician_id) DO UPDATE SET
               date_last_traded = excluded.date_last_traded,
               count_trades = excluded.count_trades,
               count_issuers = excluded.count_issuers,
               volume = excluded.volume",
            )?;

            let mut stmt_committees = tx.prepare(
                "INSERT INTO politician_committees (politician_id, committee) VALUES (?1, ?2)",
            )?;

            for politician in politicians {
                let db_pol: DbPoliticianDetail =
                    serde_json::from_value(serde_json::to_value(politician)?)?;

                let party_other = db_pol.party_other.map(|val| match val {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                });

                stmt_politician.execute(params![
                    db_pol.politician_id,
                    db_pol.state_id,
                    db_pol.party,
                    party_other,
                    db_pol.district,
                    db_pol.first_name,
                    db_pol.last_name,
                    db_pol.nickname,
                    db_pol.middle_name,
                    db_pol.full_name,
                    db_pol.dob,
                    db_pol.gender,
                    db_pol.social_facebook,
                    db_pol.social_twitter,
                    db_pol.social_youtube,
                    db_pol.website,
                    db_pol.chamber
                ])?;

                stmt_stats.execute(params![
                    db_pol.politician_id,
                    db_pol.stats.date_last_traded,
                    db_pol.stats.count_trades,
                    db_pol.stats.count_issuers,
                    db_pol.stats.volume
                ])?;

                tx.execute(
                    "DELETE FROM politician_committees WHERE politician_id = ?1",
                    params![db_pol.politician_id],
                )?;
                for committee in db_pol.committees {
                    stmt_committees.execute(params![db_pol.politician_id, committee])?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_politician_stats(&mut self, stats: &[PoliticianStatsRow]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO politician_stats (
                   politician_id,
                   date_last_traded,
                   count_trades,
                   count_issuers,
                   volume
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(politician_id) DO UPDATE SET
                   date_last_traded = excluded.date_last_traded,
                   count_trades = excluded.count_trades,
                   count_issuers = excluded.count_issuers,
                   volume = excluded.volume",
            )?;
            for row in stats {
                stmt.execute(params![
                    row.politician_id,
                    row.date_last_traded,
                    row.count_trades,
                    row.count_issuers,
                    row.volume
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn upsert_issuers(&mut self, issuers: &[IssuerDetail]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;

        {
            let mut stmt_issuer = tx.prepare(
            "INSERT INTO issuers (issuer_id, state_id, c2iq, country, issuer_name, issuer_ticker, sector)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(issuer_id) DO UPDATE SET
               state_id = excluded.state_id,
               c2iq = excluded.c2iq,
               country = excluded.country,
               issuer_name = excluded.issuer_name,
               issuer_ticker = excluded.issuer_ticker,
               sector = excluded.sector,
               enriched_at = issuers.enriched_at",
            )?;

            let mut stmt_stats = tx.prepare(
                "INSERT INTO issuer_stats (
               issuer_id,
               count_trades,
               count_politicians,
               volume,
               date_last_traded
             )
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(issuer_id) DO UPDATE SET
               count_trades = excluded.count_trades,
               count_politicians = excluded.count_politicians,
               volume = excluded.volume,
               date_last_traded = excluded.date_last_traded",
            )?;

            let mut stmt_performance = tx.prepare(
                "INSERT INTO issuer_performance (
               issuer_id,
               mcap,
               trailing1,
               trailing1_change,
               trailing7,
               trailing7_change,
               trailing30,
               trailing30_change,
               trailing90,
               trailing90_change,
               trailing365,
               trailing365_change,
               wtd,
               wtd_change,
               mtd,
               mtd_change,
               qtd,
               qtd_change,
               ytd,
               ytd_change
             )
             VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
               ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
             )
             ON CONFLICT(issuer_id) DO UPDATE SET
               mcap = excluded.mcap,
               trailing1 = excluded.trailing1,
               trailing1_change = excluded.trailing1_change,
               trailing7 = excluded.trailing7,
               trailing7_change = excluded.trailing7_change,
               trailing30 = excluded.trailing30,
               trailing30_change = excluded.trailing30_change,
               trailing90 = excluded.trailing90,
               trailing90_change = excluded.trailing90_change,
               trailing365 = excluded.trailing365,
               trailing365_change = excluded.trailing365_change,
               wtd = excluded.wtd,
               wtd_change = excluded.wtd_change,
               mtd = excluded.mtd,
               mtd_change = excluded.mtd_change,
               qtd = excluded.qtd,
               qtd_change = excluded.qtd_change,
               ytd = excluded.ytd,
               ytd_change = excluded.ytd_change",
            )?;

            let mut stmt_eod = tx.prepare(
                "INSERT INTO issuer_eod_prices (issuer_id, price_date, price)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(issuer_id, price_date) DO UPDATE SET price = excluded.price",
            )?;

            for issuer in issuers {
                let db_issuer: DbIssuerDetail =
                    serde_json::from_value(serde_json::to_value(issuer)?)?;

                stmt_issuer.execute(params![
                    db_issuer.issuer_id,
                    db_issuer.state_id,
                    db_issuer.c2iq,
                    db_issuer.country,
                    db_issuer.issuer_name,
                    db_issuer.issuer_ticker,
                    db_issuer.sector
                ])?;

                stmt_stats.execute(params![
                    db_issuer.issuer_id,
                    db_issuer.stats.count_trades,
                    db_issuer.stats.count_politicians,
                    db_issuer.stats.volume,
                    db_issuer.stats.date_last_traded
                ])?;

                if let Some(perf) = db_issuer.performance {
                    stmt_performance.execute(params![
                        db_issuer.issuer_id,
                        perf.mcap,
                        perf.trailing1,
                        perf.trailing1_change,
                        perf.trailing7,
                        perf.trailing7_change,
                        perf.trailing30,
                        perf.trailing30_change,
                        perf.trailing90,
                        perf.trailing90_change,
                        perf.trailing365,
                        perf.trailing365_change,
                        perf.wtd,
                        perf.wtd_change,
                        perf.mtd,
                        perf.mtd_change,
                        perf.qtd,
                        perf.qtd_change,
                        perf.ytd,
                        perf.ytd_change
                    ])?;

                    for row in perf.eod_prices {
                        if let Some((date, price)) = eod_pair(&row) {
                            stmt_eod.execute(params![db_issuer.issuer_id, date, price])?;
                        }
                    }
                } else {
                    tx.execute(
                        "DELETE FROM issuer_performance WHERE issuer_id = ?1",
                        params![db_issuer.issuer_id],
                    )?;
                    tx.execute(
                        "DELETE FROM issuer_eod_prices WHERE issuer_id = ?1",
                        params![db_issuer.issuer_id],
                    )?;
                }
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_issuer_stats(&mut self, stats: &[IssuerStatsRow]) -> Result<(), DbError> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO issuer_stats (
                   issuer_id,
                   count_trades,
                   count_politicians,
                   volume,
                   date_last_traded
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(issuer_id) DO UPDATE SET
                   count_trades = excluded.count_trades,
                   count_politicians = excluded.count_politicians,
                   volume = excluded.volume,
                   date_last_traded = excluded.date_last_traded",
            )?;
            for row in stats {
                stmt.execute(params![
                    row.issuer_id,
                    row.count_trades,
                    row.count_politicians,
                    row.volume,
                    row.date_last_traded
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Persist extracted trade detail fields to the database.
    ///
    /// Updates the trades table (with COALESCE/CASE sentinel protection),
    /// the assets table (asset_type, only when current value is "unknown"),
    /// and the trade_committees / trade_labels join tables (delete+insert).
    /// Always sets `enriched_at` to the current UTC timestamp.
    pub fn update_trade_detail(
        &self,
        tx_id: i64,
        detail: &ScrapedTradeDetail,
    ) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;

        // 1. Update trades table with COALESCE for nullable fields and
        //    CASE for sentinel-protected fields.
        tx.execute(
            "UPDATE trades SET
               price = COALESCE(?1, price),
               size = COALESCE(?2, size),
               size_range_high = COALESCE(?3, size_range_high),
               size_range_low = COALESCE(?4, size_range_low),
               filing_id = CASE WHEN ?5 > 0 THEN ?5 ELSE filing_id END,
               filing_url = CASE WHEN ?6 != '' THEN ?6 ELSE filing_url END,
               has_capital_gains = COALESCE(?7, has_capital_gains),
               enriched_at = ?8
             WHERE tx_id = ?9",
            params![
                detail.price,
                detail.size,
                detail.size_range_high,
                detail.size_range_low,
                detail.filing_id.unwrap_or(0),
                detail.filing_url.as_deref().unwrap_or(""),
                detail.has_capital_gains.map(|b| if b { 1 } else { 0 }),
                chrono::Utc::now().to_rfc3339(),
                tx_id,
            ],
        )?;

        // 2. Update assets table -- only upgrade from "unknown" to a real type.
        if let Some(ref asset_type) = detail.asset_type {
            if asset_type != "unknown" {
                tx.execute(
                    "UPDATE assets SET asset_type = ?1
                     WHERE asset_id = ?2 AND asset_type = 'unknown'",
                    params![asset_type, tx_id],
                )?;
            }
        }

        // 3. Update trade_committees (delete+insert).
        if !detail.committees.is_empty() {
            tx.execute(
                "DELETE FROM trade_committees WHERE tx_id = ?1",
                params![tx_id],
            )?;
            let mut stmt = tx.prepare(
                "INSERT INTO trade_committees (tx_id, committee) VALUES (?1, ?2)",
            )?;
            for committee in &detail.committees {
                stmt.execute(params![tx_id, committee])?;
            }
        }

        // 4. Update trade_labels (delete+insert).
        if !detail.labels.is_empty() {
            tx.execute(
                "DELETE FROM trade_labels WHERE tx_id = ?1",
                params![tx_id],
            )?;
            let mut stmt = tx.prepare(
                "INSERT INTO trade_labels (tx_id, label) VALUES (?1, ?2)",
            )?;
            for label in &detail.labels {
                stmt.execute(params![tx_id, label])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn count_unenriched_trades(&self) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades WHERE enriched_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn get_unenriched_trade_ids(&self, limit: Option<i64>) -> Result<Vec<i64>, DbError> {
        let sql = match limit {
            Some(n) => format!(
                "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id LIMIT {}",
                n
            ),
            None => "SELECT tx_id FROM trades WHERE enriched_at IS NULL ORDER BY tx_id".to_string(),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    }

    pub fn get_unenriched_politician_ids(
        &self,
        limit: Option<i64>,
    ) -> Result<Vec<String>, DbError> {
        let sql = match limit {
            Some(n) => format!(
                "SELECT politician_id FROM politicians WHERE enriched_at IS NULL ORDER BY politician_id LIMIT {}",
                n
            ),
            None => "SELECT politician_id FROM politicians WHERE enriched_at IS NULL ORDER BY politician_id".to_string(),
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    pub fn get_unenriched_issuer_ids(&self, limit: Option<i64>) -> Result<Vec<i64>, DbError> {
        let sql = match limit {
            Some(n) => format!(
                "SELECT issuer_id FROM issuers WHERE enriched_at IS NULL ORDER BY issuer_id LIMIT {}",
                n
            ),
            None => {
                "SELECT issuer_id FROM issuers WHERE enriched_at IS NULL ORDER BY issuer_id"
                    .to_string()
            }
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(ids)
    }

    /// Query enriched trades with JOINed politician, issuer, asset,
    /// committee, and label data. Supports filtering by party, state,
    /// transaction type, politician name, issuer name/ticker, and date range.
    pub fn query_trades(&self, filter: &DbTradeFilter) -> Result<Vec<DbTradeRow>, DbError> {
        let mut sql = String::from(
            "SELECT t.tx_id, t.pub_date, t.tx_date, t.tx_type, t.value,
                    t.price, t.size, t.filing_url, t.reporting_gap, t.enriched_at,
                    p.first_name || ' ' || p.last_name AS politician_name,
                    p.party, p.state_id, p.chamber,
                    i.issuer_name, i.issuer_ticker,
                    a.asset_type,
                    COALESCE(GROUP_CONCAT(DISTINCT tc.committee), '') AS committees,
                    COALESCE(GROUP_CONCAT(DISTINCT tl.label), '') AS labels
             FROM trades t
             JOIN politicians p ON t.politician_id = p.politician_id
             JOIN issuers i ON t.issuer_id = i.issuer_id
             JOIN assets a ON t.asset_id = a.asset_id
             LEFT JOIN trade_committees tc ON t.tx_id = tc.tx_id
             LEFT JOIN trade_labels tl ON t.tx_id = tl.tx_id
             WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref party) = filter.party {
            sql.push_str(&format!(" AND p.party = ?{}", param_idx));
            params_vec.push(Box::new(party.clone()));
            param_idx += 1;
        }
        if let Some(ref state) = filter.state {
            sql.push_str(&format!(" AND UPPER(p.state_id) = UPPER(?{})", param_idx));
            params_vec.push(Box::new(state.clone()));
            param_idx += 1;
        }
        if let Some(ref tx_type) = filter.tx_type {
            sql.push_str(&format!(" AND t.tx_type = ?{}", param_idx));
            params_vec.push(Box::new(tx_type.clone()));
            param_idx += 1;
        }
        if let Some(ref name) = filter.name {
            sql.push_str(&format!(
                " AND (p.first_name || ' ' || p.last_name) LIKE ?{}",
                param_idx
            ));
            params_vec.push(Box::new(format!("%{}%", name)));
            param_idx += 1;
        }
        if let Some(ref issuer) = filter.issuer {
            sql.push_str(&format!(
                " AND (i.issuer_name LIKE ?{n} OR i.issuer_ticker LIKE ?{n})",
                n = param_idx
            ));
            params_vec.push(Box::new(format!("%{}%", issuer)));
            param_idx += 1;
        }
        if let Some(ref since) = filter.since {
            sql.push_str(&format!(" AND t.pub_date >= ?{}", param_idx));
            params_vec.push(Box::new(since.clone()));
            param_idx += 1;
        }
        if let Some(ref until) = filter.until {
            sql.push_str(&format!(" AND t.pub_date <= ?{}", param_idx));
            params_vec.push(Box::new(until.clone()));
            param_idx += 1;
        }

        sql.push_str(" GROUP BY t.tx_id ORDER BY t.pub_date DESC");

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let _ = param_idx; // suppress unused warning

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let committees_str: String = row.get(17)?;
            let labels_str: String = row.get(18)?;

            Ok(DbTradeRow {
                tx_id: row.get(0)?,
                pub_date: row.get(1)?,
                tx_date: row.get(2)?,
                tx_type: row.get(3)?,
                value: row.get(4)?,
                price: row.get(5)?,
                size: row.get(6)?,
                filing_url: row.get(7)?,
                reporting_gap: row.get(8)?,
                enriched_at: row.get(9)?,
                politician_name: row.get(10)?,
                party: row.get(11)?,
                state: row.get(12)?,
                chamber: row.get(13)?,
                issuer_name: row.get(14)?,
                issuer_ticker: row.get::<_, Option<String>>(15)?.unwrap_or_default(),
                asset_type: row.get(16)?,
                committees: if committees_str.is_empty() {
                    Vec::new()
                } else {
                    committees_str.split(',').map(|s| s.to_string()).collect()
                },
                labels: if labels_str.is_empty() {
                    Vec::new()
                } else {
                    labels_str.split(',').map(|s| s.to_string()).collect()
                },
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

pub struct PoliticianStatsRow {
    pub politician_id: String,
    pub date_last_traded: Option<String>,
    pub count_trades: i64,
    pub count_issuers: i64,
    pub volume: i64,
}

pub struct IssuerStatsRow {
    pub issuer_id: i64,
    pub count_trades: i64,
    pub count_politicians: i64,
    pub volume: i64,
    pub date_last_traded: String,
}

/// A fully-joined trade row returned by [`Db::query_trades`].
///
/// Includes politician, issuer, asset, committee, and label data merged
/// from six tables via SQL JOINs and GROUP_CONCAT.
#[derive(Debug, Clone, Serialize)]
pub struct DbTradeRow {
    pub tx_id: i64,
    pub pub_date: String,
    pub tx_date: String,
    pub tx_type: String,
    pub value: i64,
    pub price: Option<f64>,
    pub size: Option<i64>,
    pub filing_url: String,
    pub reporting_gap: i64,
    pub enriched_at: Option<String>,
    pub politician_name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub issuer_name: String,
    pub issuer_ticker: String,
    pub asset_type: String,
    pub committees: Vec<String>,
    pub labels: Vec<String>,
}

/// Filter parameters for [`Db::query_trades`].
#[derive(Debug, Default)]
pub struct DbTradeFilter {
    pub party: Option<String>,
    pub state: Option<String>,
    pub tx_type: Option<String>,
    pub name: Option<String>,
    pub issuer: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<i64>,
}

fn normalize_empty(value: Option<&str>) -> Option<String> {
    match value {
        Some(val) if val.trim().is_empty() => None,
        Some(val) => Some(val.to_string()),
        None => None,
    }
}

fn eod_pair(values: &[DbEodValue]) -> Option<(String, f64)> {
    let mut date: Option<String> = None;
    let mut price: Option<f64> = None;
    for value in values {
        match value {
            DbEodValue::Date(val) => date = Some(val.clone()),
            DbEodValue::Price(val) => price = Some(*val),
        }
    }
    match (date, price) {
        (Some(d), Some(p)) => Some((d, p)),
        _ => None,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbTrade {
    #[serde(rename = "_txId")]
    tx_id: i64,
    #[serde(rename = "_politicianId")]
    politician_id: String,
    #[serde(rename = "_assetId")]
    asset_id: i64,
    #[serde(rename = "_issuerId")]
    issuer_id: i64,
    pub_date: String,
    filing_date: String,
    tx_date: String,
    tx_type: String,
    tx_type_extended: Option<serde_json::Value>,
    has_capital_gains: bool,
    owner: String,
    chamber: String,
    price: Option<f64>,
    size: Option<i64>,
    size_range_high: Option<i64>,
    size_range_low: Option<i64>,
    value: i64,
    filing_id: i64,
    #[serde(rename = "filingURL")]
    filing_url: String,
    reporting_gap: i64,
    comment: Option<String>,
    committees: Vec<String>,
    asset: DbAsset,
    issuer: DbTradeIssuer,
    politician: DbTradePolitician,
    labels: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbAsset {
    asset_type: String,
    asset_ticker: Option<String>,
    instrument: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbTradeIssuer {
    #[serde(rename = "_stateId")]
    state_id: Option<String>,
    #[serde(rename = "c2iq")]
    c2iq: Option<String>,
    country: Option<String>,
    issuer_name: String,
    issuer_ticker: Option<String>,
    sector: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbTradePolitician {
    #[serde(rename = "_stateId")]
    state_id: String,
    chamber: String,
    dob: String,
    first_name: String,
    gender: String,
    last_name: String,
    nickname: Option<String>,
    party: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbPoliticianDetail {
    #[serde(rename = "_politicianId")]
    politician_id: String,
    #[serde(rename = "_stateId")]
    state_id: String,
    party: String,
    party_other: Option<serde_json::Value>,
    district: Option<String>,
    first_name: String,
    last_name: String,
    nickname: Option<String>,
    middle_name: Option<String>,
    full_name: String,
    dob: String,
    gender: String,
    social_facebook: Option<String>,
    social_twitter: Option<String>,
    social_youtube: Option<String>,
    website: Option<String>,
    chamber: String,
    committees: Vec<String>,
    stats: DbPoliticianStats,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbPoliticianStats {
    date_last_traded: Option<String>,
    count_trades: i64,
    count_issuers: i64,
    volume: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbIssuerDetail {
    #[serde(rename = "_issuerId")]
    issuer_id: i64,
    #[serde(rename = "_stateId")]
    state_id: Option<String>,
    #[serde(rename = "c2iq")]
    c2iq: Option<String>,
    country: Option<String>,
    issuer_name: String,
    issuer_ticker: Option<String>,
    performance: Option<DbPerformance>,
    sector: Option<String>,
    stats: DbIssuerStats,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbIssuerStats {
    count_trades: i64,
    count_politicians: i64,
    volume: i64,
    date_last_traded: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbPerformance {
    #[serde(rename = "eodPrices")]
    eod_prices: Vec<Vec<DbEodValue>>,
    mcap: i64,
    trailing1: f64,
    trailing1_change: f64,
    trailing7: f64,
    trailing7_change: f64,
    trailing30: f64,
    trailing30_change: f64,
    trailing90: f64,
    trailing90_change: f64,
    trailing365: f64,
    trailing365_change: f64,
    wtd: f64,
    wtd_change: f64,
    mtd: f64,
    mtd_change: f64,
    qtd: f64,
    qtd_change: f64,
    ytd: f64,
    ytd_change: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DbEodValue {
    Price(f64),
    Date(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_test_db() -> Db {
        let db = Db::open_in_memory().expect("open in-memory db");
        db.init().expect("init schema");
        db
    }

    fn has_column(db: &Db, table: &str, column: &str) -> bool {
        let sql = format!("PRAGMA table_info({})", table);
        let mut stmt = db.conn.prepare(&sql).expect("prepare pragma");
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        names.contains(&column.to_string())
    }

    fn get_user_version(db: &Db) -> i32 {
        db.conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version")
    }

    /// The OLD schema without enriched_at columns, used for migration tests.
    const OLD_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS assets (
    asset_id INTEGER PRIMARY KEY,
    asset_type TEXT NOT NULL,
    asset_ticker TEXT,
    instrument TEXT
);
CREATE TABLE IF NOT EXISTS issuers (
    issuer_id INTEGER PRIMARY KEY,
    state_id TEXT,
    c2iq TEXT,
    country TEXT,
    issuer_name TEXT NOT NULL,
    issuer_ticker TEXT,
    sector TEXT
);
CREATE TABLE IF NOT EXISTS politicians (
    politician_id TEXT PRIMARY KEY,
    state_id TEXT NOT NULL,
    party TEXT NOT NULL,
    party_other TEXT,
    district TEXT,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    nickname TEXT,
    middle_name TEXT,
    full_name TEXT,
    dob TEXT NOT NULL,
    gender TEXT NOT NULL,
    social_facebook TEXT,
    social_twitter TEXT,
    social_youtube TEXT,
    website TEXT,
    chamber TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS trades (
    tx_id INTEGER PRIMARY KEY,
    politician_id TEXT NOT NULL,
    asset_id INTEGER NOT NULL,
    issuer_id INTEGER NOT NULL,
    pub_date TEXT NOT NULL,
    filing_date TEXT NOT NULL,
    tx_date TEXT NOT NULL,
    tx_type TEXT NOT NULL,
    tx_type_extended TEXT,
    has_capital_gains INTEGER NOT NULL,
    owner TEXT NOT NULL,
    chamber TEXT NOT NULL,
    price REAL,
    size INTEGER,
    size_range_high INTEGER,
    size_range_low INTEGER,
    value INTEGER NOT NULL,
    filing_id INTEGER NOT NULL,
    filing_url TEXT NOT NULL,
    reporting_gap INTEGER NOT NULL,
    comment TEXT,
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS trade_committees (
    tx_id INTEGER NOT NULL,
    committee TEXT NOT NULL,
    PRIMARY KEY (tx_id, committee),
    FOREIGN KEY (tx_id) REFERENCES trades(tx_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS trade_labels (
    tx_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    PRIMARY KEY (tx_id, label),
    FOREIGN KEY (tx_id) REFERENCES trades(tx_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS politician_committees (
    politician_id TEXT NOT NULL,
    committee TEXT NOT NULL,
    PRIMARY KEY (politician_id, committee),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS politician_stats (
    politician_id TEXT PRIMARY KEY,
    date_last_traded TEXT,
    count_trades INTEGER NOT NULL,
    count_issuers INTEGER NOT NULL,
    volume INTEGER NOT NULL,
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS issuer_stats (
    issuer_id INTEGER PRIMARY KEY,
    count_trades INTEGER NOT NULL,
    count_politicians INTEGER NOT NULL,
    volume INTEGER NOT NULL,
    date_last_traded TEXT NOT NULL,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS issuer_performance (
    issuer_id INTEGER PRIMARY KEY,
    mcap INTEGER NOT NULL,
    trailing1 REAL NOT NULL,
    trailing1_change REAL NOT NULL,
    trailing7 REAL NOT NULL,
    trailing7_change REAL NOT NULL,
    trailing30 REAL NOT NULL,
    trailing30_change REAL NOT NULL,
    trailing90 REAL NOT NULL,
    trailing90_change REAL NOT NULL,
    trailing365 REAL NOT NULL,
    trailing365_change REAL NOT NULL,
    wtd REAL NOT NULL,
    wtd_change REAL NOT NULL,
    mtd REAL NOT NULL,
    mtd_change REAL NOT NULL,
    qtd REAL NOT NULL,
    qtd_change REAL NOT NULL,
    ytd REAL NOT NULL,
    ytd_change REAL NOT NULL,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS issuer_eod_prices (
    issuer_id INTEGER NOT NULL,
    price_date TEXT NOT NULL,
    price REAL NOT NULL,
    PRIMARY KEY (issuer_id, price_date),
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS ingest_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_trades_politician ON trades(politician_id);
CREATE INDEX IF NOT EXISTS idx_trades_issuer ON trades(issuer_id);
CREATE INDEX IF NOT EXISTS idx_trades_pub_date ON trades(pub_date);
CREATE INDEX IF NOT EXISTS idx_trades_tx_date ON trades(tx_date);
CREATE INDEX IF NOT EXISTS idx_politicians_party ON politicians(party);
CREATE INDEX IF NOT EXISTS idx_politicians_state ON politicians(state_id);
CREATE INDEX IF NOT EXISTS idx_issuers_sector ON issuers(sector);
CREATE INDEX IF NOT EXISTS idx_trade_labels_label ON trade_labels(label);
CREATE INDEX IF NOT EXISTS idx_trade_committees_committee ON trade_committees(committee);
CREATE INDEX IF NOT EXISTS idx_politician_committees_committee ON politician_committees(committee);
CREATE INDEX IF NOT EXISTS idx_eod_prices_date ON issuer_eod_prices(price_date);
";

    #[test]
    fn test_init_creates_enriched_at_columns() {
        let db = open_test_db();
        assert!(has_column(&db, "trades", "enriched_at"), "trades missing enriched_at");
        assert!(has_column(&db, "politicians", "enriched_at"), "politicians missing enriched_at");
        assert!(has_column(&db, "issuers", "enriched_at"), "issuers missing enriched_at");
    }

    #[test]
    fn test_init_idempotent() {
        let db = open_test_db();
        // Call init a second time -- must not error
        db.init().expect("second init should not error");
        assert_eq!(get_user_version(&db), 1);
    }

    #[test]
    fn test_migration_on_existing_db() {
        // Simulate a pre-migration database: create old schema, leave user_version at 0
        let db = Db::open_in_memory().expect("open in-memory db");
        db.conn.execute_batch(OLD_SCHEMA).expect("create old schema");

        // Verify no enriched_at columns yet
        assert!(!has_column(&db, "trades", "enriched_at"));
        assert!(!has_column(&db, "politicians", "enriched_at"));
        assert!(!has_column(&db, "issuers", "enriched_at"));
        assert_eq!(get_user_version(&db), 0);

        // Insert a test politician row before migration
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
                [],
            )
            .expect("insert test politician");

        // Run init which should apply migration
        db.init().expect("init with migration");

        // Verify enriched_at columns exist
        assert!(has_column(&db, "trades", "enriched_at"));
        assert!(has_column(&db, "politicians", "enriched_at"));
        assert!(has_column(&db, "issuers", "enriched_at"));

        // Verify user_version is now 1
        assert_eq!(get_user_version(&db), 1);

        // Verify pre-existing data is preserved
        let name: String = db
            .conn
            .query_row(
                "SELECT first_name FROM politicians WHERE politician_id = 'P000001'",
                [],
                |row| row.get(0),
            )
            .expect("query test politician");
        assert_eq!(name, "Jane");
    }

    #[test]
    fn test_enriched_at_defaults_to_null() {
        let db = open_test_db();

        // Insert required parent rows first (foreign keys are ON)
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert asset");
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name) VALUES (1, 'TestCorp')",
                [],
            )
            .expect("insert issuer");
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
                [],
            )
            .expect("insert politician");

        // Insert a trade row without specifying enriched_at
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date,
                 tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (1, 'P000001', 1, 1, '2025-01-01', '2025-01-01', '2025-01-01', 'buy', 0,
                 'self', 'senate', 50000, 100, 'https://example.com', 5)",
                [],
            )
            .expect("insert trade");

        // Verify enriched_at is NULL
        let enriched: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM trades WHERE tx_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("query enriched_at");
        assert!(enriched.is_none(), "enriched_at should default to NULL");
    }

    // --- Test helpers for upsert_scraped_trades tests ---

    fn make_test_scraped_trade(tx_id: i64, politician_id: &str, issuer_id: i64) -> ScrapedTrade {
        use crate::scrape::{ScrapedIssuer, ScrapedPolitician};
        ScrapedTrade {
            tx_id,
            politician_id: politician_id.to_string(),
            issuer_id,
            chamber: "senate".to_string(),
            comment: None,
            issuer: ScrapedIssuer {
                state_id: None,
                c2iq: None,
                country: None,
                issuer_name: format!("TestCorp{}", issuer_id),
                issuer_ticker: Some("TST".to_string()),
                sector: None,
            },
            owner: "self".to_string(),
            politician: ScrapedPolitician {
                state_id: "CA".to_string(),
                chamber: "senate".to_string(),
                dob: "1970-01-01".to_string(),
                first_name: "Jane".to_string(),
                gender: "female".to_string(),
                last_name: "Doe".to_string(),
                nickname: None,
                party: "Democrat".to_string(),
            },
            price: None,
            pub_date: "2025-06-15T00:00:00Z".to_string(),
            reporting_gap: 5,
            tx_date: "2025-06-10".to_string(),
            tx_type: "buy".to_string(),
            tx_type_extended: None,
            value: 50000,
            filing_url: None,
            filing_id: None,
        }
    }

    // --- Upsert sentinel protection tests ---

    #[test]
    fn test_upsert_preserves_enriched_filing_id() {
        let mut db = open_test_db();
        // Insert with a real filing_id
        let mut trade = make_test_scraped_trade(100, "P000001", 1);
        trade.filing_id = Some(12345);
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        // Verify filing_id was set
        let fid: i64 = db
            .conn
            .query_row("SELECT filing_id FROM trades WHERE tx_id = 100", [], |row| {
                row.get(0)
            })
            .expect("query filing_id");
        assert_eq!(fid, 12345);

        // Re-upsert with filing_id=None (becomes 0 sentinel)
        let trade2 = make_test_scraped_trade(100, "P000001", 1);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        // filing_id should still be 12345, not 0
        let fid2: i64 = db
            .conn
            .query_row("SELECT filing_id FROM trades WHERE tx_id = 100", [], |row| {
                row.get(0)
            })
            .expect("query filing_id after re-upsert");
        assert_eq!(fid2, 12345, "filing_id should be preserved, not overwritten with 0");
    }

    #[test]
    fn test_upsert_preserves_enriched_filing_url() {
        let mut db = open_test_db();
        let mut trade = make_test_scraped_trade(101, "P000001", 1);
        trade.filing_url = Some("https://example.com/filing/123".to_string());
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        let url: String = db
            .conn
            .query_row("SELECT filing_url FROM trades WHERE tx_id = 101", [], |row| {
                row.get(0)
            })
            .expect("query filing_url");
        assert_eq!(url, "https://example.com/filing/123");

        // Re-upsert with filing_url=None (becomes "" sentinel)
        let trade2 = make_test_scraped_trade(101, "P000001", 1);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        let url2: String = db
            .conn
            .query_row("SELECT filing_url FROM trades WHERE tx_id = 101", [], |row| {
                row.get(0)
            })
            .expect("query filing_url after re-upsert");
        assert_eq!(
            url2, "https://example.com/filing/123",
            "filing_url should be preserved, not overwritten with empty string"
        );
    }

    #[test]
    fn test_upsert_preserves_enriched_price() {
        let mut db = open_test_db();
        let mut trade = make_test_scraped_trade(102, "P000001", 1);
        trade.price = Some(150.0);
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        let price: f64 = db
            .conn
            .query_row("SELECT price FROM trades WHERE tx_id = 102", [], |row| {
                row.get(0)
            })
            .expect("query price");
        assert!((price - 150.0).abs() < f64::EPSILON);

        // Re-upsert with price=None
        let trade2 = make_test_scraped_trade(102, "P000001", 1);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        let price2: Option<f64> = db
            .conn
            .query_row("SELECT price FROM trades WHERE tx_id = 102", [], |row| {
                row.get(0)
            })
            .expect("query price after re-upsert");
        assert_eq!(
            price2,
            Some(150.0),
            "price should be preserved via COALESCE, not overwritten with NULL"
        );
    }

    #[test]
    fn test_upsert_preserves_enriched_at_timestamp() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(103, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        // Simulate enrichment by setting enriched_at
        db.conn
            .execute(
                "UPDATE trades SET enriched_at = '2026-01-15T12:00:00Z' WHERE tx_id = 103",
                [],
            )
            .expect("set enriched_at");

        // Re-upsert the same trade
        let trade2 = make_test_scraped_trade(103, "P000001", 1);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        let enriched: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM trades WHERE tx_id = 103",
                [],
                |row| row.get(0),
            )
            .expect("query enriched_at");
        assert_eq!(
            enriched.as_deref(),
            Some("2026-01-15T12:00:00Z"),
            "enriched_at should be preserved across re-upsert"
        );
    }

    #[test]
    fn test_upsert_asset_type_sentinel_protection() {
        let mut db = open_test_db();
        // First insert creates asset with "unknown" type (scraped trade default)
        let trade = make_test_scraped_trade(104, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        // Simulate enrichment: set asset_type to "stock"
        db.conn
            .execute(
                "UPDATE assets SET asset_type = 'stock' WHERE asset_id = 104",
                [],
            )
            .expect("enrich asset_type");

        // Re-upsert same trade (inserts asset with "unknown" again)
        let trade2 = make_test_scraped_trade(104, "P000001", 1);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        let asset_type: String = db
            .conn
            .query_row(
                "SELECT asset_type FROM assets WHERE asset_id = 104",
                [],
                |row| row.get(0),
            )
            .expect("query asset_type");
        assert_eq!(
            asset_type, "stock",
            "asset_type should be preserved, not overwritten with 'unknown'"
        );
    }

    #[test]
    fn test_upsert_overwrites_sentinel_with_real_value() {
        let mut db = open_test_db();
        // First insert with sentinel filing_id (None -> 0)
        let trade = make_test_scraped_trade(105, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("first upsert");

        let fid: i64 = db
            .conn
            .query_row("SELECT filing_id FROM trades WHERE tx_id = 105", [], |row| {
                row.get(0)
            })
            .expect("query filing_id");
        assert_eq!(fid, 0, "initial filing_id should be 0 sentinel");

        // Re-upsert with a real filing_id
        let mut trade2 = make_test_scraped_trade(105, "P000001", 1);
        trade2.filing_id = Some(99999);
        db.upsert_scraped_trades(&[trade2]).expect("second upsert");

        let fid2: i64 = db
            .conn
            .query_row("SELECT filing_id FROM trades WHERE tx_id = 105", [], |row| {
                row.get(0)
            })
            .expect("query filing_id after re-upsert");
        assert_eq!(
            fid2, 99999,
            "filing_id should be overwritten when incoming value is non-sentinel"
        );
    }

    // --- Enrichment query tests ---

    #[test]
    fn test_get_unenriched_trade_ids_returns_all() {
        let mut db = open_test_db();
        for i in 1..=3 {
            let trade = make_test_scraped_trade(i, &format!("P00000{}", i), i);
            db.upsert_scraped_trades(&[trade]).expect("upsert");
        }

        let ids = db
            .get_unenriched_trade_ids(None)
            .expect("get_unenriched_trade_ids");
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn test_get_unenriched_trade_ids_excludes_enriched() {
        let mut db = open_test_db();
        for i in 1..=3 {
            let trade = make_test_scraped_trade(i, &format!("P00000{}", i), i);
            db.upsert_scraped_trades(&[trade]).expect("upsert");
        }

        // Mark trade 2 as enriched
        db.conn
            .execute(
                "UPDATE trades SET enriched_at = '2026-01-15T12:00:00Z' WHERE tx_id = 2",
                [],
            )
            .expect("set enriched_at");

        let ids = db
            .get_unenriched_trade_ids(None)
            .expect("get_unenriched_trade_ids");
        assert_eq!(ids, vec![1, 3], "should exclude enriched trade 2");
    }

    #[test]
    fn test_get_unenriched_trade_ids_with_limit() {
        let mut db = open_test_db();
        for i in 1..=5 {
            let trade = make_test_scraped_trade(i, &format!("P00000{}", i), i);
            db.upsert_scraped_trades(&[trade]).expect("upsert");
        }

        let ids = db
            .get_unenriched_trade_ids(Some(2))
            .expect("get_unenriched_trade_ids");
        assert_eq!(ids.len(), 2, "should return exactly 2 IDs");
        assert_eq!(ids, vec![1, 2], "should return lowest tx_ids first");
    }

    #[test]
    fn test_get_unenriched_politician_ids() {
        let mut db = open_test_db();
        let trade1 = make_test_scraped_trade(1, "P000001", 1);
        let trade2 = make_test_scraped_trade(2, "P000002", 2);
        db.upsert_scraped_trades(&[trade1, trade2])
            .expect("upsert");

        let ids = db
            .get_unenriched_politician_ids(None)
            .expect("get_unenriched_politician_ids");
        assert_eq!(ids, vec!["P000001", "P000002"]);
    }

    #[test]
    fn test_get_unenriched_issuer_ids() {
        let mut db = open_test_db();
        let trade1 = make_test_scraped_trade(1, "P000001", 10);
        let trade2 = make_test_scraped_trade(2, "P000002", 20);
        db.upsert_scraped_trades(&[trade1, trade2])
            .expect("upsert");

        let ids = db
            .get_unenriched_issuer_ids(None)
            .expect("get_unenriched_issuer_ids");
        assert_eq!(ids, vec![10, 20]);
    }

    // --- update_trade_detail tests ---

    fn make_test_trade_detail() -> ScrapedTradeDetail {
        ScrapedTradeDetail {
            filing_url: Some("https://efts.sec.gov/12345".to_string()),
            filing_id: Some(12345),
            asset_type: Some("stock".to_string()),
            size: Some(50000),
            size_range_high: Some(100000),
            size_range_low: Some(15001),
            price: Some(150.50),
            has_capital_gains: Some(false),
            committees: vec!["ssfi".to_string()],
            labels: vec!["faang".to_string()],
        }
    }

    #[test]
    fn test_update_trade_detail_basic() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(200, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        let detail = make_test_trade_detail();
        db.update_trade_detail(200, &detail).expect("update_trade_detail");

        let (price, size, size_hi, size_lo, fid, furl, hcg): (
            Option<f64>, Option<i64>, Option<i64>, Option<i64>, i64, String, i32,
        ) = db
            .conn
            .query_row(
                "SELECT price, size, size_range_high, size_range_low,
                        filing_id, filing_url, has_capital_gains
                 FROM trades WHERE tx_id = 200",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .expect("query trade");

        assert!((price.unwrap() - 150.50).abs() < f64::EPSILON);
        assert_eq!(size, Some(50000));
        assert_eq!(size_hi, Some(100000));
        assert_eq!(size_lo, Some(15001));
        assert_eq!(fid, 12345);
        assert_eq!(furl, "https://efts.sec.gov/12345");
        assert_eq!(hcg, 0); // false

        // Verify enriched_at was set
        let enriched: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM trades WHERE tx_id = 200",
                [],
                |row| row.get(0),
            )
            .expect("query enriched_at");
        assert!(enriched.is_some(), "enriched_at should be set");
    }

    #[test]
    fn test_update_trade_detail_coalesce_preserves_existing() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(201, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // First update with real values
        let detail = ScrapedTradeDetail {
            price: Some(50.0),
            size: Some(100),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(201, &detail).expect("first update");

        // Second update with None values -- should not overwrite
        let detail2 = ScrapedTradeDetail::default();
        db.update_trade_detail(201, &detail2).expect("second update");

        let (price, size): (Option<f64>, Option<i64>) = db
            .conn
            .query_row(
                "SELECT price, size FROM trades WHERE tx_id = 201",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query");

        assert_eq!(price, Some(50.0), "price preserved via COALESCE");
        assert_eq!(size, Some(100), "size preserved via COALESCE");
    }

    #[test]
    fn test_update_trade_detail_asset_type_update() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(202, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Verify initial asset_type is "unknown"
        let at: String = db
            .conn
            .query_row(
                "SELECT asset_type FROM assets WHERE asset_id = 202",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(at, "unknown");

        let detail = ScrapedTradeDetail {
            asset_type: Some("stock".to_string()),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(202, &detail).expect("update");

        let at2: String = db
            .conn
            .query_row(
                "SELECT asset_type FROM assets WHERE asset_id = 202",
                [],
                |row| row.get(0),
            )
            .expect("query after update");
        assert_eq!(at2, "stock", "asset_type should be updated from unknown to stock");
    }

    #[test]
    fn test_update_trade_detail_asset_type_no_overwrite() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(203, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // First update: set asset_type to "stock"
        let detail = ScrapedTradeDetail {
            asset_type: Some("stock".to_string()),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(203, &detail).expect("first update");

        // Second update: try to change asset_type to "etf"
        let detail2 = ScrapedTradeDetail {
            asset_type: Some("etf".to_string()),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(203, &detail2).expect("second update");

        let at: String = db
            .conn
            .query_row(
                "SELECT asset_type FROM assets WHERE asset_id = 203",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(at, "stock", "asset_type should NOT be overwritten once set to non-unknown");
    }

    #[test]
    fn test_update_trade_detail_asset_type_unknown_ignored() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(204, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Update with asset_type = "unknown" -- should be a no-op
        let detail = ScrapedTradeDetail {
            asset_type: Some("unknown".to_string()),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(204, &detail).expect("update");

        let at: String = db
            .conn
            .query_row(
                "SELECT asset_type FROM assets WHERE asset_id = 204",
                [],
                |row| row.get(0),
            )
            .expect("query");
        assert_eq!(at, "unknown", "asset_type should remain unknown when incoming is unknown");
    }

    #[test]
    fn test_update_trade_detail_committees() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(205, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        let detail = ScrapedTradeDetail {
            committees: vec!["ssfi".to_string(), "hsag".to_string()],
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(205, &detail).expect("update");

        let mut stmt = db
            .conn
            .prepare("SELECT committee FROM trade_committees WHERE tx_id = 205 ORDER BY committee")
            .expect("prepare");
        let committees: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(committees, vec!["hsag", "ssfi"]);
    }

    #[test]
    fn test_update_trade_detail_committees_replace() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(206, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // First: insert one committee
        let detail = ScrapedTradeDetail {
            committees: vec!["ssfi".to_string()],
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(206, &detail).expect("first update");

        // Second: replace with two different committees
        let detail2 = ScrapedTradeDetail {
            committees: vec!["hsag".to_string(), "hsap".to_string()],
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(206, &detail2).expect("second update");

        let mut stmt = db
            .conn
            .prepare("SELECT committee FROM trade_committees WHERE tx_id = 206 ORDER BY committee")
            .expect("prepare");
        let committees: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(committees, vec!["hsag", "hsap"], "old committee should be gone, new ones present");
    }

    #[test]
    fn test_update_trade_detail_labels() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(207, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        let detail = ScrapedTradeDetail {
            labels: vec!["faang".to_string(), "crypto".to_string()],
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(207, &detail).expect("update");

        let mut stmt = db
            .conn
            .prepare("SELECT label FROM trade_labels WHERE tx_id = 207 ORDER BY label")
            .expect("prepare");
        let labels: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(labels, vec!["crypto", "faang"]);
    }

    #[test]
    fn test_update_trade_detail_filing_sentinel() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(208, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // First update: set filing_id and filing_url
        let detail = ScrapedTradeDetail {
            filing_id: Some(12345),
            filing_url: Some("https://example.com/12345".to_string()),
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(208, &detail).expect("first update");

        // Second update: filing_id=None (sentinel 0), filing_url=None (sentinel "")
        let detail2 = ScrapedTradeDetail::default();
        db.update_trade_detail(208, &detail2).expect("second update");

        let (fid, furl): (i64, String) = db
            .conn
            .query_row(
                "SELECT filing_id, filing_url FROM trades WHERE tx_id = 208",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query");

        assert_eq!(fid, 12345, "filing_id preserved via CASE sentinel");
        assert_eq!(furl, "https://example.com/12345", "filing_url preserved via CASE sentinel");
    }

    #[test]
    fn test_update_trade_detail_enriched_at_set() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(209, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Verify enriched_at is initially NULL
        let before: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM trades WHERE tx_id = 209",
                [],
                |row| row.get(0),
            )
            .expect("query before");
        assert!(before.is_none(), "enriched_at should start as NULL");

        // Update
        let detail = ScrapedTradeDetail::default();
        db.update_trade_detail(209, &detail).expect("update");

        // Verify enriched_at is now set
        let after: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM trades WHERE tx_id = 209",
                [],
                |row| row.get(0),
            )
            .expect("query after");
        assert!(after.is_some(), "enriched_at should be set after update");
        let ts = after.unwrap();
        assert!(!ts.is_empty(), "enriched_at should be a non-empty timestamp");
        // Basic sanity: should start with a year
        assert!(ts.starts_with("20"), "enriched_at should be an RFC3339 timestamp: {}", ts);
    }

    // --- count_unenriched_trades tests ---

    #[test]
    fn test_count_unenriched_trades_zero() {
        let mut db = open_test_db();
        // Insert a trade and mark it enriched
        let trade = make_test_scraped_trade(300, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");
        db.conn
            .execute(
                "UPDATE trades SET enriched_at = '2026-01-15T12:00:00Z' WHERE tx_id = 300",
                [],
            )
            .expect("set enriched_at");

        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 0, "all trades enriched, count should be 0");
    }

    #[test]
    fn test_count_unenriched_trades_some() {
        let mut db = open_test_db();
        for i in 1..=5 {
            let trade = make_test_scraped_trade(310 + i, &format!("P00000{}", i), i);
            db.upsert_scraped_trades(&[trade]).expect("upsert");
        }
        // Enrich 2 of the 5
        db.conn
            .execute(
                "UPDATE trades SET enriched_at = '2026-01-15T12:00:00Z' WHERE tx_id IN (311, 313)",
                [],
            )
            .expect("set enriched_at");

        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 3, "3 of 5 trades should be unenriched");
    }

    #[test]
    fn test_count_unenriched_trades_all() {
        let mut db = open_test_db();
        for i in 1..=4 {
            let trade = make_test_scraped_trade(320 + i, &format!("P00000{}", i), i);
            db.upsert_scraped_trades(&[trade]).expect("upsert");
        }

        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 4, "all 4 trades should be unenriched");
    }

    // --- Enrichment pipeline integration tests ---

    #[test]
    fn test_enrichment_queue_empty() {
        let db = open_test_db();
        // No trades inserted at all
        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 0, "empty db should have 0 unenriched trades");

        let ids = db.get_unenriched_trade_ids(None).expect("get ids");
        assert!(ids.is_empty(), "empty db should return empty queue");
    }

    #[test]
    fn test_enrichment_queue_partial_enrichment() {
        let mut db = open_test_db();
        // Insert 3 trades with tx_ids 100, 200, 300
        let trades: Vec<ScrapedTrade> = vec![
            make_test_scraped_trade(100, "P000001", 1),
            make_test_scraped_trade(200, "P000002", 2),
            make_test_scraped_trade(300, "P000003", 3),
        ];
        db.upsert_scraped_trades(&trades).expect("upsert");

        // All 3 should be unenriched initially
        let ids = db.get_unenriched_trade_ids(None).expect("get ids");
        assert_eq!(ids, vec![100, 200, 300], "all 3 trades should be in queue");

        // Enrich trade 100 using update_trade_detail
        let detail = ScrapedTradeDetail::default();
        db.update_trade_detail(100, &detail).expect("enrich trade 100");

        // Now queue should exclude trade 100
        let ids = db.get_unenriched_trade_ids(None).expect("get ids after enrichment");
        assert_eq!(ids, vec![200, 300], "trade 100 should be skipped after enrichment");

        // Count should be 2
        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 2, "2 trades remain unenriched");
    }

    #[test]
    fn test_enrichment_queue_batch_size_limiting() {
        let mut db = open_test_db();
        // Insert 3 unenriched trades
        let trades: Vec<ScrapedTrade> = vec![
            make_test_scraped_trade(400, "P000001", 1),
            make_test_scraped_trade(500, "P000002", 2),
            make_test_scraped_trade(600, "P000003", 3),
        ];
        db.upsert_scraped_trades(&trades).expect("upsert");

        // batch_size=2 should return only 2 IDs
        let ids = db.get_unenriched_trade_ids(Some(2)).expect("get ids with limit");
        assert_eq!(ids.len(), 2, "batch_size=2 should return exactly 2 IDs");
        assert_eq!(ids, vec![400, 500], "should return lowest tx_ids first");

        // count_unenriched_trades is independent of limit
        let count = db.count_unenriched_trades().expect("count");
        assert_eq!(count, 3, "count should still be 3 regardless of batch_size");
    }
}
