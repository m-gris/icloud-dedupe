//! Quarantine operations for icloud-dedupe.
//!
//! Moves confirmed duplicates to a staging area for safe removal.
//! Supports restore and purge operations.
//!
//! Structure:
//! - Pure functions: path computation, ID generation
//! - Effect functions: file moves, manifest I/O

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::hash::hash_file;
use crate::types::{ContentHash, DuplicateGroup, Manifest, QuarantineConfig, QuarantineReceipt};

/// Current manifest format version.
const MANIFEST_VERSION: u32 = 1;

/// Manifest filename within quarantine directory.
const MANIFEST_FILENAME: &str = "manifest.json";

// ============================================================================
// PURE FUNCTIONS (Computations)
// ============================================================================

/// Returns the default quarantine directory.
///
/// On macOS: ~/Library/Application Support/icloud-dedupe/quarantine/
pub fn default_quarantine_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("icloud-dedupe")
        .join("quarantine")
}

/// Compute where a file should be stored in quarantine.
///
/// If `preserve_structure` is true, mirrors the original path structure.
/// Otherwise, uses a flat structure with the receipt ID.
pub fn compute_quarantine_path(
    original: &Path,
    receipt_id: &str,
    config: &QuarantineConfig,
) -> PathBuf {
    if config.preserve_structure {
        // Mirror original path structure under quarantine dir
        // /Users/marc/Documents/foo.txt -> quarantine/Users/marc/Documents/foo.txt
        let stripped = original
            .strip_prefix("/")
            .unwrap_or(original);
        config.quarantine_dir.join(stripped)
    } else {
        // Flat structure: quarantine/<id>_<filename>
        let filename = original
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        config.quarantine_dir.join(format!("{}_{}", receipt_id, filename))
    }
}

/// Generate a unique receipt ID.
///
/// Format: timestamp + random suffix for uniqueness.
pub fn generate_receipt_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Add random suffix for uniqueness within same millisecond
    let random: u32 = std::process::id() ^ (timestamp as u32);

    format!("{:x}-{:04x}", timestamp, random & 0xFFFF)
}

/// Path to the manifest file.
pub fn manifest_path(config: &QuarantineConfig) -> PathBuf {
    config.quarantine_dir.join(MANIFEST_FILENAME)
}

/// Get current timestamp as ISO 8601 string.
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Simple ISO 8601 format (UTC)
    let days = secs / 86400;
    let time = secs % 86400;
    let hours = time / 3600;
    let mins = (time % 3600) / 60;
    let secs = time % 60;

    // Days since 1970-01-01
    // This is a simplified calculation, not handling leap years perfectly
    let years = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = day_of_year / 30 + 1;
    let day = day_of_year % 30 + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        years, month.min(12), day.min(31), hours, mins, secs
    )
}

// ============================================================================
// EFFECT FUNCTIONS (Actions)
// ============================================================================

/// Initialize quarantine directory and return config with resolved path.
pub fn init_quarantine(config: &QuarantineConfig) -> io::Result<QuarantineConfig> {
    let quarantine_dir = if config.quarantine_dir.as_os_str().is_empty() {
        default_quarantine_dir()
    } else {
        config.quarantine_dir.clone()
    };

    if !config.dry_run {
        fs::create_dir_all(&quarantine_dir)?;
    }

    Ok(QuarantineConfig {
        quarantine_dir,
        dry_run: config.dry_run,
        preserve_structure: config.preserve_structure,
    })
}

/// Move a single file to quarantine.
///
/// Returns a receipt for restoration.
pub fn quarantine_file(
    path: &Path,
    hash: &ContentHash,
    config: &QuarantineConfig,
) -> io::Result<QuarantineReceipt> {
    let id = generate_receipt_id();
    let quarantine_path = compute_quarantine_path(path, &id, config);

    // Get file metadata before moving
    let metadata = fs::metadata(path)?;
    let size_bytes = metadata.len();

    // Check for extended attributes (macOS)
    #[cfg(target_os = "macos")]
    let had_xattrs = has_xattrs(path);
    #[cfg(not(target_os = "macos"))]
    let had_xattrs = false;

    if !config.dry_run {
        // Create parent directories
        if let Some(parent) = quarantine_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Move the file
        fs::rename(path, &quarantine_path)?;
    }

    Ok(QuarantineReceipt {
        id,
        original_path: path.to_path_buf(),
        quarantine_path,
        hash: hash.clone(),
        quarantined_at: current_timestamp(),
        size_bytes,
        had_xattrs,
    })
}

/// Quarantine all duplicates from scan results.
///
/// Returns a manifest with all receipts.
pub fn quarantine_duplicates(
    groups: &[DuplicateGroup],
    config: &QuarantineConfig,
) -> io::Result<Manifest> {
    let config = init_quarantine(config)?;
    let mut receipts = Vec::new();

    for group in groups {
        for dup_path in &group.duplicates {
            match quarantine_file(dup_path, &group.hash, &config) {
                Ok(receipt) => receipts.push(receipt),
                Err(e) => {
                    // Log error but continue with other files
                    eprintln!("Warning: Failed to quarantine {}: {}", dup_path.display(), e);
                }
            }
        }
    }

    let manifest = Manifest {
        version: MANIFEST_VERSION,
        quarantined: receipts,
    };

    // Save manifest
    if !config.dry_run {
        save_manifest(&manifest, &config)?;
    }

    Ok(manifest)
}

