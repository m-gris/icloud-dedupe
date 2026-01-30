//! Pattern-only candidate discovery (no hashing).
//!
//! Run with: cargo run --example candidates <path>

use std::env;
use std::path::PathBuf;

use icloud_dedupe::scanner::find_candidates;
use icloud_dedupe::types::ScanConfig;

fn main() {
    let args: Vec<String> = env::args().collect();

    let path = if args.len() > 1 {
        PathBuf::from(&args[1])
    } else {
        env::current_dir().expect("Failed to get current directory")
    };

    println!("Finding conflict candidates in: {}", path.display());
    println!("(Pattern-based only — no hash verification)\n");

    let config = ScanConfig {
        roots: vec![path],
        ..Default::default()
    };

    match find_candidates(&config) {
        Ok(candidates) => {
            if candidates.is_empty() {
                println!("No conflict patterns found.");
                return;
            }

            println!("Found {} candidates:\n", candidates.len());

            for candidate in &candidates {
                println!("  Conflict: {}", candidate.path.display());
                println!("  Pattern:  {:?}", candidate.pattern);
                println!("  Original: {}", candidate.presumed_original.display());
                println!();
            }

            println!("---");
            println!("To verify these with hash comparison, use `cargo run --example scan`");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
