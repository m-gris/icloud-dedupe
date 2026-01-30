//! Domain types for icloud-dedupe.
//!
//! Pass 4: Complete types with fields and attributes.

use std::path::PathBuf;

// ============================================================================
// PRIMITIVES
// ============================================================================

/// Content identity — proof that two files are byte-identical.
///
/// Wraps a 32-byte hash (SHA-256 or BLAKE3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentHash(pub [u8; 32]);

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
#[derive(Debug)]
pub struct DuplicateGroup {
    /// The file to keep (clean name).
    pub original: PathBuf,
    /// Content hash proving equivalence.
    pub hash: ContentHash,
    /// Files to remove (conflict-named).
    pub duplicates: Vec<PathBuf>,
}

/// Record of a quarantined file (for restore).
#[derive(Debug)]
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
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Groups of confirmed duplicates.
    pub confirmed_duplicates: Vec<DuplicateGroup>,
    /// Conflict files whose originals are missing.
    pub orphaned_conflicts: Vec<PathBuf>,
    /// Conflict files that differ from their presumed originals.
    pub content_diverged: Vec<(PathBuf, PathBuf)>,
    /// Total bytes recoverable by removing duplicates.
    pub bytes_recoverable: u64,
}

/// The manifest file tracking quarantined items.
#[derive(Debug, Default)]
pub struct Manifest {
    /// Manifest format version.
    pub version: u32,
    /// All quarantined files.
    pub quarantined: Vec<QuarantineReceipt>,
}
