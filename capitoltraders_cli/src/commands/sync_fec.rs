//! The `sync-fec` subcommand: populate FEC candidate ID mappings from congress-legislators dataset.

use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::Result;
use capitoltraders_lib::{Db, download_legislators, match_legislators_to_politicians};
use clap::Args;

#[derive(Args)]
pub struct SyncFecArgs {
    /// Path to the SQLite database
    #[arg(long)]
    pub db: PathBuf,
}

pub async fn run(args: &SyncFecArgs) -> Result<()> {
    let mut db = Db::open(&args.db)?;
    db.init()?;

    // Step 1: Get existing politicians from DB for matching
    let politicians = db.get_politicians_for_fec_matching()?;
    if politicians.is_empty() {
        println!("No politicians found in database. Run 'capitoltraders sync --db {}' first to import politician data.", args.db.display());
        return Ok(());
    }
    println!("Found {} politicians in database", politicians.len());

    // Step 2: Download congress-legislators dataset
    println!("Downloading congress-legislators dataset...");
    let client = reqwest::Client::new();
    let legislators = download_legislators(&client).await?;
    println!("Loaded {} legislators from dataset", legislators.len());

    // Step 3: Match legislators to politicians
    let mappings = match_legislators_to_politicians(&legislators, &politicians);
    println!("Matched {} FEC ID mappings", mappings.len());

    if mappings.is_empty() {
        println!("No matches found. This may indicate the database has no overlapping politicians with the congress-legislators dataset.");
        return Ok(());
    }

    // Step 4: Persist to database
    let count = db.upsert_fec_mappings(&mappings)?;
    println!("Stored {} FEC ID mappings in database", count);

    // Step 5: Summary stats
    let total = db.count_fec_mappings()?;
    let unique_politicians: HashSet<&str> = mappings.iter()
        .map(|m| m.politician_id.as_str())
        .collect();
    println!(
        "\nSync complete: {} total mappings for {} unique politicians",
        total, unique_politicians.len()
    );

    Ok(())
}
