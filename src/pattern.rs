//! Pattern detection for iCloud conflict files.
//!
//! Pure functions — no I/O, easily testable.

use std::path::{Path, PathBuf};

use crate::types::ConflictPattern;

/// Minimum index for conflict patterns.
///
/// iCloud creates conflicts starting at "Copy" (implicit 1), then "Copy 2", "Copy 3", etc.
/// Similarly for numbered: "file 2.txt", "file 3.txt".
/// Index 1 is considered the original, so conflicts start at 2.
const MIN_CONFLICT_INDEX: u32 = 2;

/// Detect if a filename matches an iCloud conflict pattern.
///
/// Returns `Some(pattern)` if the filename matches, `None` otherwise.
///
/// # Patterns recognized
/// - "foo Copy.ext" → `Copy { index: None }`
/// - "foo Copy 2.ext" → `Copy { index: Some(2) }`
/// - "foo 2.ext" → `Numbered { index: 2 }`
pub fn detect_pattern(filename: &str) -> Option<ConflictPattern> {
    // Try "Copy" pattern first (more specific)
    if let Some(pattern) = detect_copy_pattern(filename) {
        return Some(pattern);
    }

    // Try numbered pattern
    if let Some(pattern) = detect_numbered_pattern(filename) {
        return Some(pattern);
    }

    None
}

/// Derive the presumed original filename from a conflict file.
///
/// Given "foo Copy 2.txt" returns "foo.txt".
/// Given "bar 3.pdf" returns "bar.pdf".
pub fn derive_original(path: &Path, pattern: &ConflictPattern) -> PathBuf {
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let original_filename = derive_original_filename(filename, pattern);

    path.with_file_name(original_filename)
}

/// Convenience function: check if a filename is a conflict file.
pub fn is_conflict_file(filename: &str) -> bool {
    detect_pattern(filename).is_some()
}

// ============================================================================
// INTERNAL: Copy pattern ("foo Copy.txt", "foo Copy 2.txt")
// ============================================================================

fn detect_copy_pattern(filename: &str) -> Option<ConflictPattern> {
    let filename_lower = filename.to_lowercase();

    // Pattern: " copy N." or " copy N" at end (N >= 2)
    // Look for " copy " followed by digits, then optional extension
    if let Some(pos) = filename_lower.rfind(" copy ") {
        let after_copy = &filename[pos + 6..]; // skip " copy "
        // Extract the number (everything before the first '.' or end)
        let num_part = after_copy.split('.').next().unwrap_or("");
        if let Ok(index) = num_part.trim().parse::<u32>() {
            if index >= MIN_CONFLICT_INDEX {
                return Some(ConflictPattern::Copy { index: Some(index) });
            }
        }
    }

    // Pattern: " copy." or " copy" at end (no number)
    // Check for " copy." followed by extension, or " copy" at very end
    if let Some(pos) = filename_lower.rfind(" copy.") {
        // Verify there's an extension after the dot
        let after_dot = &filename[pos + 6..];
        if !after_dot.is_empty() {
            return Some(ConflictPattern::Copy { index: None });
        }
    }

    // Check for " copy" at very end (no extension)
    if filename_lower.ends_with(" copy") {
        return Some(ConflictPattern::Copy { index: None });
    }

    None
}

fn derive_original_from_copy(filename: &str, index: Option<u32>) -> String {
    let filename_lower = filename.to_lowercase();

    if index.is_some() {
        // "foo Copy 2.txt" → find " copy " and take everything before + extension after number
        if let Some(pos) = filename_lower.rfind(" copy ") {
            let before = &filename[..pos];
            let after_copy = &filename[pos + 6..]; // skip " copy "
            // Find the first '.' after the number to get extension
            if let Some(dot_pos) = after_copy.find('.') {
                let ext = &after_copy[dot_pos..];
                return format!("{}{}", before, ext);
            } else {
                return before.to_string();
            }
        }
    } else {
        // "foo Copy.txt" → find " copy." and take everything before + extension
        if let Some(pos) = filename_lower.rfind(" copy.") {
            let before = &filename[..pos];
            let ext = &filename[pos + 5..]; // skip " copy", keep ".ext"
            return format!("{}{}", before, ext);
        }
        // "foo Copy" (no extension)
        if let Some(pos) = filename_lower.rfind(" copy") {
            return filename[..pos].to_string();
        }
    }

    filename.to_string()
}

// ============================================================================
// INTERNAL: Numbered pattern ("foo 2.txt", "foo 3.txt")
// ============================================================================

fn detect_numbered_pattern(filename: &str) -> Option<ConflictPattern> {
    let (stem, _ext) = split_filename(filename);

    // Pattern: " N" at end of stem where N >= 2
    // Must have a space before the number
    if let Some(pos) = stem.rfind(' ') {
        let after_space = &stem[pos + 1..];
        if let Ok(index) = after_space.parse::<u32>() {
            if index >= MIN_CONFLICT_INDEX {
                return Some(ConflictPattern::Numbered { index });
            }
        }
    }

    None
}

fn derive_original_from_numbered(filename: &str) -> String {
    let (stem, ext) = split_filename(filename);

    // "foo 2" → find last " " and take everything before
    let original_stem = if let Some(pos) = stem.rfind(' ') {
        &stem[..pos]
    } else {
        stem
    };

    if ext.is_empty() {
        original_stem.to_string()
    } else {
        format!("{}.{}", original_stem, ext)
    }
}

