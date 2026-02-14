//! The `map-employers` subcommand: export/import/load-seed for employer-to-issuer mappings.

use anyhow::{bail, Result};
use capitoltraders_lib::{
    employer_mapping::{is_blacklisted, load_seed_data, match_employer, normalize_employer},
    Db,
};
use clap::{Args, Subcommand};
use csv::{Reader, Writer};
use serde::Serialize;
use std::path::PathBuf;

use crate::output::sanitize_csv_field;

/// Arguments for the `map-employers` subcommand.
#[derive(Args)]
pub struct MapEmployersArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    #[command(subcommand)]
    pub action: MapEmployersAction,
}

#[derive(Subcommand)]
pub enum MapEmployersAction {
    /// Export unmatched employers with fuzzy match suggestions to CSV
    Export(ExportArgs),
    /// Import confirmed employer-to-issuer mappings from CSV
    Import(ImportArgs),
    /// Load seed data (curated employer mappings) into database
    LoadSeed(LoadSeedArgs),
}

#[derive(Args)]
pub struct ExportArgs {
    /// Output CSV file path
    #[arg(long, short = 'o')]
    pub output: PathBuf,

    /// Jaro-Winkler fuzzy match threshold (0.0-1.0)
    #[arg(long, default_value = "0.85")]
    pub threshold: f64,

    /// Maximum number of unmatched employers to export
    #[arg(long)]
    pub limit: Option<i64>,
}

#[derive(Args)]
pub struct ImportArgs {
    /// Input CSV file path with confirmed mappings
    #[arg(long, short = 'i')]
    pub input: PathBuf,
}

#[derive(Args)]
pub struct LoadSeedArgs {
    /// Dry run: show what would be loaded without writing to DB
    #[arg(long)]
    pub dry_run: bool,
}

/// CSV export row structure for clean serialization.
#[derive(Serialize)]
struct ExportRow {
    employer: String,
    normalized: String,
    suggestion_ticker: String,
    suggestion_name: String,
    suggestion_sector: String,
    confidence: String,
    confirmed_ticker: String,
    notes: String,
}

pub fn run(args: &MapEmployersArgs) -> Result<()> {
    let db = Db::open(&args.db)?;
    db.init()?;

    match &args.action {
        MapEmployersAction::Export(export_args) => run_export(&db, export_args),
        MapEmployersAction::Import(import_args) => run_import(&db, import_args),
        MapEmployersAction::LoadSeed(seed_args) => run_load_seed(&db, seed_args),
    }
}

fn run_export(db: &Db, args: &ExportArgs) -> Result<()> {
    // Validate threshold
    if !(0.0..=1.0).contains(&args.threshold) {
        bail!("Threshold must be between 0.0 and 1.0");
    }

    // Get unmatched employers
    let unmatched = db.get_unmatched_employers(args.limit)?;
    if unmatched.is_empty() {
        eprintln!("No unmatched employers found. Run 'capitoltraders sync-donations' first to populate donation data.");
        return Ok(());
    }

    // Get all issuers for matching
    let issuers = db.get_all_issuers_for_matching()?;
    if issuers.is_empty() {
        eprintln!("No issuers in database. Run 'capitoltraders sync' first.");
        return Ok(());
    }

    // Process each employer
    let mut rows = Vec::new();
    let mut suggestion_count = 0;

    for employer in &unmatched {
        // Skip blacklisted employers
        if is_blacklisted(employer) {
            continue;
        }

        let normalized = normalize_employer(employer);

        // Match with configurable threshold
        let match_result = match_employer(employer, &issuers, args.threshold);

        let (ticker, name, sector, confidence) = if let Some(result) = match_result {
            suggestion_count += 1;
            (
                result.issuer_ticker.clone(),
                result.issuer_name.clone(),
                String::new(), // Sector not available from issuer data
                format!("{:.2}", result.confidence),
            )
        } else {
            (String::new(), String::new(), String::new(), String::new())
        };

        rows.push(ExportRow {
            employer: sanitize_csv_field(employer),
            normalized: normalized.clone(),
            suggestion_ticker: ticker,
            suggestion_name: name,
            suggestion_sector: sector,
            confidence,
            confirmed_ticker: String::new(),
            notes: String::new(),
        });
    }

    // Write CSV
    let mut wtr = Writer::from_path(&args.output)?;
    for row in &rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;

    eprintln!(
        "Exported {} unmatched employers to {}. {} had suggestions above threshold {:.2}.",
        rows.len(),
        args.output.display(),
        suggestion_count,
        args.threshold
    );

    Ok(())
}

