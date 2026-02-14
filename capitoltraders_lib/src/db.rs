//! SQLite storage for Capitol Traders data.

use std::path::Path;

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::portfolio::TradeFIFO;
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

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(Self { conn })
    }

    /// Get a reference to the underlying connection (for internal use by committee resolver and tests).
    #[doc(hidden)]
    pub fn conn(&self) -> &Connection {
        &self.conn
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

        if version < 3 {
            self.migrate_v3()?;
            self.conn.pragma_update(None, "user_version", 3)?;
        }

        if version < 4 {
            self.migrate_v4()?;
            self.conn.pragma_update(None, "user_version", 4)?;
        }

        if version < 5 {
            self.migrate_v5()?;
            self.conn.pragma_update(None, "user_version", 5)?;
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

    fn migrate_v3(&self) -> Result<(), DbError> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS fec_mappings (
                politician_id TEXT NOT NULL,
                fec_candidate_id TEXT NOT NULL,
                bioguide_id TEXT NOT NULL,
                election_cycle INTEGER,
                last_synced TEXT NOT NULL,
                PRIMARY KEY (politician_id, fec_candidate_id),
                FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
            )",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fec_mappings_fec_id ON fec_mappings(fec_candidate_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fec_mappings_bioguide ON fec_mappings(bioguide_id)",
            [],
        )?;
        Ok(())
    }

    fn migrate_v4(&self) -> Result<(), DbError> {
        // Create fec_committees table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS fec_committees (
                committee_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                committee_type TEXT,
                designation TEXT,
                party TEXT,
                state TEXT,
                cycles TEXT,
                last_synced TEXT NOT NULL
            )",
            [],
        )?;

        // Create donations table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS donations (
                sub_id TEXT PRIMARY KEY,
                committee_id TEXT NOT NULL,
                contributor_name TEXT,
                contributor_employer TEXT,
                contributor_occupation TEXT,
                contributor_state TEXT,
                contributor_city TEXT,
                contributor_zip TEXT,
                contribution_receipt_amount REAL,
                contribution_receipt_date TEXT,
                election_cycle INTEGER,
                memo_text TEXT,
                receipt_type TEXT
            )",
            [],
        )?;

        // Create donation_sync_meta table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS donation_sync_meta (
                politician_id TEXT NOT NULL,
                committee_id TEXT NOT NULL,
                last_index INTEGER,
                last_contribution_receipt_date TEXT,
                last_synced_at TEXT NOT NULL,
                total_synced INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (politician_id, committee_id)
            )",
            [],
        )?;

        // Add committee_ids column to fec_mappings
        match self
            .conn
            .execute("ALTER TABLE fec_mappings ADD COLUMN committee_ids TEXT", [])
        {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(_, Some(ref msg)))
                if msg.contains("duplicate column name") => {}
            Err(e) => return Err(e.into()),
        }

        // Create indexes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_donations_committee ON donations(committee_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_donations_date ON donations(contribution_receipt_date)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_donations_cycle ON donations(election_cycle)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_donation_sync_meta_politician ON donation_sync_meta(politician_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fec_committees_designation ON fec_committees(designation)",
            [],
        )?;

        Ok(())
    }

    fn migrate_v5(&self) -> Result<(), DbError> {
        // Create employer_mappings table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS employer_mappings (
                normalized_employer TEXT PRIMARY KEY,
                issuer_ticker TEXT NOT NULL,
                confidence REAL NOT NULL,
                match_type TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_updated TEXT NOT NULL,
                notes TEXT
            )",
            [],
        )?;

        // Create employer_lookup table
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS employer_lookup (
                raw_employer_lower TEXT PRIMARY KEY,
                normalized_employer TEXT NOT NULL
            )",
            [],
        )?;

        // Create indexes
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_employer_mappings_ticker ON employer_mappings(issuer_ticker)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_employer_mappings_confidence ON employer_mappings(confidence)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_employer_mappings_type ON employer_mappings(match_type)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_employer_lookup_normalized ON employer_lookup(normalized_employer)",
            [],
        )?;

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

    /// Count trades that need price enrichment.
    ///
    /// Returns the count of trades that have both issuer_ticker and tx_date
    /// (required for price lookup) but have not yet been price-enriched
    /// (price_enriched_at IS NULL).
    ///
    /// IMPORTANT: Joins issuers table to access issuer_ticker, which lives
    /// on the issuers table, not the trades table.
    pub fn count_unenriched_prices(&self) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM trades t
             JOIN issuers i ON t.issuer_id = i.issuer_id
             WHERE i.issuer_ticker IS NOT NULL
               AND i.issuer_ticker <> ''
               AND t.tx_date IS NOT NULL
               AND t.price_enriched_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Fetch trades that need price enrichment.
    ///
    /// Returns trades with issuer_ticker and tx_date but no price_enriched_at.
    /// Includes the dollar range fields (size_range_low, size_range_high) and
    /// value for share estimation.
    ///
    /// IMPORTANT: Joins issuers table to access i.issuer_ticker, which lives
    /// on the issuers table, not the trades table.
    pub fn get_unenriched_price_trades(
        &self,
        limit: Option<i64>,
    ) -> Result<Vec<PriceEnrichmentRow>, DbError> {
        let sql = match limit {
            Some(n) => format!(
                "SELECT t.tx_id, i.issuer_ticker, t.tx_date, t.size_range_low, t.size_range_high, t.value
                 FROM trades t
                 JOIN issuers i ON t.issuer_id = i.issuer_id
                 WHERE i.issuer_ticker IS NOT NULL
                   AND i.issuer_ticker <> ''
                   AND t.tx_date IS NOT NULL
                   AND t.price_enriched_at IS NULL
                 ORDER BY t.tx_id
                 LIMIT {}",
                n
            ),
            None => "SELECT t.tx_id, i.issuer_ticker, t.tx_date, t.size_range_low, t.size_range_high, t.value
                     FROM trades t
                     JOIN issuers i ON t.issuer_id = i.issuer_id
                     WHERE i.issuer_ticker IS NOT NULL
                       AND i.issuer_ticker <> ''
                       AND t.tx_date IS NOT NULL
                       AND t.price_enriched_at IS NULL
                     ORDER BY t.tx_id"
                .to_string(),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(PriceEnrichmentRow {
                    tx_id: row.get(0)?,
                    issuer_ticker: row.get(1)?,
                    tx_date: row.get(2)?,
                    size_range_low: row.get(3)?,
                    size_range_high: row.get(4)?,
                    value: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Update trade price enrichment data.
    ///
    /// Stores the historical price, estimated shares, and estimated value for
    /// a trade. Always sets price_enriched_at to mark the trade as processed,
    /// even if the price is None (invalid ticker case).
    ///
    /// This ensures trades are not re-processed on subsequent runs, supporting
    /// resumability after failures.
    pub fn update_trade_prices(
        &self,
        tx_id: i64,
        trade_date_price: Option<f64>,
        estimated_shares: Option<f64>,
        estimated_value: Option<f64>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE trades
             SET trade_date_price = ?1,
                 estimated_shares = ?2,
                 estimated_value = ?3,
                 price_enriched_at = datetime('now')
             WHERE tx_id = ?4",
            params![trade_date_price, estimated_shares, estimated_value, tx_id],
        )?;
        Ok(())
    }

    /// Update the current price for a trade by tx_id.
    ///
    /// Sets current_price and refreshes price_enriched_at timestamp.
    /// Called during the second phase of price enrichment (current prices by ticker).
    pub fn update_current_price(
        &self,
        tx_id: i64,
        current_price: Option<f64>,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE trades
             SET current_price = ?1,
                 price_enriched_at = datetime('now')
             WHERE tx_id = ?2",
            params![current_price, tx_id],
        )?;
        Ok(())
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
                    t.trade_date_price, t.current_price, t.price_enriched_at,
                    t.estimated_shares, t.estimated_value,
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
            let committees_str: String = row.get(22)?;
            let labels_str: String = row.get(23)?;

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
                trade_date_price: row.get(10)?,
                current_price: row.get(11)?,
                price_enriched_at: row.get(12)?,
                estimated_shares: row.get(13)?,
                estimated_value: row.get(14)?,
                politician_name: row.get(15)?,
                party: row.get(16)?,
                state: row.get(17)?,
                chamber: row.get(18)?,
                issuer_name: row.get(19)?,
                issuer_ticker: row.get::<_, Option<String>>(20)?.unwrap_or_default(),
                asset_type: row.get(21)?,
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

    /// Query trades for FIFO portfolio calculation.
    ///
    /// Returns only stock trades with non-null estimated_shares and trade_date_price,
    /// ordered chronologically (tx_date ASC, tx_id ASC) for deterministic FIFO processing.
    pub fn query_trades_for_portfolio(&self) -> Result<Vec<TradeFIFO>, DbError> {
        let sql = "SELECT t.tx_id, t.politician_id, i.issuer_ticker, t.tx_type, t.tx_date,
                          t.estimated_shares, t.trade_date_price
                   FROM trades t
                   JOIN issuers i ON t.issuer_id = i.issuer_id
                   JOIN assets a ON t.asset_id = a.asset_id
                   WHERE t.estimated_shares IS NOT NULL
                     AND t.trade_date_price IS NOT NULL
                     AND a.asset_type = 'stock'
                   ORDER BY t.tx_date ASC, t.tx_id ASC";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(TradeFIFO {
                tx_id: row.get(0)?,
                politician_id: row.get(1)?,
                ticker: row.get(2)?,
                tx_type: row.get(3)?,
                tx_date: row.get(4)?,
                estimated_shares: row.get(5)?,
                trade_date_price: row.get(6)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Upsert calculated positions to the positions table.
    ///
    /// Inserts all positions (including closed positions with shares_held near zero)
    /// for audit trail. Uses ON CONFLICT to update existing positions.
    pub fn upsert_positions(
        &self,
        positions: &std::collections::HashMap<(String, String), crate::portfolio::Position>,
    ) -> Result<usize, DbError> {
        let tx = self.conn.unchecked_transaction()?;

        let mut count = 0;
        for ((politician_id, ticker), position) in positions {
            tx.execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
                 ON CONFLICT(politician_id, issuer_ticker)
                 DO UPDATE SET
                   shares_held = excluded.shares_held,
                   cost_basis = excluded.cost_basis,
                   realized_pnl = excluded.realized_pnl,
                   last_updated = excluded.last_updated",
                params![
                    politician_id,
                    ticker,
                    position.shares_held(),
                    position.avg_cost_basis(),
                    position.realized_pnl,
                ],
            )?;
            count += 1;
        }

        tx.commit()?;
        Ok(count)
    }

    /// Query portfolio positions with unrealized P&L.
    ///
    /// Joins positions with current prices from trades table, computes unrealized P&L
    /// and percent change. By default filters closed positions (shares_held > 0.0001).
    pub fn get_portfolio(&self, filter: &PortfolioFilter) -> Result<Vec<PortfolioPosition>, DbError> {
        let mut sql = String::from(
            "SELECT
               p.politician_id,
               p.issuer_ticker,
               p.shares_held,
               p.cost_basis,
               p.realized_pnl,
               (SELECT t2.current_price
                FROM trades t2
                JOIN issuers i2 ON t2.issuer_id = i2.issuer_id
                WHERE i2.issuer_ticker = p.issuer_ticker
                  AND t2.current_price IS NOT NULL
                ORDER BY t2.price_enriched_at DESC
                LIMIT 1) as current_price,
               (SELECT t2.price_enriched_at
                FROM trades t2
                JOIN issuers i2 ON t2.issuer_id = i2.issuer_id
                WHERE i2.issuer_ticker = p.issuer_ticker
                  AND t2.current_price IS NOT NULL
                ORDER BY t2.price_enriched_at DESC
                LIMIT 1) as price_date,
               p.last_updated
             FROM positions p",
        );

        let mut joins_politician = false;
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_idx = 1;

        // Build WHERE clause
        let mut where_clauses = Vec::new();

        if !filter.include_closed {
            where_clauses.push("p.shares_held > 0.0001".to_string());
        }

        if let Some(ref politician_id) = filter.politician_id {
            where_clauses.push(format!("p.politician_id = ?{}", param_idx));
            params_vec.push(Box::new(politician_id.clone()));
            param_idx += 1;
        }

        if let Some(ref ticker) = filter.ticker {
            where_clauses.push(format!("p.issuer_ticker = ?{}", param_idx));
            params_vec.push(Box::new(ticker.clone()));
            param_idx += 1;
        }

        if filter.party.is_some() || filter.state.is_some() {
            sql.push_str(" JOIN politicians pol ON p.politician_id = pol.politician_id");
            joins_politician = true;
        }

        if let Some(ref party) = filter.party {
            where_clauses.push(format!("pol.party = ?{}", param_idx));
            params_vec.push(Box::new(party.clone()));
            param_idx += 1;
        }

        if let Some(ref state) = filter.state {
            where_clauses.push(format!("UPPER(pol.state_id) = UPPER(?{})", param_idx));
            params_vec.push(Box::new(state.clone()));
            param_idx += 1;
        }

        if !where_clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", where_clauses.join(" AND ")));
        }

        sql.push_str(" ORDER BY p.shares_held * p.cost_basis DESC");

        let _ = (joins_politician, param_idx); // suppress unused warnings

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let politician_id: String = row.get(0)?;
            let ticker: String = row.get(1)?;
            let shares_held: f64 = row.get(2)?;
            let cost_basis: f64 = row.get(3)?;
            let realized_pnl: f64 = row.get(4)?;
            let current_price: Option<f64> = row.get(5)?;
            let price_date: Option<String> = row.get(6)?;
            let last_updated: String = row.get(7)?;

            let unrealized_pnl = current_price.map(|price| (price - cost_basis) * shares_held);
            let unrealized_pnl_pct = current_price.map(|price| {
                if cost_basis > 0.0001 {
                    ((price - cost_basis) / cost_basis) * 100.0
                } else {
                    0.0
                }
            });
            let current_value = current_price.map(|price| price * shares_held);

            Ok(PortfolioPosition {
                politician_id,
                ticker,
                shares_held,
                cost_basis,
                realized_pnl,
                unrealized_pnl,
                unrealized_pnl_pct,
                current_price,
                current_value,
                price_date,
                last_updated,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Count option trades (non-stock, non-unknown asset types).
    ///
    /// Returns count of trades where asset_type is NOT 'stock' and NOT 'unknown'.
    /// Optionally filters by politician_id.
    pub fn count_option_trades(&self, politician_id: Option<&str>) -> Result<i64, DbError> {
        let count: i64 = match politician_id {
            Some(pol_id) => {
                let sql = "SELECT COUNT(*)
                           FROM trades t
                           JOIN assets a ON t.asset_id = a.asset_id
                           WHERE a.asset_type != 'stock'
                             AND a.asset_type != 'unknown'
                             AND t.politician_id = ?1";
                self.conn.query_row(sql, params![pol_id], |row| row.get(0))?
            }
            None => {
                let sql = "SELECT COUNT(*)
                           FROM trades t
                           JOIN assets a ON t.asset_id = a.asset_id
                           WHERE a.asset_type != 'stock'
                             AND a.asset_type != 'unknown'";
                self.conn.query_row(sql, [], |row| row.get(0))?
            }
        };
        Ok(count)
    }

    /// Get all politicians as (politician_id, last_name, state_id) tuples for FEC matching
    pub fn get_politicians_for_fec_matching(&self) -> Result<Vec<(String, String, String)>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT politician_id, last_name, state_id FROM politicians"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Upsert FEC candidate ID mappings
    pub fn upsert_fec_mappings(&mut self, mappings: &[crate::fec_mapping::FecMapping]) -> Result<usize, DbError> {
        let tx = self.conn.transaction()?;
        let mut count = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO fec_mappings (politician_id, fec_candidate_id, bioguide_id, last_synced)
                 VALUES (?1, ?2, ?3, datetime('now'))
                 ON CONFLICT(politician_id, fec_candidate_id) DO UPDATE SET
                   bioguide_id = excluded.bioguide_id,
                   last_synced = datetime('now')"
            )?;
            for mapping in mappings {
                stmt.execute(params![
                    mapping.politician_id,
                    mapping.fec_candidate_id,
                    mapping.bioguide_id,
                ])?;
                count += 1;
            }
        }
        tx.commit()?;
        Ok(count)
    }

    /// Get all FEC candidate IDs for a given politician
    pub fn get_fec_ids_for_politician(&self, politician_id: &str) -> Result<Vec<String>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT fec_candidate_id FROM fec_mappings WHERE politician_id = ?1"
        )?;
        let rows = stmt.query_map([politician_id], |row| row.get(0))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Get politician_id for a given bioguide_id
    pub fn get_politician_id_for_bioguide(&self, bioguide_id: &str) -> Result<Option<String>, DbError> {
        self.conn
            .query_row(
                "SELECT DISTINCT politician_id FROM fec_mappings WHERE bioguide_id = ?1 LIMIT 1",
                params![bioguide_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(DbError::from)
    }

    /// Count total FEC mappings
    pub fn count_fec_mappings(&self) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM fec_mappings", [], |row| row.get(0)
        )?;
        Ok(count)
    }

    /// Upsert a committee record from OpenFEC Committee type.
    pub fn upsert_committee(
        &self,
        committee: &crate::openfec::types::Committee,
    ) -> Result<(), DbError> {
        let cycles_json = serde_json::to_string(&committee.cycles)?;
        self.conn.execute(
            "INSERT INTO fec_committees (
                committee_id, name, committee_type, designation, party, state, cycles, last_synced
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
            ON CONFLICT(committee_id) DO UPDATE SET
                name = excluded.name,
                committee_type = excluded.committee_type,
                designation = excluded.designation,
                party = excluded.party,
                state = excluded.state,
                cycles = excluded.cycles,
                last_synced = datetime('now')",
            params![
                committee.committee_id,
                committee.name,
                committee.committee_type,
                committee.designation,
                committee.party,
                committee.state,
                cycles_json
            ],
        )?;
        Ok(())
    }

    /// Batch upsert committees from OpenFEC API response.
    pub fn upsert_committees(
        &self,
        committees: &[crate::openfec::types::Committee],
    ) -> Result<usize, DbError> {
        for committee in committees {
            self.upsert_committee(committee)?;
        }
        Ok(committees.len())
    }

    /// Get committee IDs for a politician from the committee_ids JSON column.
    /// Returns None if no committees stored. Merges across multiple FEC candidate IDs.
    pub fn get_committees_for_politician(
        &self,
        politician_id: &str,
    ) -> Result<Option<Vec<String>>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT committee_ids FROM fec_mappings WHERE politician_id = ?1 AND committee_ids IS NOT NULL"
        )?;
        let results: Vec<String> = stmt
            .query_map(params![politician_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        if results.is_empty() {
            return Ok(None);
        }

        let mut all_committees = std::collections::HashSet::new();
        for json_str in results {
            if json_str.trim().is_empty() {
                continue;
            }
            let committees: Vec<String> = serde_json::from_str(&json_str)?;
            all_committees.extend(committees);
        }

        if all_committees.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_committees.into_iter().collect()))
        }
    }

    /// Update committee_ids for all fec_mappings rows for a politician.
    pub fn update_politician_committees(
        &self,
        politician_id: &str,
        committee_ids: &[String],
    ) -> Result<(), DbError> {
        let json = serde_json::to_string(committee_ids)?;
        self.conn.execute(
            "UPDATE fec_mappings SET committee_ids = ?1 WHERE politician_id = ?2",
            params![json, politician_id],
        )?;
        Ok(())
    }

    /// Get politician name and state for OpenFEC API fallback search.
    pub fn get_politician_info(
        &self,
        politician_id: &str,
    ) -> Result<Option<(String, String, String)>, DbError> {
        self.conn
            .query_row(
                "SELECT first_name, last_name, state_id FROM politicians WHERE politician_id = ?1",
                params![politician_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(DbError::from)
    }

    /// Insert a single donation with deduplication.
    ///
    /// Returns Ok(true) if inserted, Ok(false) if duplicate or NULL sub_id.
    /// Skips contributions with None sub_id per research recommendation.
    pub fn insert_donation(
        &self,
        contribution: &crate::openfec::types::Contribution,
        committee_id: &str,
        cycle: Option<i32>,
    ) -> Result<bool, DbError> {
        // Skip NULL sub_id (per research: OpenFEC sometimes returns records without sub_id)
        let Some(ref sub_id) = contribution.sub_id else {
            return Ok(false);
        };

        let changes = self.conn.execute(
            "INSERT OR IGNORE INTO donations (
                sub_id, committee_id, contributor_name, contributor_employer,
                contributor_occupation, contributor_state, contributor_city,
                contributor_zip, contribution_receipt_amount,
                contribution_receipt_date, election_cycle, memo_text, receipt_type
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL)",
            params![
                sub_id,
                committee_id,
                contribution.contributor_name,
                contribution.contributor_employer,
                contribution.contributor_occupation,
                contribution.contributor_state,
                None::<String>, // contributor_city not in Contribution type
                None::<String>, // contributor_zip not in Contribution type
                contribution.contribution_receipt_amount,
                contribution.contribution_receipt_date,
                cycle,
            ],
        )?;

        Ok(changes > 0)
    }

    /// Load sync cursor for a politician/committee pair.
    ///
    /// Returns Some((last_index, last_contribution_receipt_date)) if cursor exists,
    /// None if this is the first sync for this politician/committee.
    pub fn load_sync_cursor(
        &self,
        politician_id: &str,
        committee_id: &str,
    ) -> Result<Option<(i64, String)>, DbError> {
        self.conn
            .query_row(
                "SELECT last_index, last_contribution_receipt_date
                 FROM donation_sync_meta
                 WHERE politician_id = ?1 AND committee_id = ?2
                   AND last_index IS NOT NULL",
                params![politician_id, committee_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(DbError::from)
    }

    /// Save sync cursor and donations atomically in a single transaction.
    ///
    /// This is CRITICAL for preventing cursor state desync (Pitfall 1 from research).
    /// Returns the count of actually inserted donations (excludes NULL sub_id and duplicates).
    pub fn save_sync_cursor_with_donations(
        &self,
        politician_id: &str,
        committee_id: &str,
        contributions: &[crate::openfec::types::Contribution],
        cycle: Option<i32>,
        last_index: i64,
        last_date: &str,
    ) -> Result<usize, DbError> {
        let tx = self.conn.unchecked_transaction()?;

        let mut inserted_count = 0;
        for contribution in contributions {
            // Skip NULL sub_id
            let Some(ref sub_id) = contribution.sub_id else {
                continue;
            };

            let changes = tx.execute(
                "INSERT OR IGNORE INTO donations (
                    sub_id, committee_id, contributor_name, contributor_employer,
                    contributor_occupation, contributor_state, contributor_city,
                    contributor_zip, contribution_receipt_amount,
                    contribution_receipt_date, election_cycle, memo_text, receipt_type
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, NULL)",
                params![
                    sub_id,
                    committee_id,
                    contribution.contributor_name,
                    contribution.contributor_employer,
                    contribution.contributor_occupation,
                    contribution.contributor_state,
                    None::<String>,
                    None::<String>,
                    contribution.contribution_receipt_amount,
                    contribution.contribution_receipt_date,
                    cycle,
                ],
            )?;

            if changes > 0 {
                inserted_count += 1;
            }
        }

        // Update cursor state with inserted count
        tx.execute(
            "INSERT OR REPLACE INTO donation_sync_meta (
                politician_id, committee_id, last_index,
                last_contribution_receipt_date, last_synced_at, total_synced
            ) VALUES (
                ?1, ?2, ?3, ?4, datetime('now'),
                COALESCE(
                    (SELECT total_synced FROM donation_sync_meta
                     WHERE politician_id = ?1 AND committee_id = ?2),
                    0
                ) + ?5
            )",
            params![
                politician_id,
                committee_id,
                last_index,
                last_date,
                inserted_count
            ],
        )?;

        tx.commit()?;
        Ok(inserted_count)
    }

    /// Mark a politician/committee sync as completed.
    ///
    /// Sets last_index to NULL to signal "sync completed, no more pages".
    /// Subsequent syncs can check is_sync_completed by testing last_index IS NULL.
    pub fn mark_sync_completed(
        &self,
        politician_id: &str,
        committee_id: &str,
    ) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO donation_sync_meta (
                politician_id, committee_id, last_index,
                last_contribution_receipt_date, last_synced_at, total_synced
            ) VALUES (
                ?1, ?2, NULL, NULL, datetime('now'),
                COALESCE(
                    (SELECT total_synced FROM donation_sync_meta
                     WHERE politician_id = ?1 AND committee_id = ?2),
                    0
                )
            )",
            params![politician_id, committee_id],
        )?;
        Ok(())
    }

    /// Find politicians by partial name match.
    ///
    /// Returns Vec of (politician_id, full_name) tuples.
    /// Caller handles disambiguation if multiple matches.
    pub fn find_politician_by_name(&self, name: &str) -> Result<Vec<(String, String)>, DbError> {
        let pattern = format!("%{}%", name);
        let mut stmt = self.conn.prepare(
            "SELECT politician_id, first_name || ' ' || last_name AS full_name
             FROM politicians
             WHERE (first_name || ' ' || last_name) LIKE ?1",
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Count donations for a politician across all their committees.
    ///
    /// Joins through donation_sync_meta to link donations (which only have committee_id)
    /// to politicians.
    pub fn count_donations_for_politician(&self, politician_id: &str) -> Result<i64, DbError> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM donations d
             JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
             WHERE dsm.politician_id = ?1",
            params![politician_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Query donations with filtering and dynamic WHERE clause construction.
    ///
    /// Returns individual donation records joined through donation_sync_meta
    /// to link donations to politicians. Supports filtering by politician,
    /// cycle, amount, employer, and contributor state.
    pub fn query_donations(&self, filter: &DonationFilter) -> Result<Vec<DonationRow>, DbError> {
        let (where_clause, params_vec) = build_donation_where_clause(filter);

        let mut sql = format!(
            "SELECT
                d.sub_id,
                COALESCE(d.contributor_name, 'Unknown') as contributor_name,
                COALESCE(d.contributor_employer, '') as contributor_employer,
                COALESCE(d.contributor_occupation, '') as contributor_occupation,
                COALESCE(d.contributor_state, '') as contributor_state,
                d.contribution_receipt_amount,
                COALESCE(d.contribution_receipt_date, '') as contribution_receipt_date,
                COALESCE(d.election_cycle, 0) as election_cycle,
                COALESCE(fc.name, '') as committee_name,
                COALESCE(fc.designation, '') as designation,
                p.first_name || ' ' || p.last_name AS politician_name
            FROM donations d
            JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
            JOIN politicians p ON dsm.politician_id = p.politician_id
            LEFT JOIN fec_committees fc ON d.committee_id = fc.committee_id
            {}
            ORDER BY d.contribution_receipt_amount DESC",
            where_clause
        );

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(DonationRow {
                sub_id: row.get(0)?,
                contributor_name: row.get(1)?,
                contributor_employer: row.get(2)?,
                contributor_occupation: row.get(3)?,
                contributor_state: row.get(4)?,
                amount: row.get(5)?,
                date: row.get(6)?,
                cycle: row.get(7)?,
                committee_name: row.get(8)?,
                committee_designation: row.get(9)?,
                politician_name: row.get(10)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Aggregate donations by contributor with total amount, count, and date range.
    ///
    /// Returns one row per unique contributor (name + state combination),
    /// ordered by total contribution amount descending.
    pub fn query_donations_by_contributor(
        &self,
        filter: &DonationFilter,
    ) -> Result<Vec<ContributorAggRow>, DbError> {
        let (where_clause, params_vec) = build_donation_where_clause(filter);

        let mut sql = format!(
            "SELECT
                COALESCE(d.contributor_name, 'Unknown') as contributor_name,
                COALESCE(d.contributor_state, '') as contributor_state,
                SUM(d.contribution_receipt_amount) as total_amount,
                COUNT(*) as donation_count,
                AVG(d.contribution_receipt_amount) as avg_amount,
                MAX(d.contribution_receipt_amount) as max_donation,
                MIN(d.contribution_receipt_date) as first_donation,
                MAX(d.contribution_receipt_date) as last_donation
            FROM donations d
            JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
            {}
            GROUP BY COALESCE(d.contributor_name, 'Unknown'), COALESCE(d.contributor_state, '')
            ORDER BY total_amount DESC",
            where_clause
        );

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(ContributorAggRow {
                contributor_name: row.get(0)?,
                contributor_state: row.get(1)?,
                total_amount: row.get(2)?,
                donation_count: row.get(3)?,
                avg_amount: row.get(4)?,
                max_donation: row.get(5)?,
                first_donation: row.get(6)?,
                last_donation: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Aggregate donations by employer with total amount, count, and contributor count.
    ///
    /// Returns one row per unique employer, ordered by total contribution amount descending.
    pub fn query_donations_by_employer(
        &self,
        filter: &DonationFilter,
    ) -> Result<Vec<EmployerAggRow>, DbError> {
        let (where_clause, params_vec) = build_donation_where_clause(filter);

        let mut sql = format!(
            "SELECT
                COALESCE(d.contributor_employer, 'Unknown') as employer,
                SUM(d.contribution_receipt_amount) as total_amount,
                COUNT(*) as donation_count,
                AVG(d.contribution_receipt_amount) as avg_amount,
                COUNT(DISTINCT d.contributor_name) as contributor_count
            FROM donations d
            JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
            {}
            GROUP BY COALESCE(d.contributor_employer, 'Unknown')
            ORDER BY total_amount DESC",
            where_clause
        );

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(EmployerAggRow {
                employer: row.get(0)?,
                total_amount: row.get(1)?,
                donation_count: row.get(2)?,
                avg_amount: row.get(3)?,
                contributor_count: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Aggregate donations by contributor state with total amount, count, and contributor count.
    ///
    /// Returns one row per unique state, ordered by total contribution amount descending.
    pub fn query_donations_by_state(
        &self,
        filter: &DonationFilter,
    ) -> Result<Vec<StateAggRow>, DbError> {
        let (where_clause, params_vec) = build_donation_where_clause(filter);

        let mut sql = format!(
            "SELECT
                COALESCE(d.contributor_state, 'Unknown') as state,
                SUM(d.contribution_receipt_amount) as total_amount,
                COUNT(*) as donation_count,
                AVG(d.contribution_receipt_amount) as avg_amount,
                COUNT(DISTINCT d.contributor_name) as contributor_count
            FROM donations d
            JOIN donation_sync_meta dsm ON d.committee_id = dsm.committee_id
            {}
            GROUP BY COALESCE(d.contributor_state, 'Unknown')
            ORDER BY total_amount DESC",
            where_clause
        );

        if let Some(n) = filter.limit {
            sql.push_str(&format!(" LIMIT {}", n));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(StateAggRow {
                state: row.get(0)?,
                total_amount: row.get(1)?,
                donation_count: row.get(2)?,
                avg_amount: row.get(3)?,
                contributor_count: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

/// Build dynamic WHERE clause for donation queries.
///
/// Shared helper for all four donation query methods to avoid code duplication.
/// Returns the WHERE clause string and a vector of boxed parameters.
fn build_donation_where_clause(
    filter: &DonationFilter,
) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    let mut clauses = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref politician_id) = filter.politician_id {
        clauses.push(format!("dsm.politician_id = ?{}", idx));
        params.push(Box::new(politician_id.clone()));
        idx += 1;
    }

    if let Some(cycle) = filter.cycle {
        clauses.push(format!("d.election_cycle = ?{}", idx));
        params.push(Box::new(cycle));
        idx += 1;
    }

    if let Some(min_amount) = filter.min_amount {
        clauses.push(format!("d.contribution_receipt_amount >= ?{}", idx));
        params.push(Box::new(min_amount));
        idx += 1;
    }

    if let Some(ref employer) = filter.employer {
        clauses.push(format!("d.contributor_employer LIKE ?{}", idx));
        params.push(Box::new(format!("%{}%", employer)));
        idx += 1;
    }

    if let Some(ref contributor_state) = filter.contributor_state {
        clauses.push(format!("d.contributor_state = ?{}", idx));
        params.push(Box::new(contributor_state.clone()));
        idx += 1;
    }

    let _ = idx; // suppress unused warning

    let where_clause = if clauses.is_empty() {
        "WHERE 1=1".to_string()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };

    (where_clause, params)
}

/// Filter parameters for donation queries.
#[derive(Debug, Default)]
pub struct DonationFilter {
    pub politician_id: Option<String>,
    pub cycle: Option<i32>,
    pub min_amount: Option<f64>,
    pub employer: Option<String>,
    pub contributor_state: Option<String>,
    pub limit: Option<i64>,
}

/// Individual donation record returned by query_donations.
#[derive(Debug, Clone, Serialize)]
pub struct DonationRow {
    pub sub_id: String,
    pub contributor_name: String,
    pub contributor_employer: String,
    pub contributor_occupation: String,
    pub contributor_state: String,
    pub amount: f64,
    pub date: String,
    pub cycle: i64,
    pub committee_name: String,
    pub committee_designation: String,
    pub politician_name: String,
}

/// Aggregated donation data by contributor.
#[derive(Debug, Clone, Serialize)]
pub struct ContributorAggRow {
    pub contributor_name: String,
    pub contributor_state: String,
    pub total_amount: f64,
    pub donation_count: i64,
    pub avg_amount: f64,
    pub max_donation: f64,
    pub first_donation: String,
    pub last_donation: String,
}

/// Aggregated donation data by employer.
#[derive(Debug, Clone, Serialize)]
pub struct EmployerAggRow {
    pub employer: String,
    pub total_amount: f64,
    pub donation_count: i64,
    pub avg_amount: f64,
    pub contributor_count: i64,
}

/// Aggregated donation data by contributor state.
#[derive(Debug, Clone, Serialize)]
pub struct StateAggRow {
    pub state: String,
    pub total_amount: f64,
    pub donation_count: i64,
    pub avg_amount: f64,
    pub contributor_count: i64,
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
    pub trade_date_price: Option<f64>,
    pub current_price: Option<f64>,
    pub price_enriched_at: Option<String>,
    pub estimated_shares: Option<f64>,
    pub estimated_value: Option<f64>,
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

/// A portfolio position with unrealized P&L calculations.
#[derive(Debug, Clone, Serialize)]
pub struct PortfolioPosition {
    pub politician_id: String,
    pub ticker: String,
    pub shares_held: f64,
    pub cost_basis: f64,
    pub realized_pnl: f64,
    pub unrealized_pnl: Option<f64>,
    pub unrealized_pnl_pct: Option<f64>,
    pub current_price: Option<f64>,
    pub current_value: Option<f64>,
    pub price_date: Option<String>,
    pub last_updated: String,
}

/// Filter parameters for [`Db::get_portfolio`].
#[derive(Debug, Default)]
pub struct PortfolioFilter {
    pub politician_id: Option<String>,
    pub ticker: Option<String>,
    pub party: Option<String>,
    pub state: Option<String>,
    pub include_closed: bool,
}

/// A trade row for price enrichment, including ticker and date information.
///
/// Used by the price enrichment pipeline to fetch trades that need historical
/// price data. Includes the issuer_ticker from the issuers table (via JOIN).
#[derive(Debug)]
pub struct PriceEnrichmentRow {
    pub tx_id: i64,
    pub issuer_ticker: String,
    pub tx_date: String,
    pub size_range_low: Option<i64>,
    pub size_range_high: Option<i64>,
    pub value: i64,
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
CREATE TABLE IF NOT EXISTS ingest_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
";

    /// The V1 schema with enriched_at columns but without price columns, used for v1-to-v2 migration test.
    const V1_SCHEMA: &str = "
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
    sector TEXT,
    enriched_at TEXT
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
    chamber TEXT NOT NULL,
    enriched_at TEXT
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
    enriched_at TEXT,
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

    /// The V2 schema with enriched_at and price columns but without fec_mappings table.
    const V2_SCHEMA: &str = "
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
    sector TEXT,
    enriched_at TEXT
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
    chamber TEXT NOT NULL,
    enriched_at TEXT
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
    enriched_at TEXT,
    trade_date_price REAL,
    current_price REAL,
    price_enriched_at TEXT,
    estimated_shares REAL,
    estimated_value REAL,
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE,
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,
    FOREIGN KEY (issuer_id) REFERENCES issuers(issuer_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS positions (
    politician_id TEXT NOT NULL,
    issuer_ticker TEXT NOT NULL,
    shares_held REAL NOT NULL,
    cost_basis REAL NOT NULL,
    realized_pnl REAL NOT NULL DEFAULT 0.0,
    last_updated TEXT NOT NULL,
    PRIMARY KEY (politician_id, issuer_ticker),
    FOREIGN KEY (politician_id) REFERENCES politicians(politician_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS ingest_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
        assert_eq!(get_user_version(&db), 4);
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

        // Verify user_version is now 3 (all migrations v1, v2, v3 applied)
        assert_eq!(get_user_version(&db), 4);

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
    fn test_migration_v1_to_v2() {
        // Simulate a v1 database: create v1 schema with enriched_at columns,
        // set user_version to 1
        let db = Db::open_in_memory().expect("open in-memory db");
        db.conn.execute_batch(V1_SCHEMA).expect("create v1 schema");
        db.conn.pragma_update(None, "user_version", 1).expect("set user_version to 1");

        // Verify no price columns yet
        assert!(!has_column(&db, "trades", "trade_date_price"));
        assert!(!has_column(&db, "trades", "current_price"));
        assert!(!has_column(&db, "trades", "price_enriched_at"));
        assert!(!has_column(&db, "trades", "estimated_shares"));
        assert!(!has_column(&db, "trades", "estimated_value"));
        assert_eq!(get_user_version(&db), 1);

        // Insert a test trade before migration
        db.conn.execute("INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')", []).expect("insert asset");
        db.conn.execute("INSERT INTO issuers (issuer_id, issuer_name) VALUES (1, 'TestCorp')", []).expect("insert issuer");
        db.conn.execute(
            "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
             VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
            [],
        ).expect("insert test politician");
        db.conn.execute(
            "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date,
             tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
             VALUES (1, 'P000001', 1, 1, '2025-01-01', '2025-01-01', '2025-01-01', 'buy', 0,
             'self', 'senate', 50000, 100, 'https://example.com', 5)",
            [],
        ).expect("insert trade");

        // Run init which should apply v2 migration
        db.init().expect("init with v2 migration");

        // Verify price columns exist
        assert!(has_column(&db, "trades", "trade_date_price"));
        assert!(has_column(&db, "trades", "current_price"));
        assert!(has_column(&db, "trades", "price_enriched_at"));
        assert!(has_column(&db, "trades", "estimated_shares"));
        assert!(has_column(&db, "trades", "estimated_value"));

        // Verify positions table exists
        let table_exists: bool = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='positions'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            }
        ).expect("check positions table");
        assert!(table_exists, "positions table should exist");

        // Verify user_version is now 3 (v1, v2, and v3 migrations applied)
        assert_eq!(get_user_version(&db), 4);

        // Verify pre-existing trade data is preserved
        let value: i64 = db.conn.query_row(
            "SELECT value FROM trades WHERE tx_id = 1",
            [],
            |row| row.get(0),
        ).expect("query test trade");
        assert_eq!(value, 50000);
    }

    #[test]
    fn test_migration_v2_idempotent() {
        let db = open_test_db();
        // DB is now at v3. Call init again -- must not error
        db.init().expect("second init should not error");
        assert_eq!(get_user_version(&db), 4);

        // Verify price columns still exist
        assert!(has_column(&db, "trades", "trade_date_price"));
        assert!(has_column(&db, "trades", "current_price"));
        assert!(has_column(&db, "trades", "price_enriched_at"));
        assert!(has_column(&db, "trades", "estimated_shares"));
        assert!(has_column(&db, "trades", "estimated_value"));
    }

    #[test]
    fn test_fresh_db_has_fec_mappings_table() {
        let db = Db::open_in_memory().unwrap();
        db.init().unwrap();
        // Verify table exists by querying it
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM fec_mappings", [], |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_migration_v2_to_v3() {
        // Simulate a v2 database: create v2 schema with price columns,
        // set user_version to 2
        let db = Db::open_in_memory().expect("open in-memory db");
        db.conn.execute_batch(V2_SCHEMA).expect("create v2 schema");
        db.conn.pragma_update(None, "user_version", 2).expect("set user_version to 2");

        // Verify no fec_mappings table yet
        let table_exists: bool = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='fec_mappings'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            }
        ).unwrap();
        assert!(!table_exists, "fec_mappings table should not exist yet");
        assert_eq!(get_user_version(&db), 2);

        // Insert a test politician before migration
        db.conn.execute("INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')", []).expect("insert asset");
        db.conn.execute("INSERT INTO issuers (issuer_id, issuer_name) VALUES (1, 'TestCorp')", []).expect("insert issuer");
        db.conn.execute(
            "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
             VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
            [],
        ).expect("insert test politician");

        // Run init which should apply v3 migration
        db.init().expect("init with v3 migration");

        // Verify fec_mappings table now exists
        let table_exists: bool = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='fec_mappings'",
            [],
            |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            }
        ).expect("check fec_mappings table");
        assert!(table_exists, "fec_mappings table should exist after migration");

        // Verify user_version is now 3
        assert_eq!(get_user_version(&db), 4);

        // Verify pre-existing politician data is preserved
        let name: String = db.conn.query_row(
            "SELECT first_name FROM politicians WHERE politician_id = 'P000001'",
            [],
            |row| row.get(0),
        ).expect("query test politician");
        assert_eq!(name, "Jane");
    }

    #[test]
    fn test_init_sets_version_3() {
        let db = Db::open_in_memory().unwrap();
        db.init().unwrap();
        let version: i32 = db.conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, 4);
    }

    #[test]
    fn test_migration_v3_idempotent() {
        let db = Db::open_in_memory().unwrap();
        db.init().unwrap();
        db.init().unwrap(); // Should not fail
        let version: i32 = db.conn.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
        assert_eq!(version, 4);
    }

    #[test]
    fn test_query_trades_price_fields() {
        let db = open_test_db();

        // Insert required parent rows
        db.conn.execute("INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')", []).expect("insert asset");
        db.conn.execute("INSERT INTO issuers (issuer_id, issuer_name) VALUES (1, 'TestCorp')", []).expect("insert issuer");
        db.conn.execute(
            "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
             VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
            [],
        ).expect("insert politician");

        // Insert a trade
        db.conn.execute(
            "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date,
             tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
             VALUES (1, 'P000001', 1, 1, '2025-01-01', '2025-01-01', '2025-01-01', 'buy', 0,
             'self', 'senate', 50000, 100, 'https://example.com', 5)",
            [],
        ).expect("insert trade");

        // Manually UPDATE the trade to set price fields
        db.conn.execute(
            "UPDATE trades SET trade_date_price = 150.25, current_price = 175.50,
             price_enriched_at = '2025-06-01T00:00:00Z', estimated_shares = 100.0, estimated_value = 15025.0
             WHERE tx_id = 1",
            [],
        ).expect("update trade with price fields");

        // Query trades
        let filter = DbTradeFilter::default();
        let rows = db.query_trades(&filter).expect("query trades");
        assert_eq!(rows.len(), 1);

        let row = &rows[0];
        assert_eq!(row.trade_date_price, Some(150.25));
        assert_eq!(row.current_price, Some(175.50));
        assert_eq!(row.price_enriched_at, Some("2025-06-01T00:00:00Z".to_string()));
        assert_eq!(row.estimated_shares, Some(100.0));
        assert_eq!(row.estimated_value, Some(15025.0));
    }

    #[test]
    fn test_query_trades_price_fields_null() {
        let db = open_test_db();

        // Insert required parent rows
        db.conn.execute("INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')", []).expect("insert asset");
        db.conn.execute("INSERT INTO issuers (issuer_id, issuer_name) VALUES (1, 'TestCorp')", []).expect("insert issuer");
        db.conn.execute(
            "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
             VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'senate')",
            [],
        ).expect("insert politician");

        // Insert a trade without setting price fields
        db.conn.execute(
            "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date,
             tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
             VALUES (1, 'P000001', 1, 1, '2025-01-01', '2025-01-01', '2025-01-01', 'buy', 0,
             'self', 'senate', 50000, 100, 'https://example.com', 5)",
            [],
        ).expect("insert trade");

        // Query trades
        let filter = DbTradeFilter::default();
        let rows = db.query_trades(&filter).expect("query trades");
        assert_eq!(rows.len(), 1);

        let row = &rows[0];
        assert_eq!(row.trade_date_price, None);
        assert_eq!(row.current_price, None);
        assert_eq!(row.price_enriched_at, None);
        assert_eq!(row.estimated_shares, None);
        assert_eq!(row.estimated_value, None);
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

    // --- Price enrichment tests ---

    #[test]
    fn test_count_unenriched_prices_empty() {
        let db = open_test_db();
        let count = db.count_unenriched_prices().expect("count");
        assert_eq!(count, 0, "empty DB should have 0 unenriched prices");
    }

    #[test]
    fn test_count_unenriched_prices_excludes_no_ticker() {
        let mut db = open_test_db();
        // Insert trade but issuer has NULL ticker
        let trade = make_test_scraped_trade(100, "P000001", 1);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Verify issuer has NULL ticker (default from make_test_scraped_trade)
        // Actually, make_test_scraped_trade sets issuer_ticker to Some("TST")
        // So we need to manually set it to NULL for this test
        db.conn
            .execute(
                "UPDATE issuers SET issuer_ticker = NULL WHERE issuer_id = 1",
                [],
            )
            .expect("clear ticker");

        let count = db.count_unenriched_prices().expect("count");
        assert_eq!(
            count, 0,
            "trade without issuer_ticker should be excluded"
        );
    }

    #[test]
    fn test_count_unenriched_prices_with_ticker() {
        let mut db = open_test_db();
        // Insert trade with ticker (make_test_scraped_trade sets issuer_ticker to "TST")
        let trade = make_test_scraped_trade(101, "P000002", 2);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        let count = db.count_unenriched_prices().expect("count");
        assert_eq!(count, 1, "trade with ticker should be counted");
    }

    #[test]
    fn test_count_unenriched_prices_excludes_already_enriched() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(102, "P000003", 3);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Initially counted
        assert_eq!(db.count_unenriched_prices().expect("count before"), 1);

        // Mark as enriched
        db.update_trade_prices(102, Some(150.0), Some(100.0), Some(15000.0))
            .expect("update");

        // Should no longer be counted
        let count = db.count_unenriched_prices().expect("count after");
        assert_eq!(count, 0, "enriched trade should be excluded");
    }

    #[test]
    fn test_get_unenriched_price_trades_basic() {
        let mut db = open_test_db();
        let trade1 = make_test_scraped_trade(201, "P000010", 10);
        let trade2 = make_test_scraped_trade(202, "P000011", 11);
        db.upsert_scraped_trades(&[trade1, trade2])
            .expect("upsert");

        let rows = db
            .get_unenriched_price_trades(None)
            .expect("get_unenriched");
        assert_eq!(rows.len(), 2, "should return both trades");
        assert_eq!(rows[0].tx_id, 201);
        assert_eq!(rows[0].issuer_ticker, "TST");
        assert_eq!(rows[0].tx_date, "2025-06-10");
        assert_eq!(rows[1].tx_id, 202);
    }

    #[test]
    fn test_get_unenriched_price_trades_with_limit() {
        let mut db = open_test_db();
        let trade1 = make_test_scraped_trade(301, "P000020", 20);
        let trade2 = make_test_scraped_trade(302, "P000021", 21);
        let trade3 = make_test_scraped_trade(303, "P000022", 22);
        db.upsert_scraped_trades(&[trade1, trade2, trade3])
            .expect("upsert");

        let rows = db
            .get_unenriched_price_trades(Some(2))
            .expect("get_unenriched with limit");
        assert_eq!(rows.len(), 2, "should respect limit parameter");
        assert_eq!(rows[0].tx_id, 301);
        assert_eq!(rows[1].tx_id, 302);
    }

    #[test]
    fn test_get_unenriched_price_trades_has_range_fields() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(401, "P000030", 30);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Enrich with size range data
        let detail = ScrapedTradeDetail {
            asset_type: Some("stock".to_string()),
            committees: vec![],
            labels: vec![],
            filing_id: None,
            filing_url: None,
            has_capital_gains: None,
            price: None,
            size: None,
            size_range_high: Some(50000),
            size_range_low: Some(15001),
        };
        db.update_trade_detail(401, &detail).expect("update");

        let rows = db
            .get_unenriched_price_trades(None)
            .expect("get_unenriched");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].size_range_low, Some(15001));
        assert_eq!(rows[0].size_range_high, Some(50000));
        assert_eq!(rows[0].value, 50000);
    }

    #[test]
    fn test_update_trade_prices_stores_values() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(501, "P000040", 40);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        db.update_trade_prices(501, Some(123.45), Some(100.5), Some(12406.725))
            .expect("update");

        // Query back via raw SQL
        let (price, shares, value, enriched_at): (Option<f64>, Option<f64>, Option<f64>, Option<String>) = db
            .conn
            .query_row(
                "SELECT trade_date_price, estimated_shares, estimated_value, price_enriched_at FROM trades WHERE tx_id = 501",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("query");

        assert_eq!(price, Some(123.45));
        assert_eq!(shares, Some(100.5));
        assert_eq!(value, Some(12406.725));
        assert!(enriched_at.is_some(), "price_enriched_at should be set");
    }

    #[test]
    fn test_update_trade_prices_stores_none() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(502, "P000041", 41);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Update with all None (invalid ticker case)
        db.update_trade_prices(502, None, None, None)
            .expect("update");

        let (price, shares, value, enriched_at): (Option<f64>, Option<f64>, Option<f64>, Option<String>) = db
            .conn
            .query_row(
                "SELECT trade_date_price, estimated_shares, estimated_value, price_enriched_at FROM trades WHERE tx_id = 502",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("query");

        assert_eq!(price, None);
        assert_eq!(shares, None);
        assert_eq!(value, None);
        assert!(
            enriched_at.is_some(),
            "price_enriched_at should still be set even with None values"
        );
    }

    #[test]
    fn test_update_trade_prices_skips_on_rerun() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(503, "P000042", 42);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        // Initially unenriched
        assert_eq!(db.count_unenriched_prices().expect("count before"), 1);

        // Enrich
        db.update_trade_prices(503, Some(150.0), Some(200.0), Some(30000.0))
            .expect("update");

        // Should no longer be counted as unenriched
        assert_eq!(
            db.count_unenriched_prices().expect("count after"),
            0,
            "enriched trade should not be re-processed"
        );
    }

    #[test]
    fn test_update_current_price_stores_value() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(504, "P000043", 43);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        db.update_current_price(504, Some(155.50)).expect("update");

        let (current_price, enriched_at): (Option<f64>, Option<String>) = db
            .conn
            .query_row(
                "SELECT current_price, price_enriched_at FROM trades WHERE tx_id = 504",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query");

        assert_eq!(current_price, Some(155.50));
        assert!(enriched_at.is_some(), "price_enriched_at should be set");
    }

    #[test]
    fn test_update_current_price_stores_none() {
        let mut db = open_test_db();
        let trade = make_test_scraped_trade(505, "P000044", 44);
        db.upsert_scraped_trades(&[trade]).expect("upsert");

        db.update_current_price(505, None).expect("update");

        let (current_price, enriched_at): (Option<f64>, Option<String>) = db
            .conn
            .query_row(
                "SELECT current_price, price_enriched_at FROM trades WHERE tx_id = 505",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("query");

        assert_eq!(current_price, None);
        assert!(
            enriched_at.is_some(),
            "price_enriched_at should be set even with None value"
        );
    }

    #[test]
    fn test_query_trades_for_portfolio_empty() {
        let db = open_test_db();
        let trades = db
            .query_trades_for_portfolio()
            .expect("query_trades_for_portfolio");
        assert_eq!(trades.len(), 0);
    }

    #[test]
    fn test_query_trades_for_portfolio_filters_options() {
        let db = open_test_db();

        // Insert politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert issuer
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker)
                 VALUES (1, 'Apple Inc.', 'AAPL')",
                [],
            )
            .expect("insert issuer");

        // Insert stock asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert stock asset");

        // Insert option asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (2, 'stock-option')",
                [],
            )
            .expect("insert option asset");

        // Insert stock trade with prices
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, estimated_shares, trade_date_price)
                 VALUES (1, 'P000001', 1, 1, '2024-01-01', '2024-01-01', '2024-01-01', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0, 100.0, 50.0)",
                [],
            )
            .expect("insert stock trade");

        // Insert option trade with prices
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, estimated_shares, trade_date_price)
                 VALUES (2, 'P000001', 2, 1, '2024-01-02', '2024-01-02', '2024-01-02', 'buy', 0, 'self', 'house', 1000, 1, 'http://example.com', 0, 10.0, 100.0)",
                [],
            )
            .expect("insert option trade");

        let trades = db
            .query_trades_for_portfolio()
            .expect("query_trades_for_portfolio");

        assert_eq!(trades.len(), 1, "Should only return stock trade");
        assert_eq!(trades[0].tx_id, 1);
        assert_eq!(trades[0].ticker, "AAPL");
    }

    #[test]
    fn test_query_trades_for_portfolio_ordering() {
        let db = open_test_db();

        // Insert test data
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker)
                 VALUES (1, 'Apple Inc.', 'AAPL')",
                [],
            )
            .expect("insert issuer");

        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert asset");

        // Insert trades in non-chronological order
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, estimated_shares, trade_date_price)
                 VALUES (3, 'P000001', 1, 1, '2024-01-03', '2024-01-03', '2024-01-03', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0, 100.0, 50.0)",
                [],
            )
            .expect("insert trade 3");

        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, estimated_shares, trade_date_price)
                 VALUES (1, 'P000001', 1, 1, '2024-01-01', '2024-01-01', '2024-01-01', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0, 100.0, 50.0)",
                [],
            )
            .expect("insert trade 1");

        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, estimated_shares, trade_date_price)
                 VALUES (2, 'P000001', 1, 1, '2024-01-02', '2024-01-02', '2024-01-02', 'sell', 0, 'self', 'house', 2000, 1, 'http://example.com', 0, 40.0, 50.0)",
                [],
            )
            .expect("insert trade 2");

        let trades = db
            .query_trades_for_portfolio()
            .expect("query_trades_for_portfolio");

        assert_eq!(trades.len(), 3);
        assert_eq!(trades[0].tx_id, 1, "Should be ordered by tx_date ASC");
        assert_eq!(trades[1].tx_id, 2);
        assert_eq!(trades[2].tx_id, 3);
    }

    #[test]
    fn test_query_trades_for_portfolio_skips_unenriched() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker)
                 VALUES (1, 'Apple Inc.', 'AAPL')",
                [],
            )
            .expect("insert issuer");

        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert asset");

        // Insert trade without estimated_shares (NULL)
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (1, 'P000001', 1, 1, '2024-01-01', '2024-01-01', '2024-01-01', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0)",
                [],
            )
            .expect("insert unenriched trade");

        let trades = db
            .query_trades_for_portfolio()
            .expect("query_trades_for_portfolio");

        assert_eq!(
            trades.len(),
            0,
            "Should exclude trades with NULL estimated_shares"
        );
    }

    #[test]
    fn test_upsert_positions_basic() {
        use crate::portfolio::Position;
        use std::collections::HashMap;

        let db = open_test_db();

        // Insert politician for foreign key
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        let mut positions = HashMap::new();
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());

        positions.insert(("P000001".to_string(), "AAPL".to_string()), pos);

        let count = db.upsert_positions(&positions).expect("upsert_positions");
        assert_eq!(count, 1);

        // Verify row in positions table
        let (shares, cost_basis, realized_pnl): (f64, f64, f64) = db
            .conn
            .query_row(
                "SELECT shares_held, cost_basis, realized_pnl FROM positions WHERE politician_id = 'P000001' AND issuer_ticker = 'AAPL'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query position");

        assert_eq!(shares, 100.0);
        assert_eq!(cost_basis, 50.0);
        assert_eq!(realized_pnl, 0.0);
    }

    #[test]
    fn test_upsert_positions_updates_existing() {
        use crate::portfolio::Position;
        use std::collections::HashMap;

        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // First upsert
        let mut positions = HashMap::new();
        let mut pos = Position::new("P000001".to_string(), "AAPL".to_string());
        pos.buy(100.0, 50.0, "2024-01-01".to_string());
        positions.insert(("P000001".to_string(), "AAPL".to_string()), pos);
        db.upsert_positions(&positions).expect("upsert_positions 1");

        // Second upsert with updated position
        let mut positions2 = HashMap::new();
        let mut pos2 = Position::new("P000001".to_string(), "AAPL".to_string());
        pos2.buy(100.0, 50.0, "2024-01-01".to_string());
        pos2.sell(40.0, 70.0).expect("sell");
        positions2.insert(("P000001".to_string(), "AAPL".to_string()), pos2);
        let count = db.upsert_positions(&positions2).expect("upsert_positions 2");

        assert_eq!(count, 1);

        // Verify updated values
        let (shares, cost_basis, realized_pnl): (f64, f64, f64) = db
            .conn
            .query_row(
                "SELECT shares_held, cost_basis, realized_pnl FROM positions WHERE politician_id = 'P000001' AND issuer_ticker = 'AAPL'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query position");

        assert_eq!(shares, 60.0);
        assert_eq!(cost_basis, 50.0);
        assert_eq!(realized_pnl, 800.0); // (70-50)*40
    }

    #[test]
    fn test_get_portfolio_empty() {
        let db = open_test_db();
        let filter = PortfolioFilter::default();
        let positions = db.get_portfolio(&filter).expect("get_portfolio");
        assert_eq!(positions.len(), 0);
    }

    #[test]
    fn test_get_portfolio_with_unrealized_pnl() {
        let db = open_test_db();

        // Insert politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert issuer
        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker)
                 VALUES (1, 'Apple Inc.', 'AAPL')",
                [],
            )
            .expect("insert issuer");

        // Insert position
        db.conn
            .execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES ('P000001', 'AAPL', 100.0, 50.0, 0.0, '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert position");

        // Insert asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert asset");

        // Insert trade with current_price
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap, current_price, price_enriched_at)
                 VALUES (1, 'P000001', 1, 1, '2024-01-01', '2024-01-01', '2024-01-01', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0, 75.0, '2024-01-02T00:00:00Z')",
                [],
            )
            .expect("insert trade");

        let filter = PortfolioFilter::default();
        let positions = db.get_portfolio(&filter).expect("get_portfolio");

        assert_eq!(positions.len(), 1);
        let pos = &positions[0];
        assert_eq!(pos.ticker, "AAPL");
        assert_eq!(pos.shares_held, 100.0);
        assert_eq!(pos.cost_basis, 50.0);
        assert_eq!(pos.current_price, Some(75.0));
        assert_eq!(pos.unrealized_pnl, Some(2500.0)); // (75-50)*100
        assert_eq!(pos.unrealized_pnl_pct, Some(50.0)); // (75-50)/50*100
        assert_eq!(pos.current_value, Some(7500.0)); // 75*100
    }

    #[test]
    fn test_get_portfolio_filters_closed() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert position with shares_held = 0
        db.conn
            .execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES ('P000001', 'AAPL', 0.0, 50.0, 1000.0, '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert closed position");

        // Default filter (include_closed = false)
        let filter = PortfolioFilter::default();
        let positions = db.get_portfolio(&filter).expect("get_portfolio");
        assert_eq!(
            positions.len(),
            0,
            "Closed position should be filtered by default"
        );

        // With include_closed = true
        let filter = PortfolioFilter {
            include_closed: true,
            ..Default::default()
        };
        let positions = db.get_portfolio(&filter).expect("get_portfolio");
        assert_eq!(positions.len(), 1, "Closed position should be included");
        assert_eq!(positions[0].shares_held, 0.0);
    }

    #[test]
    fn test_get_portfolio_filter_by_politician() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician 1");

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000002', 'NY', 'Republican', 'Jane', 'Smith', '1975-01-01', 'female', 'senate')",
                [],
            )
            .expect("insert politician 2");

        db.conn
            .execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES ('P000001', 'AAPL', 100.0, 50.0, 0.0, '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert position 1");

        db.conn
            .execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES ('P000002', 'MSFT', 200.0, 60.0, 0.0, '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert position 2");

        let filter = PortfolioFilter {
            politician_id: Some("P000001".to_string()),
            ..Default::default()
        };
        let positions = db.get_portfolio(&filter).expect("get_portfolio");

        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].politician_id, "P000001");
        assert_eq!(positions[0].ticker, "AAPL");
    }

    #[test]
    fn test_count_option_trades() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        db.conn
            .execute(
                "INSERT INTO issuers (issuer_id, issuer_name, issuer_ticker)
                 VALUES (1, 'Apple Inc.', 'AAPL')",
                [],
            )
            .expect("insert issuer");

        // Insert stock asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (1, 'stock')",
                [],
            )
            .expect("insert stock asset");

        // Insert option asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (2, 'stock-option')",
                [],
            )
            .expect("insert option asset");

        // Insert unknown asset
        db.conn
            .execute(
                "INSERT INTO assets (asset_id, asset_type) VALUES (3, 'unknown')",
                [],
            )
            .expect("insert unknown asset");

        // Insert stock trade
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (1, 'P000001', 1, 1, '2024-01-01', '2024-01-01', '2024-01-01', 'buy', 0, 'self', 'house', 5000, 1, 'http://example.com', 0)",
                [],
            )
            .expect("insert stock trade");

        // Insert option trades
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (2, 'P000001', 2, 1, '2024-01-02', '2024-01-02', '2024-01-02', 'buy', 0, 'self', 'house', 1000, 1, 'http://example.com', 0)",
                [],
            )
            .expect("insert option trade 1");

        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (3, 'P000001', 2, 1, '2024-01-03', '2024-01-03', '2024-01-03', 'sell', 0, 'self', 'house', 500, 1, 'http://example.com', 0)",
                [],
            )
            .expect("insert option trade 2");

        // Insert unknown trade
        db.conn
            .execute(
                "INSERT INTO trades (tx_id, politician_id, asset_id, issuer_id, pub_date, filing_date, tx_date, tx_type, has_capital_gains, owner, chamber, value, filing_id, filing_url, reporting_gap)
                 VALUES (4, 'P000001', 3, 1, '2024-01-04', '2024-01-04', '2024-01-04', 'buy', 0, 'self', 'house', 2000, 1, 'http://example.com', 0)",
                [],
            )
            .expect("insert unknown trade");

        let count = db.count_option_trades(None).expect("count_option_trades");
        assert_eq!(count, 2, "Should count only option trades (not stock or unknown)");

        let count_filtered = db
            .count_option_trades(Some("P000001"))
            .expect("count_option_trades filtered");
        assert_eq!(count_filtered, 2);
    }

    #[test]
    fn test_get_portfolio_missing_current_price() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'John', 'Doe', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert position without corresponding current_price trade
        db.conn
            .execute(
                "INSERT INTO positions (politician_id, issuer_ticker, shares_held, cost_basis, realized_pnl, last_updated)
                 VALUES ('P000001', 'AAPL', 100.0, 50.0, 0.0, '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert position");

        let filter = PortfolioFilter::default();
        let positions = db.get_portfolio(&filter).expect("get_portfolio");

        assert_eq!(positions.len(), 1);
        let pos = &positions[0];
        assert_eq!(pos.current_price, None);
        assert_eq!(pos.unrealized_pnl, None);
        assert_eq!(pos.unrealized_pnl_pct, None);
        assert_eq!(pos.current_value, None);
    }

    #[test]
    fn test_upsert_fec_mappings() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'Jane', 'Doe', '1970-01-01', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        let mappings = vec![
            crate::fec_mapping::FecMapping {
                politician_id: "P000001".to_string(),
                fec_candidate_id: "H0CA05080".to_string(),
                bioguide_id: "D000001".to_string(),
            },
            crate::fec_mapping::FecMapping {
                politician_id: "P000001".to_string(),
                fec_candidate_id: "H0CA05120".to_string(),
                bioguide_id: "D000001".to_string(),
            },
        ];

        let count = db.upsert_fec_mappings(&mappings).expect("upsert_fec_mappings");
        assert_eq!(count, 2);

        // Verify we can query them back
        let fec_ids = db.get_fec_ids_for_politician("P000001").expect("get_fec_ids");
        assert_eq!(fec_ids.len(), 2);
        assert!(fec_ids.contains(&"H0CA05080".to_string()));
        assert!(fec_ids.contains(&"H0CA05120".to_string()));
    }

    #[test]
    fn test_get_fec_ids_for_politician() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000002', 'TX', 'Republican', 'John', 'Smith', '1965-05-15', 'male', 'senate')",
                [],
            )
            .expect("insert politician");

        let mappings = vec![crate::fec_mapping::FecMapping {
            politician_id: "P000002".to_string(),
            fec_candidate_id: "S4TX00123".to_string(),
            bioguide_id: "S000002".to_string(),
        }];

        db.upsert_fec_mappings(&mappings).expect("upsert");

        let fec_ids = db.get_fec_ids_for_politician("P000002").expect("get_fec_ids");
        assert_eq!(fec_ids.len(), 1);
        assert_eq!(fec_ids[0], "S4TX00123");
    }

    #[test]
    fn test_get_fec_ids_for_unknown_politician() {
        let db = open_test_db();

        let fec_ids = db
            .get_fec_ids_for_politician("P999999")
            .expect("get_fec_ids for unknown politician");
        assert_eq!(fec_ids.len(), 0, "Should return empty vec for unknown politician");
    }

    #[test]
    fn test_get_politician_id_for_bioguide() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000003', 'NY', 'Democrat', 'Alice', 'Johnson', '1975-03-20', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        let mappings = vec![crate::fec_mapping::FecMapping {
            politician_id: "P000003".to_string(),
            fec_candidate_id: "H0NY05080".to_string(),
            bioguide_id: "J000003".to_string(),
        }];

        db.upsert_fec_mappings(&mappings).expect("upsert");

        let pol_id = db
            .get_politician_id_for_bioguide("J000003")
            .expect("get_politician_id_for_bioguide");
        assert_eq!(pol_id, Some("P000003".to_string()));
    }

    #[test]
    fn test_get_politician_id_for_unknown_bioguide() {
        let db = open_test_db();

        let pol_id = db
            .get_politician_id_for_bioguide("X999999")
            .expect("get_politician_id for unknown bioguide");
        assert_eq!(pol_id, None, "Should return None for unknown bioguide");
    }

    #[test]
    fn test_upsert_fec_mappings_idempotent() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000004', 'FL', 'Republican', 'Bob', 'Brown', '1980-07-10', 'male', 'senate')",
                [],
            )
            .expect("insert politician");

        let mappings = vec![crate::fec_mapping::FecMapping {
            politician_id: "P000004".to_string(),
            fec_candidate_id: "S6FL00456".to_string(),
            bioguide_id: "B000004".to_string(),
        }];

        // First upsert
        let count1 = db.upsert_fec_mappings(&mappings).expect("first upsert");
        assert_eq!(count1, 1);

        // Second upsert (should update, not error)
        let count2 = db.upsert_fec_mappings(&mappings).expect("second upsert");
        assert_eq!(count2, 1);

        // Verify count is still 1 (idempotent)
        let total = db.count_fec_mappings().expect("count");
        assert_eq!(total, 1, "Upsert should be idempotent");
    }

    #[test]
    fn test_migrate_v4_fresh_db() {
        let db = Db::open_in_memory().expect("open");
        db.init().expect("init");

        let version: i32 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("get version");
        assert_eq!(version, 5);

        // Verify all three new tables exist
        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('fec_committees', 'donations', 'donation_sync_meta')")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(tables.len(), 3);
        assert!(tables.contains(&"fec_committees".to_string()));
        assert!(tables.contains(&"donations".to_string()));
        assert!(tables.contains(&"donation_sync_meta".to_string()));
    }

    #[test]
    fn test_migrate_v4_idempotent() {
        let db = Db::open_in_memory().expect("open");
        db.init().expect("first init");
        db.init().expect("second init should not fail");

        let version: i32 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("get version");
        assert_eq!(version, 5);
    }

    #[test]
    fn test_migrate_v4_from_v3() {
        let db = Db::open_in_memory().expect("open");

        // Manually set version to 3 and create fec_mappings without committee_ids
        db.conn.pragma_update(None, "user_version", 3).expect("set version");
        db.conn.execute(
            "CREATE TABLE IF NOT EXISTS fec_mappings (
                politician_id TEXT NOT NULL,
                fec_candidate_id TEXT NOT NULL,
                bioguide_id TEXT NOT NULL,
                election_cycle INTEGER,
                last_synced TEXT NOT NULL,
                PRIMARY KEY (politician_id, fec_candidate_id)
            )",
            [],
        ).expect("create fec_mappings v3");

        // Now run init which should migrate to v4
        db.init().expect("init");

        // Verify committee_ids column exists
        let has_column: bool = db
            .conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('fec_mappings') WHERE name='committee_ids'")
            .expect("prepare")
            .query_row([], |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            })
            .expect("query");
        assert!(has_column, "committee_ids column should exist after v4 migration");
    }

    #[test]
    fn test_migrate_v5_from_v4() {
        let db = Db::open_in_memory().expect("open");

        // Manually set version to 4
        db.conn.pragma_update(None, "user_version", 4).expect("set version");

        // Now run init which should migrate to v5
        db.init().expect("init");

        // Verify user_version is 5
        let version: i32 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("get version");
        assert_eq!(version, 5);

        // Verify employer_mappings table exists
        let has_employer_mappings: bool = db
            .conn
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='employer_mappings'")
            .expect("prepare")
            .query_row([], |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            })
            .expect("query");
        assert!(has_employer_mappings, "employer_mappings table should exist after v5 migration");

        // Verify employer_lookup table exists
        let has_employer_lookup: bool = db
            .conn
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='employer_lookup'")
            .expect("prepare")
            .query_row([], |row| {
                let count: i32 = row.get(0)?;
                Ok(count > 0)
            })
            .expect("query");
        assert!(has_employer_lookup, "employer_lookup table should exist after v5 migration");
    }

    #[test]
    fn test_fresh_db_has_employer_tables() {
        let db = Db::open_in_memory().expect("open");
        db.init().expect("init");

        // Verify both employer_mappings and employer_lookup tables exist
        let tables: Vec<String> = db
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('employer_mappings', 'employer_lookup')")
            .expect("prepare")
            .query_map([], |row| row.get(0))
            .expect("query")
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(tables.len(), 2);
        assert!(tables.contains(&"employer_mappings".to_string()));
        assert!(tables.contains(&"employer_lookup".to_string()));
    }

    #[test]
    fn test_migrate_v5_idempotent() {
        let db = Db::open_in_memory().expect("open");
        db.init().expect("first init");
        db.init().expect("second init should not fail");

        let version: i32 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("get version");
        assert_eq!(version, 5);
    }

    #[test]
    fn test_v5_version_check() {
        let db = Db::open_in_memory().expect("open");
        db.init().expect("init");

        let version: i32 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("get version");
        assert_eq!(version, 5, "fresh database should have version 5");
    }

    #[test]
    fn test_upsert_committee() {
        let db = open_test_db();

        let committee = crate::openfec::types::Committee {
            committee_id: "C00123456".to_string(),
            name: "Example Committee".to_string(),
            committee_type: Some("H".to_string()),
            designation: Some("P".to_string()),
            party: Some("DEM".to_string()),
            state: Some("CA".to_string()),
            cycles: vec![2020, 2022],
        };

        db.upsert_committee(&committee).expect("upsert");

        let row: (String, String, String, Vec<i32>) = db
            .conn
            .query_row(
                "SELECT committee_id, name, designation, cycles FROM fec_committees WHERE committee_id = ?1",
                params!["C00123456"],
                |row| {
                    let cycles_json: String = row.get(3)?;
                    let cycles: Vec<i32> = serde_json::from_str(&cycles_json).unwrap();
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, cycles))
                },
            )
            .expect("query");

        assert_eq!(row.0, "C00123456");
        assert_eq!(row.1, "Example Committee");
        assert_eq!(row.2, "P");
        assert_eq!(row.3, vec![2020, 2022]);
    }

    #[test]
    fn test_upsert_committee_update() {
        let db = open_test_db();

        let committee1 = crate::openfec::types::Committee {
            committee_id: "C00111111".to_string(),
            name: "Original Name".to_string(),
            committee_type: None,
            designation: None,
            party: None,
            state: None,
            cycles: vec![2020],
        };

        let committee2 = crate::openfec::types::Committee {
            committee_id: "C00111111".to_string(),
            name: "Updated Name".to_string(),
            committee_type: Some("H".to_string()),
            designation: Some("P".to_string()),
            party: Some("REP".to_string()),
            state: Some("TX".to_string()),
            cycles: vec![2020, 2022],
        };

        db.upsert_committee(&committee1).expect("first upsert");
        db.upsert_committee(&committee2).expect("second upsert");

        let name: String = db
            .conn
            .query_row(
                "SELECT name FROM fec_committees WHERE committee_id = ?1",
                params!["C00111111"],
                |row| row.get(0),
            )
            .expect("query");

        assert_eq!(name, "Updated Name");
    }

    #[test]
    fn test_upsert_committees_from_api() {
        let db = open_test_db();

        let committees = vec![
            crate::openfec::types::Committee {
                committee_id: "C00100001".to_string(),
                name: "Committee One".to_string(),
                committee_type: Some("H".to_string()),
                designation: Some("P".to_string()),
                party: Some("DEM".to_string()),
                state: Some("CA".to_string()),
                cycles: vec![2020, 2022],
            },
            crate::openfec::types::Committee {
                committee_id: "C00100002".to_string(),
                name: "Committee Two".to_string(),
                committee_type: Some("S".to_string()),
                designation: Some("A".to_string()),
                party: Some("REP".to_string()),
                state: Some("TX".to_string()),
                cycles: vec![2022],
            },
        ];

        let count = db.upsert_committees(&committees).expect("upsert");
        assert_eq!(count, 2);

        let db_count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM fec_committees", [], |row| row.get(0))
            .expect("count");
        assert_eq!(db_count, 2);
    }

    #[test]
    fn test_update_and_get_politician_committees() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000005', 'NY', 'Democrat', 'Test', 'Person', '1970-01-01', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert fec_mapping
        db.upsert_fec_mappings(&[crate::fec_mapping::FecMapping {
            politician_id: "P000005".to_string(),
            fec_candidate_id: "H0NY05080".to_string(),
            bioguide_id: "P000005".to_string(),
        }])
        .expect("upsert mapping");

        // Update committee_ids
        let committee_ids = vec!["C00123456".to_string(), "C00789012".to_string()];
        db.update_politician_committees("P000005", &committee_ids)
            .expect("update");

        // Get them back
        let retrieved = db
            .get_committees_for_politician("P000005")
            .expect("get")
            .expect("should have committees");

        assert_eq!(retrieved.len(), 2);
        assert!(retrieved.contains(&"C00123456".to_string()));
        assert!(retrieved.contains(&"C00789012".to_string()));
    }

    #[test]
    fn test_get_committees_null_returns_none() {
        let mut db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000006', 'CA', 'Republican', 'No', 'Committees', '1965-01-01', 'female', 'senate')",
                [],
            )
            .expect("insert politician");

        // Insert fec_mapping with no committee_ids
        db.upsert_fec_mappings(&[crate::fec_mapping::FecMapping {
            politician_id: "P000006".to_string(),
            fec_candidate_id: "S4CA00001".to_string(),
            bioguide_id: "C000006".to_string(),
        }])
        .expect("upsert mapping");

        let result = db.get_committees_for_politician("P000006").expect("get");
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_politician_info() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000007', 'TX', 'Democrat', 'First', 'Last', '1975-05-15', 'male', 'house')",
                [],
            )
            .expect("insert politician");

        let info = db
            .get_politician_info("P000007")
            .expect("get_politician_info")
            .expect("should exist");

        assert_eq!(info.0, "First");
        assert_eq!(info.1, "Last");
        assert_eq!(info.2, "TX");
    }

    #[test]
    fn test_get_politician_info_not_found() {
        let db = open_test_db();

        let info = db.get_politician_info("P999999").expect("get_politician_info");
        assert_eq!(info, None);
    }

    // ============================================================================
    // Donation Sync DB Operation Tests
    // ============================================================================

    #[test]
    fn test_insert_donation_new() {
        let db = open_test_db();

        let contribution = crate::openfec::types::Contribution {
            sub_id: Some("SUB123456".to_string()),
            committee: None,
            contributor_name: Some("John Donor".to_string()),
            contributor_state: Some("CA".to_string()),
            contributor_employer: Some("TechCorp".to_string()),
            contributor_occupation: Some("Engineer".to_string()),
            contribution_receipt_date: Some("2024-01-15".to_string()),
            contribution_receipt_amount: Some(2500.0),
        };

        let inserted = db
            .insert_donation(&contribution, "C00000001", Some(2024))
            .expect("insert_donation");
        assert!(inserted, "Should return true for new donation");

        // Verify donation was inserted
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM donations WHERE sub_id = 'SUB123456'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "Donation should exist in database");
    }

    #[test]
    fn test_insert_donation_duplicate() {
        let db = open_test_db();

        let contribution = crate::openfec::types::Contribution {
            sub_id: Some("SUB123456".to_string()),
            committee: None,
            contributor_name: Some("John Donor".to_string()),
            contributor_state: Some("CA".to_string()),
            contributor_employer: Some("TechCorp".to_string()),
            contributor_occupation: Some("Engineer".to_string()),
            contribution_receipt_date: Some("2024-01-15".to_string()),
            contribution_receipt_amount: Some(2500.0),
        };

        // First insert
        let inserted1 = db
            .insert_donation(&contribution, "C00000001", Some(2024))
            .expect("first insert");
        assert!(inserted1, "First insert should return true");

        // Second insert (duplicate)
        let inserted2 = db
            .insert_donation(&contribution, "C00000001", Some(2024))
            .expect("second insert");
        assert!(!inserted2, "Duplicate insert should return false");

        // Verify only one donation exists
        let count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM donations WHERE sub_id = 'SUB123456'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1, "Only one donation should exist");
    }

    #[test]
    fn test_insert_donation_null_sub_id() {
        let db = open_test_db();

        let contribution = crate::openfec::types::Contribution {
            sub_id: None, // NULL sub_id should be skipped
            committee: None,
            contributor_name: Some("John Donor".to_string()),
            contributor_state: Some("CA".to_string()),
            contributor_employer: Some("TechCorp".to_string()),
            contributor_occupation: Some("Engineer".to_string()),
            contribution_receipt_date: Some("2024-01-15".to_string()),
            contribution_receipt_amount: Some(2500.0),
        };

        let inserted = db
            .insert_donation(&contribution, "C00000001", Some(2024))
            .expect("insert_donation with NULL sub_id");
        assert!(!inserted, "Should return false for NULL sub_id");

        // Verify no donation was inserted
        let count: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM donations", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0, "No donation should exist for NULL sub_id");
    }

    #[test]
    fn test_load_sync_cursor_none() {
        let db = open_test_db();

        let cursor = db
            .load_sync_cursor("P000001", "C00000001")
            .expect("load_sync_cursor");
        assert_eq!(cursor, None, "Should return None for non-existent cursor");
    }

    #[test]
    fn test_save_and_load_cursor() {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000010', 'NY', 'Democrat', 'Test', 'Politician', '1970-01-01', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        let contributions = vec![crate::openfec::types::Contribution {
            sub_id: Some("SUB111".to_string()),
            committee: None,
            contributor_name: Some("Jane Donor".to_string()),
            contributor_state: Some("NY".to_string()),
            contributor_employer: None,
            contributor_occupation: None,
            contribution_receipt_date: Some("2024-02-01".to_string()),
            contribution_receipt_amount: Some(1000.0),
        }];

        let inserted = db
            .save_sync_cursor_with_donations(
                "P000010",
                "C00000002",
                &contributions,
                Some(2024),
                230880619,
                "2024-02-01",
            )
            .expect("save_sync_cursor_with_donations");
        assert_eq!(inserted, 1, "Should insert 1 donation");

        // Load cursor
        let cursor = db
            .load_sync_cursor("P000010", "C00000002")
            .expect("load_sync_cursor")
            .expect("cursor should exist");

        assert_eq!(cursor.0, 230880619, "last_index should match");
        assert_eq!(
            cursor.1, "2024-02-01",
            "last_contribution_receipt_date should match"
        );
    }

    #[test]
    fn test_save_cursor_increments_total() {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000011', 'CA', 'Republican', 'Test', 'Two', '1975-05-05', 'male', 'senate')",
                [],
            )
            .expect("insert politician");

        // First batch
        let contributions1 = vec![crate::openfec::types::Contribution {
            sub_id: Some("SUB222".to_string()),
            committee: None,
            contributor_name: Some("Alice".to_string()),
            contributor_state: Some("CA".to_string()),
            contributor_employer: None,
            contributor_occupation: None,
            contribution_receipt_date: Some("2024-03-01".to_string()),
            contribution_receipt_amount: Some(500.0),
        }];

        let inserted1 = db
            .save_sync_cursor_with_donations(
                "P000011",
                "C00000003",
                &contributions1,
                Some(2024),
                100,
                "2024-03-01",
            )
            .expect("first save");
        assert_eq!(inserted1, 1);

        // Second batch
        let contributions2 = vec![crate::openfec::types::Contribution {
            sub_id: Some("SUB333".to_string()),
            committee: None,
            contributor_name: Some("Bob".to_string()),
            contributor_state: Some("TX".to_string()),
            contributor_employer: None,
            contributor_occupation: None,
            contribution_receipt_date: Some("2024-03-02".to_string()),
            contribution_receipt_amount: Some(750.0),
        }];

        let inserted2 = db
            .save_sync_cursor_with_donations(
                "P000011",
                "C00000003",
                &contributions2,
                Some(2024),
                200,
                "2024-03-02",
            )
            .expect("second save");
        assert_eq!(inserted2, 1);

        // Verify total_synced accumulated
        let total: i64 = db
            .conn
            .query_row(
                "SELECT total_synced FROM donation_sync_meta WHERE politician_id = 'P000011' AND committee_id = 'C00000003'",
                [],
                |row| row.get(0),
            )
            .expect("get total_synced");
        assert_eq!(total, 2, "total_synced should accumulate");
    }

    #[test]
    fn test_save_cursor_transaction_atomicity() {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000012', 'FL', 'Democrat', 'Atomic', 'Test', '1980-01-01', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        let contributions = vec![crate::openfec::types::Contribution {
            sub_id: Some("SUB444".to_string()),
            committee: None,
            contributor_name: Some("Charlie".to_string()),
            contributor_state: Some("FL".to_string()),
            contributor_employer: None,
            contributor_occupation: None,
            contribution_receipt_date: Some("2024-04-01".to_string()),
            contribution_receipt_amount: Some(2000.0),
        }];

        db.save_sync_cursor_with_donations(
            "P000012",
            "C00000004",
            &contributions,
            Some(2024),
            300,
            "2024-04-01",
        )
        .expect("save");

        // Verify both donation and cursor exist (atomic transaction)
        let donation_count: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM donations WHERE sub_id = 'SUB444'",
                [],
                |row| row.get(0),
            )
            .expect("count donations");
        assert_eq!(donation_count, 1, "Donation should exist");

        let cursor = db
            .load_sync_cursor("P000012", "C00000004")
            .expect("load cursor")
            .expect("cursor should exist");
        assert_eq!(cursor.0, 300, "Cursor should exist with correct last_index");
    }

    #[test]
    fn test_mark_sync_completed() {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000013', 'TX', 'Republican', 'Complete', 'Sync', '1985-01-01', 'male', 'senate')",
                [],
            )
            .expect("insert politician");

        // Set up initial cursor
        let contributions = vec![crate::openfec::types::Contribution {
            sub_id: Some("SUB555".to_string()),
            committee: None,
            contributor_name: Some("Dave".to_string()),
            contributor_state: Some("TX".to_string()),
            contributor_employer: None,
            contributor_occupation: None,
            contribution_receipt_date: Some("2024-05-01".to_string()),
            contribution_receipt_amount: Some(1500.0),
        }];

        db.save_sync_cursor_with_donations(
            "P000013",
            "C00000005",
            &contributions,
            Some(2024),
            400,
            "2024-05-01",
        )
        .expect("save");

        // Mark completed
        db.mark_sync_completed("P000013", "C00000005")
            .expect("mark_sync_completed");

        // Verify last_index is NULL
        let cursor = db.load_sync_cursor("P000013", "C00000005").expect("load");
        assert_eq!(cursor, None, "Cursor should return None when completed (last_index NULL)");

        // Verify total_synced is preserved
        let total: i64 = db
            .conn
            .query_row(
                "SELECT total_synced FROM donation_sync_meta WHERE politician_id = 'P000013' AND committee_id = 'C00000005'",
                [],
                |row| row.get(0),
            )
            .expect("get total_synced");
        assert_eq!(total, 1, "total_synced should be preserved");
    }

    #[test]
    fn test_find_politician_by_name_found() {
        let db = open_test_db();

        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000014', 'CA', 'Democrat', 'Nancy', 'Pelosi', '1940-03-26', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        let matches = db
            .find_politician_by_name("Pelosi")
            .expect("find_politician_by_name");
        assert_eq!(matches.len(), 1, "Should find 1 match");
        assert_eq!(matches[0].0, "P000014");
        assert_eq!(matches[0].1, "Nancy Pelosi");
    }

    #[test]
    fn test_find_politician_by_name_multiple() {
        let db = open_test_db();

        db.conn
            .execute_batch(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000015', 'CA', 'Democrat', 'Nancy', 'Pelosi', '1940-03-26', 'female', 'house');
                 INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000016', 'NY', 'Republican', 'John', 'Pelosi', '1965-01-01', 'male', 'senate');",
            )
            .expect("insert politicians");

        let matches = db
            .find_politician_by_name("Pelosi")
            .expect("find_politician_by_name");
        assert_eq!(matches.len(), 2, "Should find 2 matches");
    }

    #[test]
    fn test_find_politician_by_name_not_found() {
        let db = open_test_db();

        let matches = db
            .find_politician_by_name("Nonexistent")
            .expect("find_politician_by_name");
        assert_eq!(matches.len(), 0, "Should return empty Vec for no matches");
    }

    #[test]
    fn test_count_donations_for_politician() {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000017', 'MA', 'Democrat', 'Count', 'Test', '1975-01-01', 'female', 'senate')",
                [],
            )
            .expect("insert politician");

        let contributions = vec![
            crate::openfec::types::Contribution {
                sub_id: Some("SUB666".to_string()),
                committee: None,
                contributor_name: Some("Eve".to_string()),
                contributor_state: Some("MA".to_string()),
                contributor_employer: None,
                contributor_occupation: None,
                contribution_receipt_date: Some("2024-06-01".to_string()),
                contribution_receipt_amount: Some(100.0),
            },
            crate::openfec::types::Contribution {
                sub_id: Some("SUB777".to_string()),
                committee: None,
                contributor_name: Some("Frank".to_string()),
                contributor_state: Some("MA".to_string()),
                contributor_employer: None,
                contributor_occupation: None,
                contribution_receipt_date: Some("2024-06-02".to_string()),
                contribution_receipt_amount: Some(200.0),
            },
        ];

        db.save_sync_cursor_with_donations(
            "P000017",
            "C00000006",
            &contributions,
            Some(2024),
            500,
            "2024-06-02",
        )
        .expect("save");

        let count = db
            .count_donations_for_politician("P000017")
            .expect("count_donations_for_politician");
        assert_eq!(count, 2, "Should count 2 donations");
    }

    /// Setup helper for donation query tests.
    /// Creates test politician, committees, sync_meta, and sample donations.
    fn setup_donation_query_test_db() -> Db {
        let db = open_test_db();

        // Insert test politician
        db.conn
            .execute(
                "INSERT INTO politicians (politician_id, state_id, party, first_name, last_name, dob, gender, chamber)
                 VALUES ('P000001', 'CA', 'Democrat', 'Nancy', 'Pelosi', '1940-03-26', 'female', 'house')",
                [],
            )
            .expect("insert politician");

        // Insert test committee
        db.conn
            .execute(
                "INSERT INTO fec_committees (committee_id, name, designation, committee_type, last_synced)
                 VALUES ('C00001', 'Pelosi for Congress', 'P', 'H', '2024-01-01T00:00:00Z')",
                [],
            )
            .expect("insert committee");

        // Insert sync_meta to link politician to committee
        db.conn
            .execute(
                "INSERT INTO donation_sync_meta (politician_id, committee_id, last_synced_at, total_synced)
                 VALUES ('P000001', 'C00001', '2024-01-01T00:00:00Z', 6)",
                [],
            )
            .expect("insert sync_meta");

        // Insert 6 test donations with varying attributes
        let donations = [
            ("SUB001", Some("Alice Smith"), Some("Tech Corp"), Some("CA"), 500.0, "2024-01-15", 2024),
            ("SUB002", Some("Bob Jones"), Some("Finance LLC"), Some("NY"), 1000.0, "2024-02-10", 2024),
            ("SUB003", None, Some("Unknown Employer"), Some("CA"), 250.0, "2024-03-05", 2024), // NULL name
            ("SUB004", Some("Charlie Brown"), Some("Tech Corp"), Some("TX"), 750.0, "2024-01-20", 2022),
            ("SUB005", Some("Alice Smith"), Some("Tech Corp"), Some("CA"), 300.0, "2024-04-01", 2024), // duplicate contributor
            ("SUB006", Some("Diana Prince"), Some("Legal Services"), Some("NY"), 100.0, "2024-05-15", 2024),
        ];

        for (sub_id, name, employer, state, amount, date, cycle) in donations {
            db.conn
                .execute(
                    "INSERT INTO donations (sub_id, committee_id, contributor_name, contributor_employer, contributor_state, contribution_receipt_amount, contribution_receipt_date, election_cycle)
                     VALUES (?1, 'C00001', ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![sub_id, name, employer, state, amount, date, cycle],
                )
                .expect("insert donation");
        }

        db
    }

    #[test]
    fn test_query_donations_no_filter() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter::default();

        let rows = db.query_donations(&filter).expect("query_donations");

        assert_eq!(rows.len(), 6, "Should return all 6 donations");
        // First row should be highest amount (sorted DESC)
        assert_eq!(rows[0].amount, 1000.0);
        assert_eq!(rows[0].contributor_name, "Bob Jones");
    }

    #[test]
    fn test_query_donations_with_politician_filter() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter {
            politician_id: Some("P000001".to_string()),
            ..Default::default()
        };

        let rows = db.query_donations(&filter).expect("query_donations");

        assert_eq!(rows.len(), 6, "Should return all donations for P000001");
        // Verify all rows have correct politician
        assert!(rows.iter().all(|r| r.politician_name == "Nancy Pelosi"));
    }

    #[test]
    fn test_query_donations_with_cycle_filter() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter {
            cycle: Some(2024),
            ..Default::default()
        };

        let rows = db.query_donations(&filter).expect("query_donations");

        assert_eq!(rows.len(), 5, "Should return 5 donations for cycle 2024");
        assert!(rows.iter().all(|r| r.cycle == 2024));
    }

    #[test]
    fn test_query_donations_with_min_amount() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter {
            min_amount: Some(500.0),
            ..Default::default()
        };

        let rows = db.query_donations(&filter).expect("query_donations");

        assert_eq!(rows.len(), 3, "Should return 3 donations >= $500");
        assert!(rows.iter().all(|r| r.amount >= 500.0));
    }

    #[test]
    fn test_query_donations_with_limit() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter {
            limit: Some(2),
            ..Default::default()
        };

        let rows = db.query_donations(&filter).expect("query_donations");

        assert_eq!(rows.len(), 2, "Should respect LIMIT 2");
        // Should be top 2 by amount
        assert_eq!(rows[0].amount, 1000.0);
        assert_eq!(rows[1].amount, 750.0);
    }

    #[test]
    fn test_query_donations_null_handling() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter::default();

        let rows = db.query_donations(&filter).expect("query_donations");

        // Find the row with NULL contributor_name (SUB003)
        let null_row = rows.iter().find(|r| r.sub_id == "SUB003").expect("find SUB003");
        assert_eq!(null_row.contributor_name, "Unknown", "NULL name should become 'Unknown'");
    }

    #[test]
    fn test_query_donations_by_contributor() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter::default();

        let rows = db.query_donations_by_contributor(&filter).expect("query_donations_by_contributor");

        // Should group by (name, state) - expect 5 unique contributors
        // Alice Smith CA: $800 (2 donations)
        // Bob Jones NY: $1000 (1 donation)
        // Unknown (NULL): $250 (1 donation)
        // Charlie Brown TX: $750 (1 donation)
        // Diana Prince NY: $100 (1 donation)
        assert_eq!(rows.len(), 5, "Should return 5 unique contributors");

        // First row should be Bob Jones with highest total
        assert_eq!(rows[0].contributor_name, "Bob Jones");
        assert_eq!(rows[0].total_amount, 1000.0);
        assert_eq!(rows[0].donation_count, 1);

        // Find Alice Smith who has 2 donations
        let alice = rows.iter().find(|r| r.contributor_name == "Alice Smith").expect("find Alice");
        assert_eq!(alice.total_amount, 800.0, "Alice Smith should have $800 total (500 + 300)");
        assert_eq!(alice.donation_count, 2, "Alice Smith should have 2 donations");
        assert_eq!(alice.max_donation, 500.0);
        assert_eq!(alice.avg_amount, 400.0);
    }

    #[test]
    fn test_query_donations_by_employer() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter::default();

        let rows = db.query_donations_by_employer(&filter).expect("query_donations_by_employer");

        // Tech Corp: $1550 (3 donations from Alice x2 + Charlie)
        // Finance LLC: $1000 (1 donation)
        // Unknown Employer: $250 (1 donation)
        // Legal Services: $100 (1 donation)
        assert_eq!(rows.len(), 4, "Should return 4 unique employers");

        // First row should be Tech Corp with highest total
        let tech = &rows[0];
        assert_eq!(tech.employer, "Tech Corp");
        assert_eq!(tech.total_amount, 1550.0, "Tech Corp should have $1550 total");
        assert_eq!(tech.donation_count, 3, "Tech Corp should have 3 donations");
        assert_eq!(tech.contributor_count, 2, "Tech Corp should have 2 distinct contributors (Alice, Charlie)");
    }

    #[test]
    fn test_query_donations_by_state() {
        let db = setup_donation_query_test_db();
        let filter = DonationFilter::default();

        let rows = db.query_donations_by_state(&filter).expect("query_donations_by_state");

        // CA: $1050 (3 donations: Alice $500 + NULL $250 + Alice $300)
        // NY: $1100 (2 donations: Bob $1000 + Diana $100)
        // TX: $750 (1 donation: Charlie $750)
        assert_eq!(rows.len(), 3, "Should return 3 unique states");

        // First row should be NY with highest total
        let ny = &rows[0];
        assert_eq!(ny.state, "NY");
        assert_eq!(ny.total_amount, 1100.0);
        assert_eq!(ny.donation_count, 2);
        assert_eq!(ny.contributor_count, 2, "NY should have 2 distinct contributors (Bob, Diana)");

        // CA should be second
        let ca = &rows[1];
        assert_eq!(ca.state, "CA");
        assert_eq!(ca.total_amount, 1050.0);
        assert_eq!(ca.donation_count, 3);
        assert_eq!(ca.contributor_count, 1, "CA should have 1 distinct named contributor (Alice, NULL doesn't count)");
    }
}
