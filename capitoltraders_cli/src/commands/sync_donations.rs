//! Donation sync pipeline for fetching FEC Schedule A contributions.
//!
//! Fetches contributions for politicians' committees and stores them in SQLite.
//! Uses Semaphore + JoinSet + mpsc pattern for concurrent fetching with rate limiting.

use anyhow::{bail, Result};
use capitoltraders_lib::{
    committee::CommitteeResolver,
    openfec::{
        types::{Contribution, ScheduleAQuery},
        OpenFecClient, OpenFecError,
    },
    Db,
};
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;
use tokio::time::sleep;

/// Donation sync CLI arguments.
#[derive(Args)]
pub struct SyncDonationsArgs {
    /// SQLite database path (required)
    #[arg(long)]
    pub db: PathBuf,

    /// Politician name to sync donations for (searches by partial match)
    #[arg(long)]
    pub politician: Option<String>,

    /// Election cycle year (e.g. 2024). If omitted, syncs all available cycles.
    #[arg(long)]
    pub cycle: Option<i32>,

    /// Donations per API page (default: 100)
    #[arg(long, default_value = "100")]
    pub batch_size: i32,
}

/// Message sent from fetch tasks to receiver.
enum DonationMessage {
    Page {
        politician_id: String,
        committee_id: String,
        contributions: Vec<Contribution>,
        cycle: Option<i32>,
        last_index: i64,
        last_date: String,
    },
    Completed {
        politician_id: String,
        committee_id: String,
    },
    Error {
        committee_id: String,
        error: OpenFecError,
    },
}

/// Circuit breaker to stop processing after consecutive failures.
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
}

impl CircuitBreaker {
    fn new(threshold: usize) -> Self {
        Self {
            consecutive_failures: 0,
            threshold,
        }
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
    }

    fn record_failure(&mut self) {
        self.consecutive_failures += 1;
    }

    fn is_tripped(&self) -> bool {
        self.consecutive_failures >= self.threshold
    }
}

