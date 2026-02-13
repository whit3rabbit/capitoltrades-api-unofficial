//! The `donations` subcommand: queries synced FEC donation data.

use anyhow::{bail, Result};
use capitoltraders_lib::{validation, Db, DonationFilter};
use clap::Args;
use std::path::PathBuf;

use crate::output::{
    print_contributor_agg_csv, print_contributor_agg_markdown, print_contributor_agg_table,
    print_contributor_agg_xml, print_donations_csv, print_donations_markdown,
    print_donations_table, print_donations_xml, print_employer_agg_csv,
    print_employer_agg_markdown, print_employer_agg_table, print_employer_agg_xml, print_json,
    print_state_agg_csv, print_state_agg_markdown, print_state_agg_table, print_state_agg_xml,
    OutputFormat,
};

/// Arguments for the `donations` subcommand.
///
/// Displays FEC donation data from the local SQLite database.
/// Requires a synced database with FEC mappings and donation data.
#[derive(Args)]
pub struct DonationsArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Filter by politician name (partial match, resolves to politician_id)
    #[arg(long)]
    pub politician: Option<String>,

    /// Filter by election cycle year (e.g., 2024)
    #[arg(long)]
    pub cycle: Option<i32>,

    /// Minimum contribution amount in dollars
    #[arg(long)]
    pub min_amount: Option<f64>,

    /// Filter by employer name (partial match)
    #[arg(long)]
    pub employer: Option<String>,

    /// Filter by contributor state (e.g., CA, TX)
    #[arg(long)]
    pub state: Option<String>,

    /// Show top N results
    #[arg(long)]
    pub top: Option<i64>,

    /// Group results by: contributor, employer, state
    #[arg(long)]
    pub group_by: Option<String>,
}

pub fn run(args: &DonationsArgs, format: &OutputFormat) -> Result<()> {
    let db = Db::open(&args.db)?;

    // Resolve politician name to ID if provided
    let politician_id = if let Some(ref name) = args.politician {
        let matches = db.find_politician_by_name(name)?;
        match matches.len() {
            0 => bail!("No politician found matching '{}'", name),
            1 => Some(matches[0].0.clone()),
            _ => {
                let names: Vec<String> = matches.iter().map(|m| m.1.clone()).collect();
                bail!(
                    "Multiple politicians match '{}': {}. Please be more specific.",
                    name,
                    names.join(", ")
                );
            }
        }
    } else {
        None
    };

    // Validate state if provided
    let state = match args.state {
        Some(ref val) => Some(validation::validate_state(val.trim())?.to_string()),
        None => None,
    };

    // Validate cycle if provided
    if let Some(cycle) = args.cycle {
        if cycle < 1976 || cycle % 2 != 0 {
            bail!("Invalid cycle: must be an even year >= 1976");
        }
    }

    // Validate min_amount if provided
    if let Some(amount) = args.min_amount {
        if amount < 0.0 {
            bail!("--min-amount must be non-negative");
        }
    }

    // Validate top if provided
    if let Some(top) = args.top {
        if top <= 0 {
            bail!("--top must be a positive integer");
        }
    }

    // Validate group_by if provided
    if let Some(ref group_by) = args.group_by {
        let normalized = group_by.trim().to_lowercase();
        if !matches!(normalized.as_str(), "contributor" | "employer" | "state") {
            bail!(
                "Invalid --group-by value: '{}'. Valid options: contributor, employer, state",
                group_by
            );
        }
    }

    // Build filter
    let filter = DonationFilter {
        politician_id,
        cycle: args.cycle,
        min_amount: args.min_amount,
        employer: args.employer.clone(),
        contributor_state: state,
        limit: args.top,
    };

    // Dispatch based on group_by
    match args.group_by.as_deref() {
        None => {
            // Individual listing
            let donations = db.query_donations(&filter)?;
            if donations.is_empty() {
                eprintln!("No donations found matching the given filters.");
                eprintln!("Hint: Run 'capitoltraders sync-fec' and 'capitoltraders sync-donations' first.");
                return Ok(());
            }
            match format {
                OutputFormat::Table => print_donations_table(&donations),
                OutputFormat::Json => print_json(&donations),
                OutputFormat::Csv => print_donations_csv(&donations)?,
                OutputFormat::Markdown => print_donations_markdown(&donations),
                OutputFormat::Xml => print_donations_xml(&donations),
            }
        }
        Some("contributor") => {
            // Contributor aggregation
            let rows = db.query_donations_by_contributor(&filter)?;
            if rows.is_empty() {
                eprintln!("No donations found matching the given filters.");
                eprintln!("Hint: Run 'capitoltraders sync-fec' and 'capitoltraders sync-donations' first.");
                return Ok(());
            }
            match format {
                OutputFormat::Table => print_contributor_agg_table(&rows),
                OutputFormat::Json => print_json(&rows),
                OutputFormat::Csv => print_contributor_agg_csv(&rows)?,
                OutputFormat::Markdown => print_contributor_agg_markdown(&rows),
                OutputFormat::Xml => print_contributor_agg_xml(&rows),
            }
        }
        Some("employer") => {
            // Employer aggregation
            let rows = db.query_donations_by_employer(&filter)?;
            if rows.is_empty() {
                eprintln!("No donations found matching the given filters.");
                eprintln!("Hint: Run 'capitoltraders sync-fec' and 'capitoltraders sync-donations' first.");
                return Ok(());
            }
            match format {
                OutputFormat::Table => print_employer_agg_table(&rows),
                OutputFormat::Json => print_json(&rows),
                OutputFormat::Csv => print_employer_agg_csv(&rows)?,
                OutputFormat::Markdown => print_employer_agg_markdown(&rows),
                OutputFormat::Xml => print_employer_agg_xml(&rows),
            }
        }
        Some("state") => {
            // State aggregation
            let rows = db.query_donations_by_state(&filter)?;
            if rows.is_empty() {
                eprintln!("No donations found matching the given filters.");
                eprintln!("Hint: Run 'capitoltraders sync-fec' and 'capitoltraders sync-donations' first.");
                return Ok(());
            }
            match format {
                OutputFormat::Table => print_state_agg_table(&rows),
                OutputFormat::Json => print_json(&rows),
                OutputFormat::Csv => print_state_agg_csv(&rows)?,
                OutputFormat::Markdown => print_state_agg_markdown(&rows),
                OutputFormat::Xml => print_state_agg_xml(&rows),
            }
        }
        _ => unreachable!("group_by validated above"),
    }

    Ok(())
}
