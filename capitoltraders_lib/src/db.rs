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

        if version < 2 {
            self.migrate_v2()?;
            self.conn.pragma_update(None, "user_version", 2)?;
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

    fn migrate_v2(&self) -> Result<(), DbError> {
        for sql in &[
            "ALTER TABLE trades ADD COLUMN trade_date_price REAL",
            "ALTER TABLE trades ADD COLUMN current_price REAL",
            "ALTER TABLE trades ADD COLUMN price_enriched_at TEXT",
            "ALTER TABLE trades ADD COLUMN estimated_shares REAL",
            "ALTER TABLE trades ADD COLUMN estimated_value REAL",
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

    /// Atomically replace all politician-committee memberships.
    ///
    /// Clears the entire politician_committees table and inserts the provided
    /// memberships. Uses an EXISTS subquery to silently skip politician_ids
    /// not present in the politicians table (handles politicians with no
    /// trades who appear on committee lists but have no DB record).
    ///
    /// Returns the number of rows actually inserted.
    pub fn replace_all_politician_committees(
        &self,
        memberships: &[(String, String)],
    ) -> Result<usize, DbError> {
        let tx = self.conn.unchecked_transaction()?;

        tx.execute("DELETE FROM politician_committees", [])?;

        let mut inserted = 0usize;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO politician_committees (politician_id, committee)
                 SELECT ?1, ?2 WHERE EXISTS (
                     SELECT 1 FROM politicians WHERE politician_id = ?1
                 )",
            )?;

            for (pol_id, committee) in memberships {
                let rows = stmt.execute(params![pol_id, committee])?;
                inserted += rows;
            }
        }

        tx.commit()?;
        Ok(inserted)
    }

    /// Mark all politicians as enriched by setting enriched_at on rows
    /// where it is currently NULL.
    pub fn mark_politicians_enriched(&self) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE politicians SET enriched_at = datetime('now') WHERE enriched_at IS NULL",
            [],
        )?;
        Ok(())
    }

    /// Count politicians that have not yet been enriched.
    pub fn count_unenriched_politicians(&self) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM politicians WHERE enriched_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count issuers that have not yet been enriched (enriched_at IS NULL).
    pub fn count_unenriched_issuers(&self) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM issuers WHERE enriched_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Persist scraped issuer detail data to the database.
    ///
    /// Updates the issuers table (with COALESCE protection for nullable fields),
    /// upserts issuer_stats, and writes performance + EOD prices if available.
    /// Always sets `enriched_at` to the current UTC timestamp.
    pub fn update_issuer_detail(
        &self,
        issuer_id: i64,
        detail: &crate::scrape::ScrapedIssuerDetail,
    ) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;

        // Step 1: UPDATE issuers base row with COALESCE protection
        tx.execute(
            "UPDATE issuers SET
               state_id = COALESCE(?1, state_id),
               c2iq = COALESCE(?2, c2iq),
               country = COALESCE(?3, country),
               issuer_name = ?4,
               issuer_ticker = COALESCE(?5, issuer_ticker),
               sector = COALESCE(?6, sector),
               enriched_at = datetime('now')
             WHERE issuer_id = ?7",
            params![
                detail.state_id,
                detail.c2iq,
                detail.country,
                detail.issuer_name,
                detail.issuer_ticker,
                detail.sector,
                issuer_id,
            ],
        )?;

        // Step 2: UPSERT issuer_stats
        tx.execute(
            "INSERT INTO issuer_stats (issuer_id, count_trades, count_politicians, volume, date_last_traded)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(issuer_id) DO UPDATE SET
               count_trades = excluded.count_trades,
               count_politicians = excluded.count_politicians,
               volume = excluded.volume,
               date_last_traded = excluded.date_last_traded",
            params![
                issuer_id,
                detail.stats.count_trades,
                detail.stats.count_politicians,
                detail.stats.volume,
                detail.stats.date_last_traded,
            ],
        )?;

        // Step 3: Handle performance data
        if let Some(ref perf_value) = detail.performance {
            if let Some(perf_obj) = perf_value.as_object() {
                // Check all 20 required fields are present and non-null
                let required = [
                    "mcap",
                    "trailing1",
                    "trailing1Change",
                    "trailing7",
                    "trailing7Change",
                    "trailing30",
                    "trailing30Change",
                    "trailing90",
                    "trailing90Change",
                    "trailing365",
                    "trailing365Change",
                    "wtd",
                    "wtdChange",
                    "mtd",
                    "mtdChange",
                    "qtd",
                    "qtdChange",
                    "ytd",
                    "ytdChange",
                    "eodPrices",
                ];
                let all_present = required
                    .iter()
                    .all(|key| perf_obj.get(*key).map(|v| !v.is_null()).unwrap_or(false));

                if all_present {
                    // 3a: INSERT OR REPLACE issuer_performance
                    let mcap = perf_obj.get("mcap").and_then(|v| v.as_i64()).unwrap_or(0);
                    let trailing1 = perf_obj.get("trailing1").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing1_change = perf_obj.get("trailing1Change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing7 = perf_obj.get("trailing7").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing7_change = perf_obj.get("trailing7Change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing30 = perf_obj.get("trailing30").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing30_change = perf_obj.get("trailing30Change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing90 = perf_obj.get("trailing90").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing90_change = perf_obj.get("trailing90Change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing365 = perf_obj.get("trailing365").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let trailing365_change = perf_obj.get("trailing365Change").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let wtd = perf_obj.get("wtd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let wtd_change = perf_obj.get("wtdChange").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mtd = perf_obj.get("mtd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mtd_change = perf_obj.get("mtdChange").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let qtd = perf_obj.get("qtd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let qtd_change = perf_obj.get("qtdChange").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ytd = perf_obj.get("ytd").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let ytd_change = perf_obj.get("ytdChange").and_then(|v| v.as_f64()).unwrap_or(0.0);

                    tx.execute(
                        "INSERT INTO issuer_performance (
                           issuer_id, mcap,
                           trailing1, trailing1_change, trailing7, trailing7_change,
                           trailing30, trailing30_change, trailing90, trailing90_change,
                           trailing365, trailing365_change,
                           wtd, wtd_change, mtd, mtd_change,
                           qtd, qtd_change, ytd, ytd_change
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
                        params![
                            issuer_id, mcap,
                            trailing1, trailing1_change, trailing7, trailing7_change,
                            trailing30, trailing30_change, trailing90, trailing90_change,
                            trailing365, trailing365_change,
                            wtd, wtd_change, mtd, mtd_change,
                            qtd, qtd_change, ytd, ytd_change,
                        ],
                    )?;

                    // 3b: DELETE old EOD prices, then INSERT new ones
                    tx.execute(
                        "DELETE FROM issuer_eod_prices WHERE issuer_id = ?1",
                        params![issuer_id],
                    )?;

                    if let Some(eod_arr) = perf_obj.get("eodPrices").and_then(|v| v.as_array()) {
                        let mut stmt = tx.prepare(
                            "INSERT INTO issuer_eod_prices (issuer_id, price_date, price)
                             VALUES (?1, ?2, ?3)",
                        )?;
                        for entry in eod_arr {
                            if let Some(pair) = entry.as_array() {
                                // Each entry is [date_string_or_float, price_float]
                                let eod_values: Vec<DbEodValue> = pair
                                    .iter()
                                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                                    .collect();
                                if let Some((date, price)) = eod_pair(&eod_values) {
                                    stmt.execute(params![issuer_id, date, price])?;
                                }
                            }
                        }
                    }
                } else {
                    // Performance present but incomplete -- treat as no performance
                    tx.execute(
                        "DELETE FROM issuer_performance WHERE issuer_id = ?1",
                        params![issuer_id],
                    )?;
                    tx.execute(
                        "DELETE FROM issuer_eod_prices WHERE issuer_id = ?1",
                        params![issuer_id],
                    )?;
                }
            } else {
                // Performance is not an object -- treat as no performance
                tx.execute(
                    "DELETE FROM issuer_performance WHERE issuer_id = ?1",
                    params![issuer_id],
                )?;
                tx.execute(
                    "DELETE FROM issuer_eod_prices WHERE issuer_id = ?1",
                    params![issuer_id],
                )?;
            }
        } else {
            // Step 4: performance is None -- clean up stale data
            tx.execute(
                "DELETE FROM issuer_performance WHERE issuer_id = ?1",
                params![issuer_id],
            )?;
            tx.execute(
                "DELETE FROM issuer_eod_prices WHERE issuer_id = ?1",
                params![issuer_id],
            )?;
        }

        tx.commit()?;
        Ok(())
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

    /// Query politicians with JOINed stats and committee membership data.
    /// Supports filtering by party, state, name, and chamber.
    pub fn query_politicians(
        &self,
        filter: &DbPoliticianFilter,
    ) -> Result<Vec<DbPoliticianRow>, DbError> {
        let mut sql = String::from(
            "SELECT p.politician_id,
                    p.first_name || ' ' || p.last_name AS name,
                    p.party, p.state_id, p.chamber, p.gender, p.enriched_at,
                    COALESCE(ps.count_trades, 0) AS trades,
                    COALESCE(ps.count_issuers, 0) AS issuers,
                    COALESCE(ps.volume, 0) AS volume,
                    ps.date_last_traded,
                    COALESCE(GROUP_CONCAT(DISTINCT pc.committee), '') AS committees
             FROM politicians p
             LEFT JOIN politician_stats ps ON p.politician_id = ps.politician_id
             LEFT JOIN politician_committees pc ON p.politician_id = pc.politician_id
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
        if let Some(ref name) = filter.name {
            sql.push_str(&format!(
                " AND (p.first_name || ' ' || p.last_name) LIKE ?{}",
                param_idx
            ));
            params_vec.push(Box::new(format!("%{}%", name)));
            param_idx += 1;
        }
        if let Some(ref chamber) = filter.chamber {
            sql.push_str(&format!(" AND p.chamber = ?{}", param_idx));
            params_vec.push(Box::new(chamber.clone()));
            param_idx += 1;
        }

        sql.push_str(" GROUP BY p.politician_id ORDER BY COALESCE(ps.volume, 0) DESC");

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let _ = param_idx; // suppress unused warning

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let committees_str: String = row.get(11)?;

            Ok(DbPoliticianRow {
                politician_id: row.get(0)?,
                name: row.get(1)?,
                party: row.get(2)?,
                state: row.get(3)?,
                chamber: row.get(4)?,
                gender: row.get(5)?,
                enriched_at: row.get(6)?,
                trades: row.get(7)?,
                issuers: row.get(8)?,
                volume: row.get(9)?,
                last_traded: row.get(10)?,
                committees: if committees_str.is_empty() {
                    Vec::new()
                } else {
                    committees_str.split(',').map(|s| s.to_string()).collect()
                },
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Query issuers with LEFT JOINed stats and performance data.
    /// Supports filtering by search term, sector, state, and country.
    pub fn query_issuers(&self, filter: &DbIssuerFilter) -> Result<Vec<DbIssuerRow>, DbError> {
        let mut sql = String::from(
            "SELECT i.issuer_id, i.issuer_name, i.issuer_ticker, i.sector,
                    i.state_id, i.country, i.enriched_at,
                    COALESCE(s.count_trades, 0),
                    COALESCE(s.count_politicians, 0),
                    COALESCE(s.volume, 0),
                    s.date_last_traded,
                    p.mcap, p.trailing1, p.trailing1_change,
                    p.trailing7, p.trailing7_change,
                    p.trailing30, p.trailing30_change,
                    p.trailing90, p.trailing90_change,
                    p.trailing365, p.trailing365_change
             FROM issuers i
             LEFT JOIN issuer_stats s ON i.issuer_id = s.issuer_id
             LEFT JOIN issuer_performance p ON i.issuer_id = p.issuer_id
             WHERE 1=1",
        );

        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        if let Some(ref search) = filter.search {
            sql.push_str(&format!(
                " AND (i.issuer_name LIKE ?{n} OR i.issuer_ticker LIKE ?{n})",
                n = param_idx
            ));
            params_vec.push(Box::new(format!("%{}%", search)));
            param_idx += 1;
        }
        if let Some(ref sectors) = filter.sector {
            if !sectors.is_empty() {
                let placeholders: Vec<String> = sectors
                    .iter()
                    .map(|_| {
                        let ph = format!("?{}", param_idx);
                        param_idx += 1;
                        ph
                    })
                    .collect();
                sql.push_str(&format!(" AND i.sector IN ({})", placeholders.join(",")));
                for s in sectors {
                    params_vec.push(Box::new(s.clone()));
                }
            }
        }
        if let Some(ref states) = filter.state {
            if !states.is_empty() {
                let placeholders: Vec<String> = states
                    .iter()
                    .map(|_| {
                        let ph = format!("?{}", param_idx);
                        param_idx += 1;
                        ph
                    })
                    .collect();
                sql.push_str(&format!(" AND i.state_id IN ({})", placeholders.join(",")));
                for s in states {
                    params_vec.push(Box::new(s.clone()));
                }
            }
        }
        if let Some(ref countries) = filter.country {
            if !countries.is_empty() {
                let placeholders: Vec<String> = countries
                    .iter()
                    .map(|_| {
                        let ph = format!("?{}", param_idx);
                        param_idx += 1;
                        ph
                    })
                    .collect();
                sql.push_str(&format!(" AND i.country IN ({})", placeholders.join(",")));
                for c in countries {
                    params_vec.push(Box::new(c.clone()));
                }
            }
        }

        sql.push_str(" ORDER BY COALESCE(s.volume, 0) DESC");

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let _ = param_idx; // suppress unused warning

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(DbIssuerRow {
                issuer_id: row.get(0)?,
                issuer_name: row.get(1)?,
                issuer_ticker: row.get(2)?,
                sector: row.get(3)?,
                state: row.get(4)?,
                country: row.get(5)?,
                enriched_at: row.get(6)?,
                trades: row.get(7)?,
                politicians: row.get(8)?,
                volume: row.get(9)?,
                last_traded: row.get(10)?,
                mcap: row.get(11)?,
                trailing1: row.get(12)?,
                trailing1_change: row.get(13)?,
                trailing7: row.get(14)?,
                trailing7_change: row.get(15)?,
                trailing30: row.get(16)?,
                trailing30_change: row.get(17)?,
                trailing90: row.get(18)?,
                trailing90_change: row.get(19)?,
                trailing365: row.get(20)?,
                trailing365_change: row.get(21)?,
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

/// A fully-joined politician row returned by [`Db::query_politicians`].
///
/// Includes stats from politician_stats and committee memberships from
/// politician_committees via SQL JOINs and GROUP_CONCAT.
#[derive(Debug, Clone, Serialize)]
pub struct DbPoliticianRow {
    pub politician_id: String,
    pub name: String,
    pub party: String,
    pub state: String,
    pub chamber: String,
    pub gender: String,
    pub committees: Vec<String>,
    pub trades: i64,
    pub issuers: i64,
    pub volume: i64,
    pub last_traded: Option<String>,
    pub enriched_at: Option<String>,
}

/// Filter parameters for [`Db::query_politicians`].
#[derive(Debug, Default)]
pub struct DbPoliticianFilter {
    pub party: Option<String>,
    pub state: Option<String>,
    pub name: Option<String>,
    pub chamber: Option<String>,
    pub limit: Option<i64>,
}

/// A fully-joined issuer row returned by [`Db::query_issuers`].
///
/// Includes stats from issuer_stats and performance data from
/// issuer_performance via SQL LEFT JOINs.
#[derive(Debug, Clone, Serialize)]
pub struct DbIssuerRow {
    pub issuer_id: i64,
    pub issuer_name: String,
    pub issuer_ticker: Option<String>,
    pub sector: Option<String>,
    pub state: Option<String>,
    pub country: Option<String>,
    pub trades: i64,
    pub politicians: i64,
    pub volume: i64,
    pub last_traded: Option<String>,
    pub mcap: Option<i64>,
    pub trailing1: Option<f64>,
    pub trailing1_change: Option<f64>,
    pub trailing7: Option<f64>,
    pub trailing7_change: Option<f64>,
    pub trailing30: Option<f64>,
    pub trailing30_change: Option<f64>,
    pub trailing90: Option<f64>,
    pub trailing90_change: Option<f64>,
    pub trailing365: Option<f64>,
    pub trailing365_change: Option<f64>,
    pub enriched_at: Option<String>,
}

/// Filter parameters for [`Db::query_issuers`].
#[derive(Debug, Default)]
pub struct DbIssuerFilter {
    pub search: Option<String>,
    pub sector: Option<Vec<String>>,
    pub state: Option<Vec<String>>,
    pub country: Option<Vec<String>>,
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

    // --- query_trades tests ---

    /// Set up a test database with 3 trades for query_trades testing.
    ///
    /// Trade 100: John Smith (Democrat, CA, senate), Apple Inc (AAPL), buy, 2024-01-15, value 50000
    /// Trade 200: Jane Doe (Republican, TX, house), Microsoft Corp (MSFT), sell, 2024-02-20, value 100000
    /// Trade 300: John Smith (Democrat, CA, senate), Tesla Inc (TSLA), buy, 2024-03-10, value 25000
    ///
    /// Trade 100 is enriched with asset_type="stock", committees=["ssfi"], labels=["faang"].
    fn setup_test_db_with_trades() -> Db {
        use crate::scrape::{ScrapedIssuer, ScrapedPolitician};

        let mut db = open_test_db();

        let trade100 = ScrapedTrade {
            tx_id: 100,
            politician_id: "P000001".to_string(),
            issuer_id: 1,
            chamber: "senate".to_string(),
            comment: None,
            issuer: ScrapedIssuer {
                state_id: Some("CA".to_string()),
                c2iq: None,
                country: Some("US".to_string()),
                issuer_name: "Apple Inc".to_string(),
                issuer_ticker: Some("AAPL".to_string()),
                sector: Some("technology".to_string()),
            },
            owner: "self".to_string(),
            politician: ScrapedPolitician {
                state_id: "CA".to_string(),
                chamber: "senate".to_string(),
                dob: "1960-05-10".to_string(),
                first_name: "John".to_string(),
                gender: "male".to_string(),
                last_name: "Smith".to_string(),
                nickname: None,
                party: "Democrat".to_string(),
            },
            price: None,
            pub_date: "2024-01-15T00:00:00Z".to_string(),
            reporting_gap: 5,
            tx_date: "2024-01-10".to_string(),
            tx_type: "buy".to_string(),
            tx_type_extended: None,
            value: 50000,
            filing_url: Some("https://example.com/100".to_string()),
            filing_id: Some(100),
        };

        let trade200 = ScrapedTrade {
            tx_id: 200,
            politician_id: "P000002".to_string(),
            issuer_id: 2,
            chamber: "house".to_string(),
            comment: None,
            issuer: ScrapedIssuer {
                state_id: Some("WA".to_string()),
                c2iq: None,
                country: Some("US".to_string()),
                issuer_name: "Microsoft Corp".to_string(),
                issuer_ticker: Some("MSFT".to_string()),
                sector: Some("technology".to_string()),
            },
            owner: "self".to_string(),
            politician: ScrapedPolitician {
                state_id: "TX".to_string(),
                chamber: "house".to_string(),
                dob: "1975-03-22".to_string(),
                first_name: "Jane".to_string(),
                gender: "female".to_string(),
                last_name: "Doe".to_string(),
                nickname: None,
                party: "Republican".to_string(),
            },
            price: None,
            pub_date: "2024-02-20T00:00:00Z".to_string(),
            reporting_gap: 10,
            tx_date: "2024-02-10".to_string(),
            tx_type: "sell".to_string(),
            tx_type_extended: None,
            value: 100000,
            filing_url: Some("https://example.com/200".to_string()),
            filing_id: Some(200),
        };

        let trade300 = ScrapedTrade {
            tx_id: 300,
            politician_id: "P000001".to_string(),
            issuer_id: 3,
            chamber: "senate".to_string(),
            comment: None,
            issuer: ScrapedIssuer {
                state_id: Some("CA".to_string()),
                c2iq: None,
                country: Some("US".to_string()),
                issuer_name: "Tesla Inc".to_string(),
                issuer_ticker: Some("TSLA".to_string()),
                sector: Some("consumer-discretionary".to_string()),
            },
            owner: "self".to_string(),
            politician: ScrapedPolitician {
                state_id: "CA".to_string(),
                chamber: "senate".to_string(),
                dob: "1960-05-10".to_string(),
                first_name: "John".to_string(),
                gender: "male".to_string(),
                last_name: "Smith".to_string(),
                nickname: None,
                party: "Democrat".to_string(),
            },
            price: None,
            pub_date: "2024-03-10T00:00:00Z".to_string(),
            reporting_gap: 3,
            tx_date: "2024-03-07".to_string(),
            tx_type: "buy".to_string(),
            tx_type_extended: None,
            value: 25000,
            filing_url: Some("https://example.com/300".to_string()),
            filing_id: Some(300),
        };

        db.upsert_scraped_trades(&[trade100, trade200, trade300])
            .expect("upsert test trades");

        // Enrich trade 100 with asset_type, committees, labels
        let detail = ScrapedTradeDetail {
            asset_type: Some("stock".to_string()),
            committees: vec!["ssfi".to_string()],
            labels: vec!["faang".to_string()],
            ..ScrapedTradeDetail::default()
        };
        db.update_trade_detail(100, &detail)
            .expect("enrich trade 100");

        db
    }

    #[test]
    fn test_query_trades_no_filter() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter::default())
            .expect("query_trades");
        assert_eq!(rows.len(), 3, "should return all 3 trades");
        // Ordering: pub_date DESC -> trade 300 (2024-03-10), 200 (2024-02-20), 100 (2024-01-15)
        assert_eq!(rows[0].tx_id, 300);
        assert_eq!(rows[1].tx_id, 200);
        assert_eq!(rows[2].tx_id, 100);
    }

    #[test]
    fn test_query_trades_filter_party() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                party: Some("Democrat".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 2, "should return 2 Democrat trades");
        // Both should be John Smith
        for row in &rows {
            assert_eq!(row.party, "Democrat");
        }
    }

    #[test]
    fn test_query_trades_filter_state() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                state: Some("TX".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 1, "should return 1 TX trade");
        assert_eq!(rows[0].tx_id, 200);
        assert_eq!(rows[0].state, "TX");
    }

    #[test]
    fn test_query_trades_filter_tx_type() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                tx_type: Some("sell".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 1, "should return 1 sell trade");
        assert_eq!(rows[0].tx_id, 200);
    }

    #[test]
    fn test_query_trades_filter_name() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                name: Some("john".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 2, "should return 2 trades for John (case-insensitive LIKE)");
        for row in &rows {
            assert_eq!(row.politician_name, "John Smith");
        }
    }

    #[test]
    fn test_query_trades_filter_issuer() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                issuer: Some("AAPL".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 1, "should return 1 trade matching AAPL ticker");
        assert_eq!(rows[0].tx_id, 100);
        assert_eq!(rows[0].issuer_ticker, "AAPL");
    }

    #[test]
    fn test_query_trades_filter_date_range() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                since: Some("2024-02-01".to_string()),
                until: Some("2024-02-28".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 1, "should return 1 trade in Feb 2024");
        assert_eq!(rows[0].tx_id, 200);
    }

    #[test]
    fn test_query_trades_limit() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                limit: Some(2),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 2, "should return exactly 2 rows");
    }

    #[test]
    fn test_query_trades_enriched_fields() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter::default())
            .expect("query_trades");

        // Find each trade by tx_id
        let trade100 = rows.iter().find(|r| r.tx_id == 100).expect("trade 100");
        let trade200 = rows.iter().find(|r| r.tx_id == 200).expect("trade 200");
        let trade300 = rows.iter().find(|r| r.tx_id == 300).expect("trade 300");

        // Trade 100 is enriched
        assert_eq!(trade100.asset_type, "stock", "enriched trade should have asset_type=stock");
        assert_eq!(trade100.committees, vec!["ssfi"], "enriched trade should have committees");
        assert_eq!(trade100.labels, vec!["faang"], "enriched trade should have labels");
        assert!(trade100.enriched_at.is_some(), "enriched trade should have enriched_at timestamp");

        // Trade 200 is NOT enriched
        assert_eq!(trade200.asset_type, "unknown", "unenriched trade should have asset_type=unknown");
        assert!(trade200.committees.is_empty(), "unenriched trade should have empty committees");
        assert!(trade200.labels.is_empty(), "unenriched trade should have empty labels");

        // Trade 300 is NOT enriched
        assert_eq!(trade300.asset_type, "unknown", "unenriched trade should have asset_type=unknown");
        assert!(trade300.committees.is_empty(), "unenriched trade should have empty committees");
        assert!(trade300.labels.is_empty(), "unenriched trade should have empty labels");
    }

    #[test]
    fn test_query_trades_combined_filters() {
        let db = setup_test_db_with_trades();
        let rows = db
            .query_trades(&DbTradeFilter {
                party: Some("Democrat".to_string()),
                tx_type: Some("buy".to_string()),
                ..DbTradeFilter::default()
            })
            .expect("query_trades");
        assert_eq!(rows.len(), 2, "should return 2 Democrat buy trades");
        for row in &rows {
            assert_eq!(row.party, "Democrat");
            assert_eq!(row.tx_type, "buy");
        }
    }

    // ---- Politician committee persistence tests ----

    fn insert_test_politician(db: &Db, id: &str, first_name: &str) {
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES (?1, 'CA', 'Democrat', ?2, 'Test', '1970-01-01', 'female', 'senate')",
                params![id, first_name],
            )
            .expect("insert test politician");
    }

    #[test]
    fn test_replace_all_politician_committees_basic() {
        let db = open_test_db();
        insert_test_politician(&db, "P000001", "Alice");
        insert_test_politician(&db, "P000002", "Bob");

        // 3 memberships: 2 for known politicians, 1 for unknown
        let memberships = vec![
            ("P000001".to_string(), "ssfi".to_string()),
            ("P000002".to_string(), "ssfi".to_string()),
            ("P999999".to_string(), "ssfi".to_string()), // unknown politician
        ];
        let inserted = db
            .replace_all_politician_committees(&memberships)
            .expect("replace_all_politician_committees");
        assert_eq!(inserted, 2, "should insert 2, skip unknown P999999");

        // Verify the rows exist
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM politician_committees",
                [],
                |row| row.get(0),
            )
            .expect("count committees");
        assert_eq!(count, 2);
    }

    #[test]
    fn test_replace_all_politician_committees_replaces() {
        let db = open_test_db();
        insert_test_politician(&db, "P000001", "Alice");
        insert_test_politician(&db, "P000002", "Bob");

        // First call: both on ssfi
        let memberships1 = vec![
            ("P000001".to_string(), "ssfi".to_string()),
            ("P000002".to_string(), "ssfi".to_string()),
        ];
        db.replace_all_politician_committees(&memberships1)
            .expect("first replace");

        // Second call: only P000001 on hsag, P000002 not on any committee
        let memberships2 = vec![("P000001".to_string(), "hsag".to_string())];
        let inserted = db
            .replace_all_politician_committees(&memberships2)
            .expect("second replace");
        assert_eq!(inserted, 1, "should insert 1 after replacing all");

        // Verify old data is gone
        let committees: Vec<String> = {
            let mut stmt = db
                .conn
                .prepare("SELECT committee FROM politician_committees ORDER BY committee")
                .expect("prepare");
            stmt.query_map([], |row| row.get(0))
                .expect("query")
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(committees, vec!["hsag"], "only hsag should remain");
    }

    #[test]
    fn test_replace_all_politician_committees_empty() {
        let db = open_test_db();
        insert_test_politician(&db, "P000001", "Alice");

        // First populate some data
        let memberships = vec![("P000001".to_string(), "ssfi".to_string())];
        db.replace_all_politician_committees(&memberships)
            .expect("populate");

        // Now call with empty slice -- should clear all
        let inserted = db
            .replace_all_politician_committees(&[])
            .expect("empty replace");
        assert_eq!(inserted, 0);

        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM politician_committees",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 0, "table should be empty after replace with empty slice");
    }

    #[test]
    fn test_mark_politicians_enriched() {
        let db = open_test_db();
        insert_test_politician(&db, "P000001", "Alice");

        // Verify enriched_at starts NULL
        let before: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM politicians WHERE politician_id = 'P000001'",
                [],
                |row| row.get(0),
            )
            .expect("query before");
        assert!(before.is_none(), "enriched_at should start as NULL");

        db.mark_politicians_enriched()
            .expect("mark_politicians_enriched");

        let after: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM politicians WHERE politician_id = 'P000001'",
                [],
                |row| row.get(0),
            )
            .expect("query after");
        assert!(after.is_some(), "enriched_at should be set after marking");
    }

    #[test]
    fn test_count_unenriched_politicians() {
        let db = open_test_db();
        insert_test_politician(&db, "P000001", "Alice");
        insert_test_politician(&db, "P000002", "Bob");

        let count = db
            .count_unenriched_politicians()
            .expect("count_unenriched_politicians");
        assert_eq!(count, 2, "both politicians should be unenriched");

        // Mark one as enriched
        db.conn
            .execute(
                "UPDATE politicians SET enriched_at = datetime('now') WHERE politician_id = 'P000001'",
                [],
            )
            .expect("manual enrich");

        let count = db
            .count_unenriched_politicians()
            .expect("count_unenriched_politicians after one enriched");
        assert_eq!(count, 1, "only one should remain unenriched");
    }

    // --- query_politicians tests ---

    fn insert_test_politician_full(
        db: &Db,
        id: &str,
        first_name: &str,
        last_name: &str,
        party: &str,
        state: &str,
        chamber: &str,
    ) {
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES (?1, ?2, ?3, ?4, ?5, '1970-01-01', 'female', ?6)",
                params![id, state, party, first_name, last_name, chamber],
            )
            .expect("insert test politician");
    }

    fn insert_test_politician_stats(
        db: &Db,
        id: &str,
        trades: i64,
        issuers: i64,
        volume: i64,
        last_traded: Option<&str>,
    ) {
        db.conn
            .execute(
                "INSERT INTO politician_stats (politician_id, count_trades, count_issuers, volume, date_last_traded)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, trades, issuers, volume, last_traded],
            )
            .expect("insert test politician stats");
    }

    #[test]
    fn test_query_politicians_no_filter() {
        let db = open_test_db();
        insert_test_politician_full(&db, "P000001", "John", "Smith", "Democrat", "CA", "senate");
        insert_test_politician_full(&db, "P000002", "Jane", "Doe", "Republican", "TX", "house");
        insert_test_politician_stats(&db, "P000001", 10, 5, 100000, Some("2024-03-10"));
        insert_test_politician_stats(&db, "P000002", 20, 8, 200000, Some("2024-04-15"));

        let rows = db
            .query_politicians(&DbPoliticianFilter::default())
            .expect("query_politicians");
        assert_eq!(rows.len(), 2, "should return all 2 politicians");
        // Ordered by volume DESC: P000002 (200000) first, P000001 (100000) second
        assert_eq!(rows[0].politician_id, "P000002");
        assert_eq!(rows[0].name, "Jane Doe");
        assert_eq!(rows[0].volume, 200000);
        assert_eq!(rows[1].politician_id, "P000001");
        assert_eq!(rows[1].name, "John Smith");
        assert_eq!(rows[1].volume, 100000);
    }

    #[test]
    fn test_query_politicians_party_filter() {
        let db = open_test_db();
        insert_test_politician_full(&db, "P000001", "John", "Smith", "Democrat", "CA", "senate");
        insert_test_politician_full(&db, "P000002", "Jane", "Doe", "Republican", "TX", "house");
        insert_test_politician_stats(&db, "P000001", 10, 5, 100000, Some("2024-03-10"));
        insert_test_politician_stats(&db, "P000002", 20, 8, 200000, Some("2024-04-15"));

        let rows = db
            .query_politicians(&DbPoliticianFilter {
                party: Some("Democrat".to_string()),
                ..DbPoliticianFilter::default()
            })
            .expect("query_politicians");
        assert_eq!(rows.len(), 1, "should return 1 Democrat");
        assert_eq!(rows[0].politician_id, "P000001");
        assert_eq!(rows[0].party, "Democrat");
    }

    #[test]
    fn test_query_politicians_name_filter() {
        let db = open_test_db();
        insert_test_politician_full(&db, "P000001", "John", "Smith", "Democrat", "CA", "senate");
        insert_test_politician_full(&db, "P000002", "Jane", "Doe", "Republican", "TX", "house");
        insert_test_politician_stats(&db, "P000001", 10, 5, 100000, None);
        insert_test_politician_stats(&db, "P000002", 20, 8, 200000, None);

        let rows = db
            .query_politicians(&DbPoliticianFilter {
                name: Some("john".to_string()),
                ..DbPoliticianFilter::default()
            })
            .expect("query_politicians");
        assert_eq!(rows.len(), 1, "should return 1 matching 'john' (case-insensitive LIKE)");
        assert_eq!(rows[0].name, "John Smith");
    }

    #[test]
    fn test_query_politicians_with_committees() {
        let db = open_test_db();
        insert_test_politician_full(&db, "P000001", "John", "Smith", "Democrat", "CA", "senate");
        insert_test_politician_stats(&db, "P000001", 10, 5, 100000, Some("2024-03-10"));

        // Add committee memberships
        db.replace_all_politician_committees(&[
            ("P000001".to_string(), "ssfi".to_string()),
            ("P000001".to_string(), "hsag".to_string()),
        ])
        .expect("replace committees");

        let rows = db
            .query_politicians(&DbPoliticianFilter::default())
            .expect("query_politicians");
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].committees.is_empty(), "committees should be populated");
        // GROUP_CONCAT with DISTINCT - order may vary, check both present
        assert!(
            rows[0].committees.contains(&"ssfi".to_string()),
            "should contain ssfi"
        );
        assert!(
            rows[0].committees.contains(&"hsag".to_string()),
            "should contain hsag"
        );
    }

    #[test]
    fn test_query_politicians_limit() {
        let db = open_test_db();
        insert_test_politician_full(&db, "P000001", "Alice", "A", "Democrat", "CA", "senate");
        insert_test_politician_full(&db, "P000002", "Bob", "B", "Republican", "TX", "house");
        insert_test_politician_full(&db, "P000003", "Carol", "C", "Democrat", "NY", "senate");
        insert_test_politician_stats(&db, "P000001", 10, 5, 300000, None);
        insert_test_politician_stats(&db, "P000002", 20, 8, 200000, None);
        insert_test_politician_stats(&db, "P000003", 5, 3, 100000, None);

        let rows = db
            .query_politicians(&DbPoliticianFilter {
                limit: Some(2),
                ..DbPoliticianFilter::default()
            })
            .expect("query_politicians");
        assert_eq!(rows.len(), 2, "should return exactly 2 with limit=2");
        // Ordered by volume DESC: P000001 (300K), P000002 (200K)
        assert_eq!(rows[0].politician_id, "P000001");
        assert_eq!(rows[1].politician_id, "P000002");
    }

    // --- update_issuer_detail tests ---

    fn make_test_scraped_issuer_detail(
        issuer_id: i64,
        name: &str,
        perf: Option<serde_json::Value>,
    ) -> crate::scrape::ScrapedIssuerDetail {
        crate::scrape::ScrapedIssuerDetail {
            issuer_id,
            state_id: Some("ca".to_string()),
            c2iq: Some("AAPL:US".to_string()),
            country: Some("us".to_string()),
            issuer_name: name.to_string(),
            issuer_ticker: Some("AAPL".to_string()),
            performance: perf,
            sector: Some("information-technology".to_string()),
            stats: crate::scrape::ScrapedIssuerStats {
                count_trades: 100,
                count_politicians: 20,
                volume: 5000000,
                date_last_traded: "2026-01-10".to_string(),
            },
        }
    }

    fn make_test_performance_json() -> serde_json::Value {
        serde_json::json!({
            "mcap": 3500000000000_i64,
            "trailing1": 225.5,
            "trailing1Change": 0.0089,
            "trailing7": 224.0,
            "trailing7Change": 0.0156,
            "trailing30": 220.0,
            "trailing30Change": 0.025,
            "trailing90": 210.0,
            "trailing90Change": 0.0738,
            "trailing365": 180.0,
            "trailing365Change": 0.2528,
            "wtd": 224.5,
            "wtdChange": 0.0133,
            "mtd": 222.0,
            "mtdChange": 0.0203,
            "qtd": 218.0,
            "qtdChange": 0.0344,
            "ytd": 215.0,
            "ytdChange": 0.0488,
            "eodPrices": [
                ["2026-01-15", 225.5],
                ["2026-01-16", 227.3],
                ["2026-01-17", 228.1]
            ]
        })
    }

    fn insert_bare_issuer(db: &Db, issuer_id: i64, name: &str) {
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name) VALUES (?1, ?2)",
                params![issuer_id, name],
            )
            .expect("insert bare issuer");
    }

    #[test]
    fn test_update_issuer_detail_with_performance() {
        let db = open_test_db();
        insert_bare_issuer(&db, 12345, "Apple Inc.");

        let detail = make_test_scraped_issuer_detail(12345, "Apple Inc.", Some(make_test_performance_json()));
        db.update_issuer_detail(12345, &detail)
            .expect("update_issuer_detail");

        // Verify enriched_at is set
        let enriched: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM issuers WHERE issuer_id = 12345",
                [],
                |row| row.get(0),
            )
            .expect("query enriched_at");
        assert!(enriched.is_some(), "enriched_at should be set after update");

        // Verify issuer_performance row exists with correct mcap
        let mcap: i64 = db
            .conn
            .query_row(
                "SELECT mcap FROM issuer_performance WHERE issuer_id = 12345",
                [],
                |row| row.get(0),
            )
            .expect("query mcap");
        assert_eq!(mcap, 3500000000000_i64, "mcap should be 3.5T");

        // Verify trailing values
        let (t1, t1c): (f64, f64) = db
            .conn
            .query_row(
                "SELECT trailing1, trailing1_change FROM issuer_performance WHERE issuer_id = 12345",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query trailing");
        assert!((t1 - 225.5).abs() < f64::EPSILON);
        assert!((t1c - 0.0089).abs() < f64::EPSILON);

        // Verify EOD prices
        let eod_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issuer_eod_prices WHERE issuer_id = 12345",
                [],
                |row| row.get(0),
            )
            .expect("count eod prices");
        assert_eq!(eod_count, 3, "should have 3 EOD price entries");

        // Verify first EOD price
        let (date, price): (String, f64) = db
            .conn
            .query_row(
                "SELECT price_date, price FROM issuer_eod_prices WHERE issuer_id = 12345 ORDER BY price_date LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query first eod price");
        assert_eq!(date, "2026-01-15");
        assert!((price - 225.5).abs() < f64::EPSILON);

        // Verify issuer_stats
        let (ct, cp, vol): (i64, i64, i64) = db
            .conn
            .query_row(
                "SELECT count_trades, count_politicians, volume FROM issuer_stats WHERE issuer_id = 12345",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query issuer_stats");
        assert_eq!(ct, 100);
        assert_eq!(cp, 20);
        assert_eq!(vol, 5000000);
    }

    #[test]
    fn test_update_issuer_detail_no_performance() {
        let db = open_test_db();
        insert_bare_issuer(&db, 99999, "PrivateCo Holdings");

        let detail = make_test_scraped_issuer_detail(99999, "PrivateCo Holdings", None);
        db.update_issuer_detail(99999, &detail)
            .expect("update_issuer_detail");

        // Verify enriched_at is still set (even with no performance)
        let enriched: Option<String> = db
            .conn
            .query_row(
                "SELECT enriched_at FROM issuers WHERE issuer_id = 99999",
                [],
                |row| row.get(0),
            )
            .expect("query enriched_at");
        assert!(enriched.is_some(), "enriched_at should be set even with no performance");

        // Verify no issuer_performance row
        let perf_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issuer_performance WHERE issuer_id = 99999",
                [],
                |row| row.get(0),
            )
            .expect("count perf");
        assert_eq!(perf_count, 0, "should have no performance row");

        // Verify no EOD prices
        let eod_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issuer_eod_prices WHERE issuer_id = 99999",
                [],
                |row| row.get(0),
            )
            .expect("count eod");
        assert_eq!(eod_count, 0, "should have no EOD prices");
    }

    #[test]
    fn test_update_issuer_detail_preserves_existing_fields() {
        let db = open_test_db();
        // Insert issuer with sector "financials"
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, sector) VALUES (55555, 'TestCorp', 'financials')",
                [],
            )
            .expect("insert issuer with sector");

        // Update with sector: None -- COALESCE should preserve existing
        let detail = crate::scrape::ScrapedIssuerDetail {
            issuer_id: 55555,
            state_id: None,
            c2iq: None,
            country: None,
            issuer_name: "TestCorp".to_string(),
            issuer_ticker: None,
            performance: None,
            sector: None,
            stats: crate::scrape::ScrapedIssuerStats {
                count_trades: 10,
                count_politicians: 3,
                volume: 50000,
                date_last_traded: "2025-01-01".to_string(),
            },
        };
        db.update_issuer_detail(55555, &detail)
            .expect("update_issuer_detail");

        let sector: Option<String> = db
            .conn
            .query_row(
                "SELECT sector FROM issuers WHERE issuer_id = 55555",
                [],
                |row| row.get(0),
            )
            .expect("query sector");
        assert_eq!(
            sector.as_deref(),
            Some("financials"),
            "sector should be preserved via COALESCE when incoming is NULL"
        );
    }

    #[test]
    fn test_count_unenriched_issuers() {
        let db = open_test_db();
        // Insert 3 issuers
        insert_bare_issuer(&db, 1, "Corp A");
        insert_bare_issuer(&db, 2, "Corp B");
        insert_bare_issuer(&db, 3, "Corp C");

        let count = db.count_unenriched_issuers().expect("count_unenriched_issuers");
        assert_eq!(count, 3, "all 3 should be unenriched initially");

        // Enrich one
        db.conn
            .execute(
                "UPDATE issuers SET enriched_at = datetime('now') WHERE issuer_id = 2",
                [],
            )
            .expect("enrich issuer 2");

        let count = db.count_unenriched_issuers().expect("count after enrichment");
        assert_eq!(count, 2, "should return 2 after enriching 1");
    }

    #[test]
    fn test_update_issuer_detail_replaces_eod_prices() {
        let db = open_test_db();
        insert_bare_issuer(&db, 77777, "ReplaceCorp");

        // First enrichment: 3 EOD prices
        let detail1 = make_test_scraped_issuer_detail(77777, "ReplaceCorp", Some(make_test_performance_json()));
        db.update_issuer_detail(77777, &detail1)
            .expect("first update");

        let eod_count1: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issuer_eod_prices WHERE issuer_id = 77777",
                [],
                |row| row.get(0),
            )
            .expect("count eod 1");
        assert_eq!(eod_count1, 3, "should have 3 EOD prices after first enrichment");

        // Second enrichment: 2 different EOD prices
        let perf2 = serde_json::json!({
            "mcap": 3500000000000_i64,
            "trailing1": 225.5,
            "trailing1Change": 0.0089,
            "trailing7": 224.0,
            "trailing7Change": 0.0156,
            "trailing30": 220.0,
            "trailing30Change": 0.025,
            "trailing90": 210.0,
            "trailing90Change": 0.0738,
            "trailing365": 180.0,
            "trailing365Change": 0.2528,
            "wtd": 224.5,
            "wtdChange": 0.0133,
            "mtd": 222.0,
            "mtdChange": 0.0203,
            "qtd": 218.0,
            "qtdChange": 0.0344,
            "ytd": 215.0,
            "ytdChange": 0.0488,
            "eodPrices": [
                ["2026-02-01", 230.0],
                ["2026-02-02", 231.5]
            ]
        });
        let detail2 = make_test_scraped_issuer_detail(77777, "ReplaceCorp", Some(perf2));
        db.update_issuer_detail(77777, &detail2)
            .expect("second update");

        let eod_count2: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM issuer_eod_prices WHERE issuer_id = 77777",
                [],
                |row| row.get(0),
            )
            .expect("count eod 2");
        assert_eq!(
            eod_count2, 2,
            "should have 2 EOD prices after second enrichment (old ones deleted)"
        );

        // Verify the new dates are present
        let dates: Vec<String> = {
            let mut stmt = db
                .conn
                .prepare("SELECT price_date FROM issuer_eod_prices WHERE issuer_id = 77777 ORDER BY price_date")
                .expect("prepare");
            stmt.query_map([], |row| row.get(0))
                .expect("query")
                .filter_map(|r| r.ok())
                .collect()
        };
        assert_eq!(dates, vec!["2026-02-01", "2026-02-02"]);
    }

    // --- query_issuers tests ---

    fn insert_test_issuer(db: &Db, issuer_id: i64, name: &str, ticker: Option<&str>, sector: Option<&str>, state: Option<&str>, country: Option<&str>) {
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker, sector, state_id, country)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![issuer_id, name, ticker, sector, state, country],
            )
            .expect("insert test issuer");
    }

    fn insert_test_issuer_stats(db: &Db, issuer_id: i64, trades: i64, politicians: i64, volume: i64, last_traded: &str) {
        db.conn
            .execute(
                "INSERT INTO issuer_stats (issuer_id, count_trades, count_politicians, volume, date_last_traded)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![issuer_id, trades, politicians, volume, last_traded],
            )
            .expect("insert test issuer stats");
    }

    fn insert_test_issuer_performance(db: &Db, issuer_id: i64, mcap: i64) {
        db.conn
            .execute(
                "INSERT INTO issuer_performance (issuer_id, mcap, trailing1, trailing1_change, trailing7, trailing7_change, trailing30, trailing30_change, trailing90, trailing90_change, trailing365, trailing365_change, wtd, wtd_change, mtd, mtd_change, qtd, qtd_change, ytd, ytd_change)
                 VALUES (?1, ?2, 225.5, 0.0089, 224.0, 0.0156, 220.0, 0.025, 210.0, 0.0738, 180.0, 0.2528, 224.5, 0.0133, 222.0, 0.0203, 218.0, 0.0344, 215.0, 0.0488)",
                params![issuer_id, mcap],
            )
            .expect("insert test issuer performance");
    }

    #[test]
    fn test_query_issuers_no_filter() {
        let db = open_test_db();
        // Issuer 1: enriched with performance
        insert_test_issuer(&db, 100, "Apple Inc", Some("AAPL"), Some("information-technology"), Some("ca"), Some("us"));
        insert_test_issuer_stats(&db, 100, 500, 85, 50_000_000, "2024-03-14");
        insert_test_issuer_performance(&db, 100, 3_500_000_000_000);

        // Issuer 2: no performance data
        insert_test_issuer(&db, 200, "Unknown Corp", None, None, None, None);
        insert_test_issuer_stats(&db, 200, 10, 3, 1_000, "2024-01-01");

        let rows = db
            .query_issuers(&DbIssuerFilter::default())
            .expect("query_issuers");
        assert_eq!(rows.len(), 2, "should return all 2 issuers");
        // Ordered by volume DESC: 100 (50M) first, 200 (1K) second
        assert_eq!(rows[0].issuer_id, 100);
        assert_eq!(rows[0].issuer_name, "Apple Inc");
        assert_eq!(rows[0].volume, 50_000_000);
        assert!(rows[0].mcap.is_some(), "enriched issuer should have mcap");
        assert_eq!(rows[0].mcap.unwrap(), 3_500_000_000_000);
        assert!(rows[0].trailing30_change.is_some(), "enriched issuer should have trailing30_change");

        assert_eq!(rows[1].issuer_id, 200);
        assert_eq!(rows[1].issuer_name, "Unknown Corp");
        assert_eq!(rows[1].volume, 1_000);
        assert!(rows[1].mcap.is_none(), "unenriched issuer should have None mcap");
        assert!(rows[1].trailing30_change.is_none(), "unenriched issuer should have None trailing30_change");
    }

    #[test]
    fn test_query_issuers_search_filter() {
        let db = open_test_db();
        insert_test_issuer(&db, 100, "Apple Inc", Some("AAPL"), Some("information-technology"), None, None);
        insert_test_issuer(&db, 200, "Microsoft Corp", Some("MSFT"), Some("information-technology"), None, None);
        insert_test_issuer(&db, 300, "Exxon Mobil", Some("XOM"), Some("energy"), None, None);

        let rows = db
            .query_issuers(&DbIssuerFilter {
                search: Some("Apple".to_string()),
                ..DbIssuerFilter::default()
            })
            .expect("query_issuers");
        assert_eq!(rows.len(), 1, "should return only Apple");
        assert_eq!(rows[0].issuer_name, "Apple Inc");
    }

    #[test]
    fn test_query_issuers_sector_filter() {
        let db = open_test_db();
        insert_test_issuer(&db, 100, "Apple Inc", Some("AAPL"), Some("information-technology"), None, None);
        insert_test_issuer(&db, 200, "Microsoft Corp", Some("MSFT"), Some("information-technology"), None, None);
        insert_test_issuer(&db, 300, "Exxon Mobil", Some("XOM"), Some("energy"), None, None);

        let rows = db
            .query_issuers(&DbIssuerFilter {
                sector: Some(vec!["energy".to_string()]),
                ..DbIssuerFilter::default()
            })
            .expect("query_issuers");
        assert_eq!(rows.len(), 1, "should return only energy sector");
        assert_eq!(rows[0].issuer_name, "Exxon Mobil");
    }

    #[test]
    fn test_query_issuers_state_filter() {
        let db = open_test_db();
        insert_test_issuer(&db, 100, "Apple Inc", Some("AAPL"), Some("information-technology"), Some("ca"), None);
        insert_test_issuer(&db, 200, "ExxonMobil", Some("XOM"), Some("energy"), Some("tx"), None);
        insert_test_issuer(&db, 300, "Microsoft Corp", Some("MSFT"), Some("information-technology"), Some("wa"), None);

        let rows = db
            .query_issuers(&DbIssuerFilter {
                state: Some(vec!["ca".to_string()]),
                ..DbIssuerFilter::default()
            })
            .expect("query_issuers");
        assert_eq!(rows.len(), 1, "should return only CA issuers");
        assert_eq!(rows[0].issuer_name, "Apple Inc");
    }

    #[test]
    fn test_query_issuers_limit() {
        let db = open_test_db();
        for i in 1..=5 {
            insert_test_issuer(&db, i, &format!("Company {}", i), None, None, None, None);
            insert_test_issuer_stats(&db, i, 10, 5, (100 * i) as i64, "2024-01-01");
        }

        let rows = db
            .query_issuers(&DbIssuerFilter {
                limit: Some(2),
                ..DbIssuerFilter::default()
            })
            .expect("query_issuers");
        assert_eq!(rows.len(), 2, "should return exactly 2 with limit=2");
        // Ordered by volume DESC: Company 5 (500) first, Company 4 (400) second
        assert_eq!(rows[0].issuer_id, 5);
        assert_eq!(rows[1].issuer_id, 4);
    }
}