/// Run the donation sync pipeline.
pub async fn run(args: &SyncDonationsArgs, api_key: String) -> Result<()> {
    let start_time = Instant::now();

    // Step 1: Setup - Open DB for politician lookup and committee resolution
    let setup_db = Db::open(&args.db)?;
    setup_db.init()?;

    let client = Arc::new(OpenFecClient::new(api_key)?);
    let resolver = CommitteeResolver::new(
        Arc::clone(&client),
        Arc::new(Mutex::new(Db::open(&args.db)?)),
    );

    // Step 2: Politician resolution
    let politicians: Vec<(String, String)> = if let Some(ref name) = args.politician {
        // Search for politician by name
        let matches: Vec<(String, String)> = setup_db
            .conn()
            .prepare("SELECT politician_id, first_name || ' ' || last_name FROM politicians WHERE first_name || ' ' || last_name LIKE ?1")?
            .query_map([format!("%{}%", name)], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if matches.is_empty() {
            bail!("No politician found matching '{}'", name);
        } else if matches.len() > 1 {
            eprintln!("Multiple politicians match '{}':", name);
            for (id, full_name) in &matches {
                eprintln!("  - {} ({})", full_name, id);
            }
            bail!(
                "Please be more specific (matched {} politicians)",
                matches.len()
            );
        }
        matches
    } else {
        // Get all politicians with FEC mappings
        setup_db
            .conn()
            .prepare(
                "SELECT DISTINCT p.politician_id, p.first_name || ' ' || p.last_name
                 FROM politicians p
                 JOIN fec_mappings fm ON p.politician_id = fm.politician_id",
            )?
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };

    if politicians.is_empty() {
        eprintln!("No politicians with FEC mappings found");
        return Ok(());
    }

    eprintln!(
        "Starting donation sync for {} politician(s)",
        politicians.len()
    );

    // Step 3: For each politician, resolve committees and prepare sync tasks
    type CommitteeTask = (String, String, String, Option<(i64, String)>);
    let mut committee_tasks: Vec<CommitteeTask> = Vec::new();

    for (politician_id, politician_name) in &politicians {
        // Resolve committees
        let committees = resolver.resolve_committees(politician_id).await?;

        if committees.is_empty() {
            eprintln!(
                "Warning: No committees found for {} ({}), skipping",
                politician_name, politician_id
            );
            continue;
        }

        eprintln!(
            "  {} ({}): {} committee(s)",
            politician_name,
            politician_id,
            committees.len()
        );

        // For each committee, check for existing cursor
        for committee in &committees {
            // Load cursor from DB (before spawning tasks)
            let cursor = setup_db.load_sync_cursor(politician_id, &committee.committee_id)?;

            // Check if sync completed recently (within 24 hours)
            if cursor.is_none() {
                let completed_recently: bool = setup_db.conn()
                    .query_row(
                        "SELECT EXISTS(
                            SELECT 1 FROM donation_sync_meta
                            WHERE politician_id = ?1
                              AND committee_id = ?2
                              AND last_index IS NULL
                              AND datetime(last_synced_at) > datetime('now', '-24 hours')
                        )",
                        [politician_id, &committee.committee_id],
                        |row| row.get(0),
                    )?;

                if completed_recently {
                    eprintln!(
                        "    Skipping {} (completed within 24 hours)",
                        committee.name
                    );
                    continue;
                }
            }

            committee_tasks.push((
                politician_id.clone(),
                committee.committee_id.clone(),
                committee.name.clone(),
                cursor,
            ));
        }
    }

    if committee_tasks.is_empty() {
        eprintln!("All committees are up to date");
        return Ok(());
    }

    // Step 4: Concurrent committee fetch pipeline
    const CONCURRENCY: usize = 3;
    const CIRCUIT_BREAKER_THRESHOLD: usize = 5;

    let pb = ProgressBar::new(committee_tasks.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap(),
    );
    pb.set_message("syncing donations...");

    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let (tx, mut rx) = mpsc::channel::<DonationMessage>(CONCURRENCY * 2);
    let mut join_set = JoinSet::new();

    // Spawn tasks for each committee
    for (politician_id, committee_id, _committee_name, cursor) in committee_tasks {
        let sem = Arc::clone(&semaphore);
        let sender = tx.clone();
        let client_clone = Arc::clone(&client);
        let cycle = args.cycle;
        let per_page = args.batch_size;

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");

            // Jittered delay for rate limiting
            let delay_ms = rand::thread_rng().gen_range(200..500);
            sleep(Duration::from_millis(delay_ms)).await;

            // Keyset pagination loop
            let mut current_cursor = cursor;

            loop {
                // Build query with cursor
                let mut query = ScheduleAQuery::default()
                    .with_committee_id(&committee_id)
                    .with_per_page(per_page);

                if let Some(c) = cycle {
                    query = query.with_cycle(c);
                }

                if let Some((last_idx, ref last_date)) = current_cursor {
                    query = query
                        .with_last_index(last_idx)
                        .with_last_contribution_receipt_date(last_date);
                }

                // Fetch page from API
                match client_clone.get_schedule_a(&query).await {
                    Ok(response) => {
                        if response.results.is_empty() {
                            // No more results, mark completed
                            let _ = sender.send(DonationMessage::Completed {
                                politician_id: politician_id.clone(),
                                committee_id: committee_id.clone(),
                            }).await;
                            break;
                        }

                        // Extract pagination cursor
                        if let Some(ref indexes) = response.pagination.last_indexes {
                            let last_index = indexes.last_index;
                            let last_date = indexes.last_contribution_receipt_date.clone();

                            // Send page to receiver
                            let _ = sender.send(DonationMessage::Page {
                                politician_id: politician_id.clone(),
                                committee_id: committee_id.clone(),
                                contributions: response.results,
                                cycle,
                                last_index,
                                last_date: last_date.clone(),
                            }).await;

                            // Update cursor for next iteration
                            current_cursor = Some((last_index, last_date));
                        } else {
                            // No last_indexes means this is the final page
                            let _ = sender.send(DonationMessage::Page {
                                politician_id: politician_id.clone(),
                                committee_id: committee_id.clone(),
                                contributions: response.results,
                                cycle,
                                last_index: 0,
                                last_date: String::new(),
                            }).await;

                            let _ = sender.send(DonationMessage::Completed {
                                politician_id: politician_id.clone(),
                                committee_id: committee_id.clone(),
                            }).await;
                            break;
                        }
                    }
                    Err(e) => {
                        // Send error to receiver
                        let _ = sender.send(DonationMessage::Error {
                            committee_id: committee_id.clone(),
                            error: e,
                        }).await;
                        break;
                    }
                }

                // Rate limiting between pages within same committee
                let delay_ms = rand::thread_rng().gen_range(200..500);
                sleep(Duration::from_millis(delay_ms)).await;
            }
        });
    }
    drop(tx);

    // Step 5: Receiver loop (single-threaded DB writes)
    // Open a separate DB handle for the receiver to avoid mutex contention
    let receiver_db = Db::open(&args.db)?;
    receiver_db.init()?;

    let mut total_synced = 0usize;
    let mut committees_processed = 0usize;
    let mut breaker = CircuitBreaker::new(CIRCUIT_BREAKER_THRESHOLD);

    while let Some(message) = rx.recv().await {
        match message {
            DonationMessage::Page {
                politician_id,
                committee_id,
                contributions,
                cycle,
                last_index,
                last_date,
            } => {
                // Save donations with cursor atomically
                let count = receiver_db.save_sync_cursor_with_donations(
                    &politician_id,
                    &committee_id,
                    &contributions,
                    cycle,
                    last_index,
                    &last_date,
                )?;

                total_synced += count;
                pb.set_message(format!(
                    "{} donations synced ({:.1}s)",
                    total_synced,
                    start_time.elapsed().as_secs_f64()
                ));
                breaker.record_success();
            }
            DonationMessage::Completed {
                politician_id,
                committee_id,
            } => {
                // Mark sync as completed
                receiver_db.mark_sync_completed(&politician_id, &committee_id)?;
                committees_processed += 1;
                pb.inc(1);
                breaker.record_success();
            }
            DonationMessage::Error { committee_id, error } => {
                match error {
                    OpenFecError::InvalidApiKey => {
                        pb.println(
                            "Fatal: Invalid OpenFEC API key. Please check your OPENFEC_API_KEY environment variable."
                        );
                        join_set.abort_all();
                        bail!("Invalid API key");
                    }
                    OpenFecError::RateLimited => {
                        pb.println(format!("  Warning: Rate limited on {}", committee_id));
                        breaker.record_failure();

                        if breaker.is_tripped() {
                            pb.println(format!(
                                "Circuit breaker tripped after {} consecutive 429 errors, halting sync",
                                CIRCUIT_BREAKER_THRESHOLD
                            ));
                            join_set.abort_all();
                            break;
                        }
                    }
                    _ => {
                        pb.println(format!(
                            "  Warning: Error fetching donations for {}: {}",
                            committee_id, error
                        ));
                        breaker.record_failure();
                    }
                }
            }
        }
    }

    pb.finish_with_message(format!(
        "Sync complete: {} donations synced",
        total_synced
    ));

    // Step 6: Summary
    let elapsed = start_time.elapsed();
    eprintln!();
    eprintln!(
        "Donation sync complete: {} donations synced across {} committees",
        total_synced, committees_processed
    );
    eprintln!(
        "  Elapsed time: {:.1}s",
        elapsed.as_secs_f64()
    );

    if breaker.is_tripped() {
        bail!("Sync halted due to rate limiting");
    }

    Ok(())
}
