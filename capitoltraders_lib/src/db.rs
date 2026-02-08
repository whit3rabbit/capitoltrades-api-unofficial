//! SQLite storage for Capitol Traders data.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use crate::scrape::ScrapedTrade;
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
               chamber = excluded.chamber",
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
               sector = excluded.sector",
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
}