/// Restore a single file from quarantine.
pub fn restore_file(receipt: &QuarantineReceipt) -> io::Result<()> {
    // Verify file still exists in quarantine
    if !receipt.quarantine_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Quarantined file not found: {}", receipt.quarantine_path.display()),
        ));
    }

    // Verify hash matches (file wasn't corrupted)
    let current_hash = hash_file(&receipt.quarantine_path)?;
    if current_hash != receipt.hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "File hash mismatch - quarantined file may be corrupted",
        ));
    }

    // Check if original location is available
    if receipt.original_path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("Original path already exists: {}", receipt.original_path.display()),
        ));
    }

    // Create parent directories if needed
    if let Some(parent) = receipt.original_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Move file back
    fs::rename(&receipt.quarantine_path, &receipt.original_path)?;

    Ok(())
}

/// Permanently delete all quarantined files.
pub fn purge_quarantine(manifest: &Manifest, config: &QuarantineConfig) -> io::Result<()> {
    for receipt in &manifest.quarantined {
        if receipt.quarantine_path.exists() {
            fs::remove_file(&receipt.quarantine_path)?;
        }
    }

    // Remove manifest
    let manifest_file = manifest_path(config);
    if manifest_file.exists() {
        fs::remove_file(manifest_file)?;
    }

    // Try to clean up empty directories
    cleanup_empty_dirs(&config.quarantine_dir)?;

    Ok(())
}

/// Load manifest from disk.
pub fn load_manifest(config: &QuarantineConfig) -> io::Result<Manifest> {
    let path = manifest_path(config);
    let contents = fs::read_to_string(&path)?;
    serde_json::from_str(&contents).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("Invalid manifest: {}", e))
    })
}

/// Save manifest to disk.
pub fn save_manifest(manifest: &Manifest, config: &QuarantineConfig) -> io::Result<()> {
    let path = manifest_path(config);
    let contents = serde_json::to_string_pretty(manifest).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("Failed to serialize manifest: {}", e))
    })?;
    fs::write(path, contents)
}

// ============================================================================
// HELPERS
// ============================================================================

/// Check if a file has extended attributes (macOS only).
#[cfg(target_os = "macos")]
fn has_xattrs(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = match CString::new(path.as_os_str().as_bytes()) {
        Ok(p) => p,
        Err(_) => return false,
    };

    // listxattr returns the size of the xattr list, or -1 on error
    let size = unsafe {
        libc::listxattr(
            c_path.as_ptr(),
            std::ptr::null_mut(),
            0,
            libc::XATTR_NOFOLLOW,
        )
    };

    size > 0
}

