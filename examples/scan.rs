//! Quick scan example - run with: cargo run --example scan <path>
//!
//! Shows progress bar during scanning and human-readable output.
//! Use --json flag for JSON output: cargo run --example scan -- --json <path>

use std::env;
use std::path::PathBuf;

use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use icloud_dedupe::report::format_report;
use icloud_dedupe::scanner::{find_candidates, normalize_path, verify_candidate};
use icloud_dedupe::types::{OutputFormat, ScanConfig, ScanReport, VerificationResult};

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse --json flag
    let json_mode = args.iter().any(|a| a == "--json");
    let output_format = if json_mode {
        OutputFormat::Json
    } else {
        OutputFormat::Human
    };

    // Find path argument (skip --json if present)
    let path = args
        .iter()
        .skip(1)
        .find(|a| *a != "--json")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().expect("Failed to get current directory"));

    // Normalize path early so warnings print before progress bar
    let normalized = normalize_path(&path);

    // Print any path normalization warnings
    for warning in &normalized.warnings {
        eprintln!("Note: {}", warning);
    }

    println!("Scanning: {}", normalized.path.display());
    println!();

    let config = ScanConfig {
        roots: vec![normalized.path],
        ..Default::default()
    };

    // Phase 1: Discovery (spinner - we don't know total yet)
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Discovering conflict patterns...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    let candidates = match find_candidates(&config) {
        Ok(c) => c,
        Err(e) => {
            spinner.finish_and_clear();
            eprintln!("Error during discovery: {}", e);
            std::process::exit(1);
        }
    };

    spinner.finish_with_message(format!("Found {} candidates", candidates.len()));

    if candidates.is_empty() {
        println!("\nNo conflict patterns found.");
        return;
    }

    // Phase 2: Verification (parallel with progress bar)
    let progress = ProgressBar::new(candidates.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    progress.set_message("Verifying (parallel)...");

    // Parallel verification - each candidate hashes independent files
    // Return path with result so we can track errors
    let results: Vec<_> = candidates
        .par_iter()
        .progress_with(progress.clone())
        .map(|candidate| (candidate.path.clone(), verify_candidate(candidate)))
        .collect();

    progress.finish_with_message("Done!");

    // Build report from results (sequential - mutating shared struct)
    let mut report = ScanReport::default();

    for (path, result) in results {
        match result {
            Ok(VerificationResult::ConfirmedDuplicate { keep, remove, hash }) => {
                let size = std::fs::metadata(&remove).map(|m| m.len()).unwrap_or(0);
                report.bytes_recoverable += size;

                // Find or create group for this original
                if let Some(group) = report
                    .confirmed_duplicates
                    .iter_mut()
                    .find(|g| g.original == keep)
                {
                    group.duplicates.push(remove);
                } else {
                    report.confirmed_duplicates.push(
                        icloud_dedupe::types::DuplicateGroup {
                            original: keep,
                            hash,
                            duplicates: vec![remove],
                        },
                    );
                }
            }
            Ok(VerificationResult::OrphanedConflict { path, .. }) => {
                report.orphaned_conflicts.push(path);
            }
            Ok(VerificationResult::ContentDiverged {
                conflict_path,
                original_path,
                ..
            }) => {
                report.content_diverged.push((conflict_path, original_path));
            }
            Err(e) => {
                // Track files we couldn't read
                report.skipped.push((path, e.to_string()));
            }
        }
    }
    println!();

    // Output formatted report
    print!("{}", format_report(&report, output_format));
}
