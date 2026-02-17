//! Domain types for icloud-dedupe.
//!
//! Pass 4: Complete types with fields and attributes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// PRIMITIVES
// ============================================================================

/// Content identity — proof that two files are byte-identical.
///
/// Wraps a 32-byte hash (SHA-256 or BLAKE3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    /// Returns the hash as a lowercase hex string.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Serialize for ContentHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex_str = String::deserialize(deserializer)?;
        let bytes = hex_to_bytes(&hex_str).map_err(serde::de::Error::custom)?;
        Ok(ContentHash(bytes))
    }
}

/// Parse a 64-character hex string into 32 bytes.
fn hex_to_bytes(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 {
        return Err(format!("Expected 64 hex chars, got {}", hex.len()));
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        bytes[i] = u8::from_str_radix(s, 16).map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

// ============================================================================
// ENUMS
// ============================================================================

/// The conflict naming patterns iCloud uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictPattern {
    /// "foo Copy.txt", "foo Copy 2.txt"
    /// None = "Copy", Some(2) = "Copy 2"
    Copy { index: Option<u32> },
    /// "foo 2.txt", "foo 3.txt"
    Numbered { index: u32 },
}

/// Classification of file types on macOS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    /// Normal file.
    Regular,
    /// Directory that appears as file (.app, .pages, etc).
    Bundle,
    /// iCloud placeholder (.icloud stub, content not local).
    CloudPlaceholder,
}

/// Terminal states after verifying a conflict candidate.
#[derive(Debug)]
pub enum VerificationResult {
    /// Hash match: true duplicate, safe to remove.
    ConfirmedDuplicate {
        keep: PathBuf,
        remove: PathBuf,
        hash: ContentHash,
    },
    /// Original missing: orphaned conflict file, needs review.
    OrphanedConflict {
        path: PathBuf,
        presumed_original: PathBuf,
    },
    /// Content differs: same naming pattern but NOT a duplicate.
    ContentDiverged {
        conflict_path: PathBuf,
        original_path: PathBuf,
        conflict_hash: ContentHash,
        original_hash: ContentHash,
    },
}

// ============================================================================
// STRUCTS
// ============================================================================

/// A file that matches a conflict naming pattern.
#[derive(Debug)]
pub struct ConflictCandidate {
    /// Path to the conflict file.
    pub path: PathBuf,
    /// The detected conflict pattern.
    pub pattern: ConflictPattern,
    /// Derived path of the presumed original.
    pub presumed_original: PathBuf,
    /// File type classification.
    pub kind: FileKind,
}

/// A group of confirmed duplicates sharing the same content.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    /// The file to keep (clean name).
    pub original: PathBuf,
    /// Content hash proving equivalence.
    pub hash: ContentHash,
    /// Files to remove (conflict-named).
    pub duplicates: Vec<PathBuf>,
}

/// Record of a quarantined file (for restore).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineReceipt {
    /// Unique identifier for this receipt.
    pub id: String,
    /// Original location before quarantine.
    pub original_path: PathBuf,
    /// Current location in quarantine.
    pub quarantine_path: PathBuf,
    /// Content hash at time of quarantine.
    pub hash: ContentHash,
    /// When the file was quarantined (ISO 8601 string).
    pub quarantined_at: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Whether the original had extended attributes.
    pub had_xattrs: bool,
}

/// Complete scan results partitioned by outcome.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScanReport {
    /// Groups of confirmed duplicates.
    pub confirmed_duplicates: Vec<DuplicateGroup>,
    /// Conflict files whose originals are missing.
    pub orphaned_conflicts: Vec<PathBuf>,
    /// Conflict files that differ from their presumed originals.
    pub content_diverged: Vec<(PathBuf, PathBuf)>,
    /// Total bytes recoverable by removing duplicates.
    pub bytes_recoverable: u64,
    /// Files skipped due to read errors (path, error message).
    pub skipped: Vec<(PathBuf, String)>,
}

/// The manifest file tracking quarantined items.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Manifest format version.
    pub version: u32,
    /// All quarantined files.
    pub quarantined: Vec<QuarantineReceipt>,
}

// ============================================================================
// CONFIGURATION
// ============================================================================

/// Output format for reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Human-readable pretty output.
    #[default]
    Human,
    /// Machine-readable JSON.
    Json,
}

/// Configuration for scanning operations.
#[derive(Debug)]
pub struct ScanConfig {
    /// Root directories to scan.
    pub roots: Vec<PathBuf>,
    /// Maximum directory depth (None = unlimited).
    pub max_depth: Option<usize>,
    /// Whether to follow symbolic links.
    pub follow_symlinks: bool,
    /// Include hidden files (dotfiles).
    pub include_hidden: bool,
    /// Case-insensitive pattern matching (for "copy" vs "Copy").
    pub case_insensitive: bool,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            max_depth: None,
            follow_symlinks: false,
            include_hidden: true,
            case_insensitive: true,
        }
    }
}

/// Configuration for quarantine operations.
#[derive(Debug)]
pub struct QuarantineConfig {
    /// Directory to store quarantined files.
    /// Default: ~/Library/Application Support/icloud-dedupe/quarantine/
    pub quarantine_dir: PathBuf,
    /// If true, only report what would be done without moving files.
    pub dry_run: bool,
    /// Preserve directory structure in quarantine.
    pub preserve_structure: bool,
}

impl Default for QuarantineConfig {
    fn default() -> Self {
        Self {
            quarantine_dir: PathBuf::new(), // Will be set at runtime
            dry_run: false,
            preserve_structure: true,
        }
    }
}
