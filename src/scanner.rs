//! Directory scanning for iCloud conflict duplicates.
//!
//! Orchestrates pattern detection and hash verification.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::hash::{files_match, hash_file};
use crate::pattern::{derive_original, detect_pattern};
use crate::types::{
    ConflictCandidate, DuplicateGroup, FileKind, ScanConfig, ScanReport, VerificationResult,
};
#[cfg(test)]
use crate::types::ConflictPattern;

// ============================================================================
// PATH UTILITIES
// ============================================================================

/// Result of normalizing a user-provided path.
#[derive(Debug)]
pub struct NormalizedPath {
    /// The cleaned path ready for filesystem use.
    pub path: PathBuf,
    /// Warnings about path issues (caller decides whether to print).
    pub warnings: Vec<String>,
}

/// Normalize and expand a user-provided path (pure function).
///
/// Handles common shell quoting mistakes:
/// - Expands `~` to home directory
/// - Normalizes `\ ` to ` ` (redundant escaping)
///
/// Returns the cleaned path AND any warnings as data.
/// Caller decides whether/how to display warnings.
pub fn normalize_path(path: &Path) -> NormalizedPath {
    let path_str = path.to_string_lossy();
    let mut normalized = path_str.to_string();
    let mut warnings = Vec::new();

    // Check for redundant backslash escapes (e.g., "Mobile\ Documents" in quotes)
    if normalized.contains("\\ ") {
        warnings.push(
            "Found '\\ ' in path - removing redundant escapes. \
             (Tip: use quotes OR backslashes, not both)"
                .to_string(),
        );
        normalized = normalized.replace("\\ ", " ");
    }

    // Check for unexpanded tilde
    let needs_tilde_expansion = normalized.starts_with("~/") || normalized == "~";

    if needs_tilde_expansion {
        if let Some(home) = dirs::home_dir() {
            warnings.push(format!(
                "Expanded '~' to '{}' (shell didn't expand it due to quoting)",
                home.display()
            ));
            let path = if normalized == "~" {
                home
            } else {
                home.join(&normalized[2..])
            };
            return NormalizedPath { path, warnings };
        }
    }

    NormalizedPath {
        path: PathBuf::from(normalized),
        warnings,
    }
}

// ============================================================================
// PUBLIC API
// ============================================================================

/// Scan directories for iCloud conflict duplicates.
///
/// Walks the directory tree, identifies conflict-patterned files,
/// verifies originals exist, and validates content via hashing.
///
/// # Errors
/// Returns an error if a root directory cannot be read.
pub fn scan(config: &ScanConfig) -> io::Result<ScanReport> {
    let mut report = ScanReport::default();

    for root in &config.roots {
        let normalized = normalize_path(root);
        let mut walker = WalkDir::new(&normalized.path);

        if let Some(max_depth) = config.max_depth {
            walker = walker.max_depth(max_depth);
        }

        if !config.follow_symlinks {
            walker = walker.follow_links(false);
        }

        scan_walker(walker, &mut report, config)?;
    }

    Ok(report)
}

/// Scan a single directory (non-recursive convenience function).
///
/// # Errors
/// Returns an error if the directory cannot be read.
pub fn scan_dir(path: &Path) -> io::Result<ScanReport> {
    let config = ScanConfig {
        roots: vec![path.to_path_buf()],
        ..Default::default()
    };
    scan(&config)
}

/// Find conflict candidates by pattern (no hash verification).
///
/// This is the fast, pattern-only discovery phase. Returns all files
/// matching iCloud conflict patterns without checking if originals exist
/// or verifying content.
///
/// # Errors
/// Returns an error if a root directory cannot be read.
pub fn find_candidates(config: &ScanConfig) -> io::Result<Vec<ConflictCandidate>> {
    let mut candidates = Vec::new();

    for root in &config.roots {
        let normalized = normalize_path(root);
        let mut walker = WalkDir::new(&normalized.path);

        if let Some(max_depth) = config.max_depth {
            walker = walker.max_depth(max_depth);
        }

        if !config.follow_symlinks {
            walker = walker.follow_links(false);
        }

        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();

            // Skip directories
            if !path.is_file() {
                continue;
            }

            // Skip hidden files if configured
            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => continue,
            };

            if !config.include_hidden && filename.starts_with('.') {
                continue;
            }

            // Check for conflict pattern
            if let Some(pattern) = detect_pattern(filename) {
                let presumed_original = derive_original(path, &pattern);
                let kind = if path.is_dir() {
                    FileKind::Bundle
                } else {
                    FileKind::Regular
                };

                candidates.push(ConflictCandidate {
                    path: path.to_path_buf(),
                    pattern,
                    presumed_original,
                    kind,
                });
            }
        }
    }

    Ok(candidates)
}

