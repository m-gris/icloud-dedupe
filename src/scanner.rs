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
use crate::types::{DuplicateGroup, ScanConfig, ScanReport};

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
        let mut walker = WalkDir::new(root);

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
    // Check if original exists
    if !original_path.exists() {
        // Orphaned conflicts
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
            Err(_) => {
                // Could not read conflict file, skip
                continue;
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