fn run_import(db: &Db, args: &ImportArgs) -> Result<()> {
    let mut rdr = Reader::from_path(&args.input)?;

    let mut mappings = Vec::new();
    let mut lookups = Vec::new();
    let mut skipped = 0;

    for result in rdr.records() {
        let record = result?;

        // Read confirmed_ticker column (index 6)
        let confirmed_ticker = record.get(6).unwrap_or("").trim();
        if confirmed_ticker.is_empty() {
            skipped += 1;
            continue;
        }

        // Validate issuer exists
        if !db.issuer_exists_by_ticker(confirmed_ticker)? {
            eprintln!(
                "Warning: Ticker '{}' not found in database. Skipping.",
                confirmed_ticker
            );
            skipped += 1;
            continue;
        }

        // Read columns: employer (0), normalized (1)
        let employer = record.get(0).unwrap_or("").trim();
        let normalized = record.get(1).unwrap_or("").trim();

        if employer.is_empty() || normalized.is_empty() {
            skipped += 1;
            continue;
        }

        // Collect mapping: (normalized_employer, confirmed_ticker, confidence, match_type)
        mappings.push((
            normalized.to_string(),
            confirmed_ticker.to_string(),
            1.0,
            "manual",
        ));

        // Collect lookup: (raw_employer_lower, normalized_employer)
        lookups.push((employer.to_lowercase(), normalized.to_string()));
    }

    // Persist to database
    let mapping_count = db.upsert_employer_mappings(&mappings)?;
    db.insert_employer_lookups(&lookups)?;

    eprintln!(
        "Imported {} confirmed employer mappings. Skipped {} (no confirmed ticker or invalid ticker).",
        mapping_count,
        skipped
    );

    Ok(())
}

fn run_load_seed(db: &Db, args: &LoadSeedArgs) -> Result<()> {
    let seed_data = load_seed_data()?;

    let mut mappings = Vec::new();
    let mut lookups = Vec::new();
    let mut skipped = 0;
    let mut issuer_count = 0;

    for seed in &seed_data {
        // Check if ticker exists in database
        if !db.issuer_exists_by_ticker(&seed.issuer_ticker)? {
            eprintln!(
                "Warning: Ticker '{}' not found in database. Skipping {} employer variant(s). (User may not have synced this issuer yet.)",
                seed.issuer_ticker,
                seed.employer_names.len()
            );
            skipped += seed.employer_names.len();
            continue;
        }

        issuer_count += 1;

        for employer_name in &seed.employer_names {
            let normalized = normalize_employer(employer_name);

            // Collect mapping
            mappings.push((
                normalized.clone(),
                seed.issuer_ticker.clone(),
                seed.confidence,
                "exact",
            ));

            // Collect lookup
            lookups.push((
                employer_name.to_lowercase().trim().to_string(),
                normalized,
            ));
        }
    }

    if args.dry_run {
        eprintln!("DRY RUN: Would load {} mappings for {} issuers.", mappings.len(), issuer_count);
        eprintln!("Would skip {} (ticker not in database).", skipped);
        return Ok(());
    }

    // Persist to database
    db.upsert_employer_mappings(&mappings)?;
    db.insert_employer_lookups(&lookups)?;

    eprintln!(
        "Loaded {} seed mappings for {} issuers. Skipped {} (ticker not in database).",
        mappings.len(),
        issuer_count,
        skipped
    );

    Ok(())
}