/// Verify a single conflict candidate against its presumed original.
///
/// Checks:
/// 1. Does the original exist?
/// 2. Are both regular files (not bundles)?
/// 3. Do contents match (via hash)?
///
/// # Errors
/// Returns an error if files cannot be read.
pub fn verify_candidate(candidate: &ConflictCandidate) -> io::Result<VerificationResult> {
    let original = &candidate.presumed_original;
    let conflict = &candidate.path;

    // Check if original exists and is a regular file
    if !original.exists() || !original.is_file() {
        return Ok(VerificationResult::OrphanedConflict {
            path: conflict.clone(),
            presumed_original: original.clone(),
        });
    }

    // Hash both files
    let original_hash = hash_file(original)?;
    let conflict_hash = hash_file(conflict)?;

    if original_hash == conflict_hash {
        Ok(VerificationResult::ConfirmedDuplicate {
            keep: original.clone(),
            remove: conflict.clone(),
            hash: original_hash,
        })
    } else {
        Ok(VerificationResult::ContentDiverged {
            conflict_path: conflict.clone(),
            original_path: original.clone(),
            conflict_hash,
            original_hash,
        })
    }
}

// ============================================================================
// INTERNAL
// ============================================================================

fn scan_walker(
    walker: WalkDir,
    report: &mut ScanReport,
    config: &ScanConfig,
) -> io::Result<()> {
    // Collect conflict candidates grouped by their presumed original
    let mut candidates: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();

        // Skip directories
        if !path.is_file() {
            continue;
        }

        // Skip hidden files if configured
        let filename = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        if !config.include_hidden && filename.starts_with('.') {
            continue;
        }

        // Check for conflict pattern
        if let Some(pattern) = detect_pattern(filename) {
            let original_path = derive_original(path, &pattern);
            candidates
                .entry(original_path)
                .or_default()
                .push(path.to_path_buf());
        }
    }

    // Process each group of candidates
    for (original_path, conflict_paths) in candidates {
        process_candidate_group(&original_path, &conflict_paths, report)?;
    }

    Ok(())
}

