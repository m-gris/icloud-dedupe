//! Quick scan example - run with: cargo run --example scan <path>

use std::env;
use std::path::PathBuf;

use icloud_dedupe::scanner::scan;
use icloud_dedupe::types::ScanConfig;

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

    match scan(&config) {
        Ok(report) => {
            // Confirmed duplicates
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

            // Orphaned conflicts
            if !report.orphaned_conflicts.is_empty() {
                println!("=== Orphaned Conflicts (no original found) ===");
                for path in &report.orphaned_conflicts {
                    println!("  {}", path.display());
                }
                println!();
            }

            // Content diverged
            if !report.content_diverged.is_empty() {
                println!("=== Content Diverged (different content) ===");
                for (conflict, original) in &report.content_diverged {
                    println!("  {} ≠ {}", conflict.display(), original.display());
                }
                println!();
            }

            // Summary
            println!("=== Summary ===");
            println!("Duplicate groups: {}", report.confirmed_duplicates.len());
            println!(
                "Total duplicates: {}",
                report
                    .confirmed_duplicates
                    .iter()
                    .map(|g| g.duplicates.len())
                    .sum::<usize>()
            );
            println!("Orphaned conflicts: {}", report.orphaned_conflicts.len());
            println!("Diverged files: {}", report.content_diverged.len());
            println!("Bytes recoverable: {}", report.bytes_recoverable);
        }
        Err(e) => {
            eprintln!("Error scanning: {}", e);
            std::process::exit(1);
        }
    }
}