// ============================================================================
// INTERNAL: Helpers
// ============================================================================

/// Split filename into stem and extension.
/// "foo.txt" → ("foo", "txt")
/// "foo" → ("foo", "")
/// "foo.tar.gz" → ("foo.tar", "gz")
fn split_filename(filename: &str) -> (&str, &str) {
    match filename.rfind('.') {
        Some(pos) if pos > 0 => (&filename[..pos], &filename[pos + 1..]),
        _ => (filename, ""),
    }
}

fn derive_original_filename(filename: &str, pattern: &ConflictPattern) -> String {
    match pattern {
        ConflictPattern::Copy { index } => derive_original_from_copy(filename, *index),
        ConflictPattern::Numbered { .. } => derive_original_from_numbered(filename),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_pattern tests ---

    #[test]
    fn test_copy_no_index() {
        assert_eq!(
            detect_pattern("foo Copy.txt"),
            Some(ConflictPattern::Copy { index: None })
        );
    }

    #[test]
    fn test_copy_with_index() {
        assert_eq!(
            detect_pattern("foo Copy 2.txt"),
            Some(ConflictPattern::Copy { index: Some(2) })
        );
        assert_eq!(
            detect_pattern("foo Copy 3.txt"),
            Some(ConflictPattern::Copy { index: Some(3) })
        );
    }

    #[test]
    fn test_copy_case_insensitive() {
        assert_eq!(
            detect_pattern("foo copy.txt"),
            Some(ConflictPattern::Copy { index: None })
        );
        assert_eq!(
            detect_pattern("foo COPY 2.txt"),
            Some(ConflictPattern::Copy { index: Some(2) })
        );
    }

    #[test]
    fn test_numbered_pattern() {
        assert_eq!(
            detect_pattern("foo 2.txt"),
            Some(ConflictPattern::Numbered { index: 2 })
        );
        assert_eq!(
            detect_pattern("foo 3.txt"),
            Some(ConflictPattern::Numbered { index: 3 })
        );
    }

    #[test]
    fn test_numbered_requires_minimum_2() {
        // "foo 1.txt" should NOT match — index 1 is original
        assert_eq!(detect_pattern("foo 1.txt"), None);
    }

    #[test]
    fn test_no_match() {
        assert_eq!(detect_pattern("foo.txt"), None);
        assert_eq!(detect_pattern("Copy.txt"), None); // "Copy" IS the name
        assert_eq!(detect_pattern("foobar.txt"), None);
    }

    #[test]
    fn test_no_extension() {
        assert_eq!(
            detect_pattern("foo Copy"),
            Some(ConflictPattern::Copy { index: None })
        );
        assert_eq!(
            detect_pattern("foo 2"),
            Some(ConflictPattern::Numbered { index: 2 })
        );
    }

    // --- derive_original tests ---

    #[test]
    fn test_derive_from_copy_no_index() {
        let path = Path::new("/some/dir/foo Copy.txt");
        let pattern = ConflictPattern::Copy { index: None };
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("/some/dir/foo.txt")
        );
    }

    #[test]
    fn test_derive_from_copy_with_index() {
        let path = Path::new("/some/dir/foo Copy 2.txt");
        let pattern = ConflictPattern::Copy { index: Some(2) };
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("/some/dir/foo.txt")
        );
    }

    #[test]
    fn test_derive_from_numbered() {
        let path = Path::new("/some/dir/foo 2.txt");
        let pattern = ConflictPattern::Numbered { index: 2 };
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("/some/dir/foo.txt")
        );
    }

    #[test]
    fn test_derive_preserves_directory() {
        let path = Path::new("/Users/marc/Documents/report Copy 3.pdf");
        let pattern = ConflictPattern::Copy { index: Some(3) };
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("/Users/marc/Documents/report.pdf")
        );
    }

    // --- is_conflict_file tests ---

    #[test]
    fn test_is_conflict_file() {
        assert!(is_conflict_file("foo Copy.txt"));
        assert!(is_conflict_file("foo Copy 2.txt"));
        assert!(is_conflict_file("foo 2.txt"));
        assert!(!is_conflict_file("foo.txt"));
        assert!(!is_conflict_file("Copy.txt"));
    }

    // --- Edge cases ---

    #[test]
    fn test_multiple_extensions() {
        assert_eq!(
            detect_pattern("archive Copy.tar.gz"),
            Some(ConflictPattern::Copy { index: None })
        );
        let path = Path::new("archive Copy.tar.gz");
        let pattern = ConflictPattern::Copy { index: None };
        // Should become "archive.tar.gz" (only last extension considered)
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("archive.tar.gz")
        );
    }

    #[test]
    fn test_spaces_in_name() {
        // "my file Copy.txt" → "my file.txt"
        assert_eq!(
            detect_pattern("my file Copy.txt"),
            Some(ConflictPattern::Copy { index: None })
        );
        let path = Path::new("my file Copy.txt");
        let pattern = ConflictPattern::Copy { index: None };
        assert_eq!(
            derive_original(path, &pattern),
            PathBuf::from("my file.txt")
        );
    }
}
