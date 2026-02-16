//! icloud-dedupe CLI
//!
//! Detect and remove iCloud sync conflict duplicates on macOS.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use humansize::{format_size, BINARY};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use icloud_dedupe::platform::{detect_icloud, ICloudState};
use icloud_dedupe::quarantine::{
    default_quarantine_dir, init_quarantine, load_manifest, purge_quarantine,
    quarantine_duplicates, restore_file,
};
use icloud_dedupe::report::format_report;
use icloud_dedupe::scanner::{find_candidates, find_candidates_with_progress, normalize_path, verify_candidate};
use icloud_dedupe::types::{
    ConflictCandidate, DuplicateGroup, OutputFormat, QuarantineConfig, ScanConfig, ScanReport,
    VerificationResult,
};

#[derive(Parser)]
#[command(name = "icloud-dedupe")]
#[command(about = "Detect and remove iCloud sync conflict duplicates")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan for conflict files and report findings (no modifications)
    Scan {
        /// Directory to scan (default: iCloud location)
        path: Option<PathBuf>,

        /// Output format
        #[arg(long, value_enum, default_value = "human")]
        format: OutputFormatArg,

        /// Maximum directory depth
        #[arg(long)]
        max_depth: Option<usize>,
    },

    /// Move confirmed duplicates to quarantine
    Quarantine {
        /// Directory to scan (default: iCloud location)
        path: Option<PathBuf>,

        /// Preview only, don't actually move files
        #[arg(long)]
        dry_run: bool,

        /// Maximum directory depth
        #[arg(long)]
        max_depth: Option<usize>,
    },

    /// Restore files from quarantine
    Restore {
        /// Restore all quarantined files
        #[arg(long)]
        all: bool,

        /// Specific receipt ID to restore
        id: Option<String>,
    },

    /// Permanently delete all quarantined files
    Purge {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Show quarantine status and contents
    Status,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OutputFormatArg {
    Human,
    Json,
}

impl From<OutputFormatArg> for OutputFormat {
    fn from(arg: OutputFormatArg) -> Self {
        match arg {
            OutputFormatArg::Human => OutputFormat::Human,
            OutputFormatArg::Json => OutputFormat::Json,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Scan { path, format, max_depth } => cmd_scan(path, format.into(), max_depth),
        Commands::Quarantine { path, dry_run, max_depth } => cmd_quarantine(path, dry_run, max_depth),
        Commands::Restore { all, id } => cmd_restore(all, id),
        Commands::Purge { force } => cmd_purge(force),
        Commands::Status => cmd_status(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::FAILURE
        }
    }
}

// ============================================================================
// PATH RESOLUTION
// ============================================================================

/// Resolve scan path: use provided path or detect iCloud location.
fn resolve_scan_path(path: Option<PathBuf>) -> Result<PathBuf, String> {
    match path {
        Some(p) => Ok(p),
        None => {
            // Auto-detect iCloud location
            let state = detect_icloud().map_err(|e| e.to_string())?;

            match &state {
                ICloudState::DriveEnabled { container, .. } => {
                    eprintln!("Detected: {}", state);
                    Ok(container.clone())
                }
                ICloudState::DriveDisabled { container } => {
                    eprintln!("Detected: {}", state);
                    eprintln!("Note: iCloud Drive is disabled, scanning app containers only.");
                    Ok(container.clone())
                }
                ICloudState::NotConfigured { expected } => {
                    Err(format!(
                        "iCloud not configured.\n\
                         Expected directory: {}\n\
                         \n\
                         Either:\n\
                         - Sign in to iCloud in System Settings\n\
                         - Specify a path explicitly: icloud-dedupe scan <path>",
                        expected.display()
                    ))
                }
            }
        }
    }
}

// ============================================================================
// PROGRESS HELPERS
// ============================================================================

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

fn progress_bar(total: u64, msg: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );
    pb.set_message(msg.to_string());
    pb
}

// ============================================================================
// COMMAND HANDLERS
// ============================================================================

fn cmd_scan(path: Option<PathBuf>, format: OutputFormat, max_depth: Option<usize>) -> Result<(), String> {
    let resolved = resolve_scan_path(path)?;
    let normalized = normalize_path(&resolved);

    // Print warnings to stderr so they don't interfere with JSON output
    for warning in &normalized.warnings {
        eprintln!("Note: {}", warning);
    }

    let show_progress = format == OutputFormat::Human;

    if show_progress {
        eprintln!("Scanning: {}", normalized.path.display());
        eprintln!();
    }

    let config = ScanConfig {
        roots: vec![normalized.path],
        max_depth,
        ..Default::default()
    };

    // Phase 1: Discovery
    let candidates = if show_progress {
        let sp = spinner("Discovering conflict patterns...");
        let result = find_candidates_with_progress(&config, |scanned, found| {
            sp.set_message(format!(
                "Scanned {} files, found {} candidates...",
                scanned, found
            ));
        });
        match result {
            Ok(c) => {
                sp.finish_with_message(format!(
                    "Scanned files, found {} candidates",
                    c.len()
                ));
                c
            }
            Err(e) => {
                sp.finish_and_clear();
                return Err(e.to_string());
            }
        }
    } else {
        find_candidates(&config).map_err(|e| e.to_string())?
    };

    if candidates.is_empty() {
        if show_progress {
            println!("No conflict patterns found.");
        } else {
            println!("{}", format_report(&ScanReport::default(), format));
        }
        return Ok(());
    }

    // Phase 2: Verification (parallel)
    let report = if show_progress {
        build_report_with_progress(&candidates)
    } else {
        build_report(&candidates)
    };

    print!("{}", format_report(&report, format));

    Ok(())
}

fn cmd_quarantine(path: Option<PathBuf>, dry_run: bool, max_depth: Option<usize>) -> Result<(), String> {
    let resolved = resolve_scan_path(path)?;
    let normalized = normalize_path(&resolved);

    for warning in &normalized.warnings {
        eprintln!("Note: {}", warning);
    }

    eprintln!("Scanning: {}", normalized.path.display());
    eprintln!();

    let config = ScanConfig {
        roots: vec![normalized.path],
        max_depth,
        ..Default::default()
    };

    // Phase 1: Discovery
    let sp = spinner("Discovering conflict patterns...");

    let candidates = match find_candidates_with_progress(&config, |scanned, found| {
        sp.set_message(format!(
            "Scanned {} files, found {} candidates...",
            scanned, found
        ));
    }) {
        Ok(c) => {
            sp.finish_with_message(format!("Scanned files, found {} candidates", c.len()));
            c
        }
        Err(e) => {
            sp.finish_and_clear();
            return Err(e.to_string());
        }
    };

    sp.finish_with_message(format!("Found {} candidates", candidates.len()));

    if candidates.is_empty() {
        println!("No conflict patterns found.");
        return Ok(());
    }

    // Phase 2: Verification (parallel)
    let report = build_report_with_progress(&candidates);

    if report.confirmed_duplicates.is_empty() {
        println!("No confirmed duplicates found.");
        return Ok(());
    }

    let total_files: usize = report.confirmed_duplicates.iter().map(|g| g.duplicates.len()).sum();

    if dry_run {
        println!();
        println!("DRY RUN - would quarantine {} files ({}):",
            total_files,
            format_size(report.bytes_recoverable, BINARY)
        );
        for group in &report.confirmed_duplicates {
            for dup in &group.duplicates {
                println!("  {}", dup.display());
            }
        }
        return Ok(());
    }

    println!();
    println!("Quarantining {} files ({})...",
        total_files,
        format_size(report.bytes_recoverable, BINARY)
    );

    let quarantine_config = QuarantineConfig {
        quarantine_dir: default_quarantine_dir(),
        dry_run: false,
        preserve_structure: true,
    };

    let manifest = quarantine_duplicates(&report.confirmed_duplicates, &quarantine_config)
        .map_err(|e| e.to_string())?;

    println!(
        "Done. {} files moved to quarantine.",
        manifest.quarantined.len()
    );
    println!("Quarantine location: {}", quarantine_config.quarantine_dir.display());
    println!();
    println!("To restore: icloud-dedupe restore --all");
    println!("To purge:   icloud-dedupe purge");

    Ok(())
}

fn cmd_restore(all: bool, id: Option<String>) -> Result<(), String> {
    let config = QuarantineConfig {
        quarantine_dir: default_quarantine_dir(),
        ..Default::default()
    };

    let config = init_quarantine(&config).map_err(|e| e.to_string())?;
    let manifest = load_manifest(&config).map_err(|e| format!("No quarantine found: {}", e))?;

    if manifest.quarantined.is_empty() {
        println!("Quarantine is empty.");
        return Ok(());
    }

    if all {
        let pb = progress_bar(manifest.quarantined.len() as u64, "Restoring...");

        let mut restored = 0;
        let mut failed = 0;

        for receipt in &manifest.quarantined {
            match restore_file(receipt) {
                Ok(()) => {
                    restored += 1;
                }
                Err(e) => {
                    pb.println(format!("  Failed: {} - {}", receipt.original_path.display(), e));
                    failed += 1;
                }
            }
            pb.inc(1);
        }

        pb.finish_with_message("Done");
        println!();
        println!("Restored: {}, Failed: {}", restored, failed);
    } else if let Some(id) = id {
        let receipt = manifest
            .quarantined
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("Receipt not found: {}", id))?;

        restore_file(receipt).map_err(|e| e.to_string())?;
        println!("Restored: {}", receipt.original_path.display());
    } else {
        return Err("Specify --all or a receipt ID".to_string());
    }

    Ok(())
}