fn process_candidate_group(
    original_path: &Path,
    conflict_paths: &[PathBuf],
    report: &mut ScanReport,
) -> io::Result<()> {
    // Check if original exists and is a file (not a directory/bundle)
    if !original_path.exists() || !original_path.is_file() {
        // Orphaned conflicts (or original is a bundle we can't hash)
        for path in conflict_paths {
            report.orphaned_conflicts.push(path.clone());
        }
        return Ok(());
    }

    // Hash the original
    let original_hash = hash_file(original_path)?;

    let mut confirmed_duplicates: Vec<PathBuf> = Vec::new();

    for conflict_path in conflict_paths {
        // Compare content
        match files_match(original_path, conflict_path) {
            Ok(true) => {
                // Confirmed duplicate
                let size = fs::metadata(conflict_path).map(|m| m.len()).unwrap_or(0);
                report.bytes_recoverable += size;
                confirmed_duplicates.push(conflict_path.clone());
            }
            Ok(false) => {
                // Content diverged
                report
                    .content_diverged
                    .push((conflict_path.clone(), original_path.to_path_buf()));
            }
            Err(e) => {
                // Track skipped files instead of silent continue
                report
                    .skipped
                    .push((conflict_path.clone(), e.to_string()));
            }
        }
    }

    // Add duplicate group if any confirmed
    if !confirmed_duplicates.is_empty() {
        report.confirmed_duplicates.push(DuplicateGroup {
            original: original_path.to_path_buf(),
            hash: original_hash,
            duplicates: confirmed_duplicates,
        });
    }

    Ok(())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper: create a temp directory with test files
    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Original file
        let mut original = File::create(dir.path().join("document.txt")).unwrap();
        writeln!(original, "original content").unwrap();

        // Conflict copy (same content)
        let mut copy = File::create(dir.path().join("document Copy.txt")).unwrap();
        writeln!(copy, "original content").unwrap();

        // Another conflict copy
        let mut copy2 = File::create(dir.path().join("document Copy 2.txt")).unwrap();
        writeln!(copy2, "original content").unwrap();

        dir
    }

    /// Helper: create orphaned conflict (no original)
    fn setup_orphaned_conflict() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Conflict file without original
        let mut orphan = File::create(dir.path().join("missing Copy.txt")).unwrap();
        writeln!(orphan, "orphaned content").unwrap();

        dir
    }

    /// Helper: create diverged conflict (different content)
    fn setup_diverged_conflict() -> TempDir {
        let dir = TempDir::new().unwrap();

        // Original file
        let mut original = File::create(dir.path().join("file.txt")).unwrap();
        writeln!(original, "version A").unwrap();

        // Conflict with different content
        let mut conflict = File::create(dir.path().join("file Copy.txt")).unwrap();
        writeln!(conflict, "version B - modified").unwrap();

        dir
    }

    // --- scan_dir tests ---

    #[test]
    fn test_scan_finds_confirmed_duplicates() {
        let dir = setup_test_dir();
        let report = scan_dir(dir.path()).unwrap();

        // Should find duplicates: "document Copy.txt" and "document Copy 2.txt"
        assert_eq!(report.confirmed_duplicates.len(), 1); // One group
        assert_eq!(report.confirmed_duplicates[0].duplicates.len(), 2); // Two copies
    }

    #[test]
    fn test_scan_finds_orphaned_conflicts() {
        let dir = setup_orphaned_conflict();
        let report = scan_dir(dir.path()).unwrap();

        assert_eq!(report.orphaned_conflicts.len(), 1);
        assert!(report.confirmed_duplicates.is_empty());
    }

    #[test]
    fn test_scan_finds_diverged_conflicts() {
        let dir = setup_diverged_conflict();
        let report = scan_dir(dir.path()).unwrap();

        assert_eq!(report.content_diverged.len(), 1);
        assert!(report.confirmed_duplicates.is_empty());
    }

    #[test]
    fn test_scan_empty_dir() {
        let dir = TempDir::new().unwrap();
        let report = scan_dir(dir.path()).unwrap();

        assert!(report.confirmed_duplicates.is_empty());
        assert!(report.orphaned_conflicts.is_empty());
        assert!(report.content_diverged.is_empty());
        assert_eq!(report.bytes_recoverable, 0);
    }

    #[test]
    fn test_scan_no_conflicts() {
        let dir = TempDir::new().unwrap();

        // Normal files, no conflict patterns
        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(dir.path().join("file2.txt")).unwrap();

        let report = scan_dir(dir.path()).unwrap();

        assert!(report.confirmed_duplicates.is_empty());
        assert!(report.orphaned_conflicts.is_empty());
        assert!(report.content_diverged.is_empty());
    }

    #[test]
    fn test_scan_calculates_bytes_recoverable() {
        let dir = TempDir::new().unwrap();

        // Original
        let mut original = File::create(dir.path().join("data.txt")).unwrap();
        write!(original, "12345").unwrap(); // 5 bytes

        // Copy with same content
        let mut copy = File::create(dir.path().join("data Copy.txt")).unwrap();
        write!(copy, "12345").unwrap(); // 5 bytes

        let report = scan_dir(dir.path()).unwrap();

        assert_eq!(report.bytes_recoverable, 5); // Can recover 5 bytes
    }

    // --- scan with config tests ---

    #[test]
    fn test_scan_with_config() {
        let dir = setup_test_dir();

        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };

        let report = scan(&config).unwrap();

        assert_eq!(report.confirmed_duplicates.len(), 1);
    }

    // --- find_candidates tests (pattern-only, no hashing) ---

    #[test]
    fn test_find_candidates_returns_candidates() {
        let dir = setup_test_dir();
        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };

        let candidates = find_candidates(&config).unwrap();

        // Should find 2 candidates: "document Copy.txt" and "document Copy 2.txt"
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn test_find_candidates_no_hashing_occurs() {
        // Even with diverged content, find_candidates should return them
        let dir = setup_diverged_conflict();
        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };

        let candidates = find_candidates(&config).unwrap();

        // Should find the candidate (hashing hasn't happened yet)
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].path.to_string_lossy().contains("Copy"));
    }

    #[test]
    fn test_find_candidates_includes_orphaned() {
        let dir = setup_orphaned_conflict();
        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };

        let candidates = find_candidates(&config).unwrap();

        // Should find orphaned candidate (original existence not checked yet)
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_find_candidates_empty_for_no_conflicts() {
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("normal.txt")).unwrap();

        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            ..Default::default()
        };

        let candidates = find_candidates(&config).unwrap();
        assert!(candidates.is_empty());
    }

    // --- verify_candidate tests (hash-based, singular) ---

    #[test]
    fn test_verify_candidate_confirmed_duplicate() {
        let dir = TempDir::new().unwrap();

        // Original and copy with same content
        let mut orig = File::create(dir.path().join("doc.txt")).unwrap();
        writeln!(orig, "same content").unwrap();
        let mut copy = File::create(dir.path().join("doc Copy.txt")).unwrap();
        writeln!(copy, "same content").unwrap();

        let candidate = ConflictCandidate {
            path: dir.path().join("doc Copy.txt"),
            pattern: ConflictPattern::Copy { index: None },
            presumed_original: dir.path().join("doc.txt"),
            kind: FileKind::Regular,
        };

        let result = verify_candidate(&candidate).unwrap();
        assert!(matches!(result, VerificationResult::ConfirmedDuplicate { .. }));
    }

    #[test]
    fn test_verify_candidate_orphaned() {
        let dir = TempDir::new().unwrap();

        // Only the copy exists, no original
        let mut copy = File::create(dir.path().join("missing Copy.txt")).unwrap();
        writeln!(copy, "orphaned").unwrap();

        let candidate = ConflictCandidate {
            path: dir.path().join("missing Copy.txt"),
            pattern: ConflictPattern::Copy { index: None },
            presumed_original: dir.path().join("missing.txt"), // doesn't exist
            kind: FileKind::Regular,
        };

        let result = verify_candidate(&candidate).unwrap();
        assert!(matches!(result, VerificationResult::OrphanedConflict { .. }));
    }

    #[test]
    fn test_verify_candidate_diverged() {
        let dir = TempDir::new().unwrap();

        // Original and copy with DIFFERENT content
        let mut orig = File::create(dir.path().join("file.txt")).unwrap();
        writeln!(orig, "version A").unwrap();
        let mut copy = File::create(dir.path().join("file Copy.txt")).unwrap();
        writeln!(copy, "version B").unwrap();

        let candidate = ConflictCandidate {
            path: dir.path().join("file Copy.txt"),
            pattern: ConflictPattern::Copy { index: None },
            presumed_original: dir.path().join("file.txt"),
            kind: FileKind::Regular,
        };

        let result = verify_candidate(&candidate).unwrap();
        assert!(matches!(result, VerificationResult::ContentDiverged { .. }));
    }

    // --- scan with config tests ---

    #[test]
    fn test_scan_respects_max_depth() {
        let dir = TempDir::new().unwrap();

        // Create nested structure
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        // File in root (should be found with depth 1)
        let mut root_orig = File::create(dir.path().join("root.txt")).unwrap();
        writeln!(root_orig, "root content").unwrap();
        let mut root_copy = File::create(dir.path().join("root Copy.txt")).unwrap();
        writeln!(root_copy, "root content").unwrap();

        // File in subdir (should NOT be found with depth 1)
        let mut sub_orig = File::create(subdir.join("sub.txt")).unwrap();
        writeln!(sub_orig, "sub content").unwrap();
        let mut sub_copy = File::create(subdir.join("sub Copy.txt")).unwrap();
        writeln!(sub_copy, "sub content").unwrap();

        let config = ScanConfig {
            roots: vec![dir.path().to_path_buf()],
            max_depth: Some(1), // Only root level
            ..Default::default()
        };

        let report = scan(&config).unwrap();

        // Should only find root-level duplicates
        assert_eq!(report.confirmed_duplicates.len(), 1);
    }
}
