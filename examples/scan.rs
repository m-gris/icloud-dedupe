//! Quick scan example - run with: cargo run --example scan <path>
//!
//! Shows progress bar during scanning and human-readable output.

use std::env;
use std::path::PathBuf;

use humansize::{format_size, BINARY};
use indicatif::{ProgressBar, ProgressStyle};

use icloud_dedupe::scanner::{find_candidates, verify_candidate};
use icloud_dedupe::types::{ScanConfig, ScanReport, VerificationResult};

fn main() {
    let args: Vec<String> = env::args().collect();

    let path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        env::current_dir().expect("Failed to get current directory")
    };

    println!("Scanning: {}", path.display());
    println!();

    let config = ScanConfig {
        roots: vec![path],
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

    // Phase 2: Verification (progress bar - we know total now)
    let progress = ProgressBar::new(candidates.len() as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    progress.set_message("Verifying...");

    let mut report = ScanReport::default();

    for candidate in &candidates {
        match verify_candidate(candidate) {
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
            Err(_) => {
                // Skip files we can't read
            }
        }
        progress.inc(1);
    }

    progress.finish_with_message("Done!");
    println!();

    // Output results
    if !report.confirmed_duplicates.is_empty() {
        println!("=== Confirmed Duplicates ===");
        for group in &report.confirmed_duplicates {
            println!("Original: {}", group.original.display());
            for dup in &group.duplicates {
                println!("  └─ {}", dup.display());
            }
        }
        println!();
    }

    if !report.orphaned_conflicts.is_empty() {
        println!("=== Orphaned Conflicts (no original found) ===");
        for path in &report.orphaned_conflicts {
            println!("  {}", path.display());
        }
        println!();
    }

    if !report.content_diverged.is_empty() {
        println!("=== Content Diverged (different content) ===");
        for (conflict, original) in &report.content_diverged {
            println!("  {} ≠ {}", conflict.display(), original.display());
        }
        println!();
    }

    // Summary with human-readable sizes
    let total_duplicates: usize = report
        .confirmed_duplicates
        .iter()
        .map(|g| g.duplicates.len())
        .sum();

    println!("=== Summary ===");
    println!("Duplicate groups:   {}", report.confirmed_duplicates.len());
    println!("Total duplicates:   {}", total_duplicates);
    println!("Orphaned conflicts: {}", report.orphaned_conflicts.len());
    println!("Diverged files:     {}", report.content_diverged.len());
    println!(
        "Space recoverable:  {}",
        format_size(report.bytes_recoverable, BINARY)
    );
}