fn cmd_purge(force: bool) -> Result<(), String> {
    let config = QuarantineConfig {
        quarantine_dir: default_quarantine_dir(),
        ..Default::default()
    };

    let config = init_quarantine(&config).map_err(|e| e.to_string())?;
    let manifest = load_manifest(&config).map_err(|e| format!("No quarantine found: {}", e))?;

    if manifest.quarantined.is_empty() {
        println!("Quarantine is empty.");
        return Ok(());
    }

    let total_bytes: u64 = manifest.quarantined.iter().map(|r| r.size_bytes).sum();

    println!(
        "About to permanently delete {} files ({})",
        manifest.quarantined.len(),
        format_size(total_bytes, BINARY)
    );

    if !force {
        eprint!("Continue? [y/N] ");

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    purge_quarantine(&manifest, &config).map_err(|e| e.to_string())?;
    println!("Purged {} files.", manifest.quarantined.len());

    Ok(())
}

fn cmd_status() -> Result<(), String> {
    let config = QuarantineConfig {
        quarantine_dir: default_quarantine_dir(),
        ..Default::default()
    };

    println!("Quarantine location: {}", config.quarantine_dir.display());
    println!();

    let manifest = match load_manifest(&config) {
        Ok(m) => m,
        Err(_) => {
            println!("Quarantine is empty (no manifest found).");
            return Ok(());
        }
    };

    if manifest.quarantined.is_empty() {
        println!("Quarantine is empty.");
        return Ok(());
    }

    let total_bytes: u64 = manifest.quarantined.iter().map(|r| r.size_bytes).sum();

    println!("Files: {}", manifest.quarantined.len());
    println!("Total size: {}", format_size(total_bytes, BINARY));
    println!();
    println!("Contents:");

    for receipt in &manifest.quarantined {
        println!(
            "  [{}] {} ({})",
            receipt.id,
            receipt.original_path.display(),
            format_size(receipt.size_bytes, BINARY)
        );
    }

    Ok(())
}

// ============================================================================
// REPORT BUILDING
// ============================================================================

/// Build report with progress bar (parallel verification).
fn build_report_with_progress(candidates: &[ConflictCandidate]) -> ScanReport {
    let pb = progress_bar(candidates.len() as u64, "Verifying...");

    let results: Vec<_> = candidates
        .par_iter()
        .progress_with(pb.clone())
        .map(|candidate| (candidate.path.clone(), verify_candidate(candidate)))
        .collect();

    pb.finish_with_message("Done");

    assemble_report(results)
}

/// Build report without progress (for JSON output).
fn build_report(candidates: &[ConflictCandidate]) -> ScanReport {
    let results: Vec<_> = candidates
        .par_iter()
        .map(|candidate| (candidate.path.clone(), verify_candidate(candidate)))
        .collect();

    assemble_report(results)
}

/// Assemble report from verification results.
fn assemble_report(results: Vec<(PathBuf, std::io::Result<VerificationResult>)>) -> ScanReport {
    let mut report = ScanReport::default();

    for (path, result) in results {
        match result {
            Ok(VerificationResult::ConfirmedDuplicate { keep, remove, hash }) => {
                let size = std::fs::metadata(&remove).map(|m| m.len()).unwrap_or(0);
                report.bytes_recoverable += size;

                if let Some(group) = report
                    .confirmed_duplicates
                    .iter_mut()
                    .find(|g| g.original == keep)
                {
                    group.duplicates.push(remove);
                } else {
                    report.confirmed_duplicates.push(DuplicateGroup {
                        original: keep,
                        hash,
                        duplicates: vec![remove],
                    });
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
                report.skipped.push((path, e.to_string()));
            }
        }
    }

    report
}