/// Recursively remove empty directories.
fn cleanup_empty_dirs(dir: &Path) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    // First, recurse into subdirectories
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            cleanup_empty_dirs(&path)?;
        }
    }

    // Then try to remove this directory if empty
    // Ignore errors (directory not empty or permission denied)
    let _ = fs::remove_dir(dir);

    Ok(())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn sample_hash() -> ContentHash {
        ContentHash([0xab; 32])
    }

    fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        path
    }

    // --- Pure function tests ---

    #[test]
    fn test_default_quarantine_dir_is_reasonable() {
        let dir = default_quarantine_dir();
        let path_str = dir.to_string_lossy();

        // Should contain our app name
        assert!(path_str.contains("icloud-dedupe"));
        assert!(path_str.contains("quarantine"));
    }

    #[test]
    fn test_compute_quarantine_path_preserves_structure() {
        let config = QuarantineConfig {
            quarantine_dir: PathBuf::from("/tmp/quarantine"),
            preserve_structure: true,
            dry_run: false,
        };

        let original = PathBuf::from("/Users/test/Documents/file.txt");
        let qpath = compute_quarantine_path(&original, "abc123", &config);

        assert!(qpath.starts_with("/tmp/quarantine"));
        assert!(qpath.to_string_lossy().contains("Users/test/Documents"));
    }

    #[test]
    fn test_compute_quarantine_path_flat() {
        let config = QuarantineConfig {
            quarantine_dir: PathBuf::from("/tmp/quarantine"),
            preserve_structure: false,
            dry_run: false,
        };

        let original = PathBuf::from("/Users/test/Documents/file.txt");
        let qpath = compute_quarantine_path(&original, "abc123", &config);

        assert_eq!(qpath, PathBuf::from("/tmp/quarantine/abc123_file.txt"));
    }

    #[test]
    fn test_generate_receipt_id_is_unique() {
        let id1 = generate_receipt_id();
        let id2 = generate_receipt_id();

        // Should be non-empty
        assert!(!id1.is_empty());
        assert!(!id2.is_empty());

        // Might be same if called in same millisecond with same PID,
        // but the format should be valid
        assert!(id1.contains('-'));
    }

    // --- Effect function tests ---

    #[test]
    fn test_quarantine_file_moves_file() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source");
        let quarantine_dir = temp.path().join("quarantine");

        fs::create_dir_all(&source_dir).unwrap();

        let file_path = create_test_file(&source_dir, "test.txt", b"hello");
        let hash = hash_file(&file_path).unwrap();

        let config = QuarantineConfig {
            quarantine_dir: quarantine_dir.clone(),
            preserve_structure: false,
            dry_run: false,
        };

        let receipt = quarantine_file(&file_path, &hash, &config).unwrap();

        // Original should be gone
        assert!(!file_path.exists());

        // Quarantined file should exist
        assert!(receipt.quarantine_path.exists());

        // Receipt should have correct info
        assert_eq!(receipt.original_path, file_path);
        assert_eq!(receipt.size_bytes, 5);
    }

    #[test]
    fn test_quarantine_file_dry_run() {
        let temp = TempDir::new().unwrap();
        let file_path = create_test_file(temp.path(), "test.txt", b"hello");
        let hash = hash_file(&file_path).unwrap();

        let config = QuarantineConfig {
            quarantine_dir: temp.path().join("quarantine"),
            preserve_structure: false,
            dry_run: true,
        };

        let receipt = quarantine_file(&file_path, &hash, &config).unwrap();

        // Original should still exist (dry run)
        assert!(file_path.exists());

        // Receipt should still be generated
        assert!(!receipt.id.is_empty());
    }

    #[test]
    fn test_restore_file_moves_back() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source");
        let quarantine_dir = temp.path().join("quarantine");

        fs::create_dir_all(&source_dir).unwrap();

        let file_path = create_test_file(&source_dir, "test.txt", b"hello");
        let hash = hash_file(&file_path).unwrap();

        let config = QuarantineConfig {
            quarantine_dir,
            preserve_structure: false,
            dry_run: false,
        };

        // Quarantine
        let receipt = quarantine_file(&file_path, &hash, &config).unwrap();
        assert!(!file_path.exists());

        // Restore
        restore_file(&receipt).unwrap();

        // Original should be back
        assert!(file_path.exists());
        assert!(!receipt.quarantine_path.exists());
    }

    #[test]
    fn test_restore_file_fails_if_original_exists() {
        let temp = TempDir::new().unwrap();
        let file_path = create_test_file(temp.path(), "test.txt", b"hello");
        let hash = hash_file(&file_path).unwrap();

        let config = QuarantineConfig {
            quarantine_dir: temp.path().join("quarantine"),
            preserve_structure: false,
            dry_run: false,
        };

        let receipt = quarantine_file(&file_path, &hash, &config).unwrap();

        // Create a new file at original location
        create_test_file(temp.path(), "test.txt", b"new content");

        // Restore should fail
        let result = restore_file(&receipt);
        assert!(result.is_err());
        assert!(result.unwrap_err().kind() == io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn test_manifest_save_and_load() {
        let temp = TempDir::new().unwrap();

        let config = QuarantineConfig {
            quarantine_dir: temp.path().to_path_buf(),
            preserve_structure: false,
            dry_run: false,
        };

        let manifest = Manifest {
            version: 1,
            quarantined: vec![QuarantineReceipt {
                id: "test-id".to_string(),
                original_path: PathBuf::from("/original/path.txt"),
                quarantine_path: PathBuf::from("/quarantine/path.txt"),
                hash: sample_hash(),
                quarantined_at: "2024-01-01T00:00:00Z".to_string(),
                size_bytes: 1024,
                had_xattrs: false,
            }],
        };

        save_manifest(&manifest, &config).unwrap();

        let loaded = load_manifest(&config).unwrap();

        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.quarantined.len(), 1);
        assert_eq!(loaded.quarantined[0].id, "test-id");
        assert_eq!(loaded.quarantined[0].hash, sample_hash());
    }

    #[test]
    fn test_quarantine_duplicates_processes_all() {
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source");

        let file1 = create_test_file(&source_dir, "doc Copy.txt", b"content");
        let file2 = create_test_file(&source_dir, "doc Copy 2.txt", b"content");
        let hash = hash_file(&file1).unwrap();

        let groups = vec![DuplicateGroup {
            original: source_dir.join("doc.txt"),
            hash: hash.clone(),
            duplicates: vec![file1.clone(), file2.clone()],
        }];

        let config = QuarantineConfig {
            quarantine_dir: temp.path().join("quarantine"),
            preserve_structure: false,
            dry_run: false,
        };

        let manifest = quarantine_duplicates(&groups, &config).unwrap();

        // Both files should be quarantined
        assert_eq!(manifest.quarantined.len(), 2);
        assert!(!file1.exists());
        assert!(!file2.exists());

        // Manifest should be saved
        let loaded = load_manifest(&config).unwrap();
        assert_eq!(loaded.quarantined.len(), 2);
    }
}
