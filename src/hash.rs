//! Content hashing for duplicate verification.
//!
//! Uses BLAKE3 for fast, secure hashing.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

use crate::types::ContentHash;

/// Compute the BLAKE3 hash of a file's contents.
///
/// # Errors
/// Returns an error if the file cannot be read.
pub fn hash_file(path: &Path) -> io::Result<ContentHash> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();

    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(ContentHash(*hash.as_bytes()))
}

/// Check if two files have identical content.
///
/// # Errors
/// Returns an error if either file cannot be read.
pub fn files_match(a: &Path, b: &Path) -> io::Result<bool> {
    let hash_a = hash_file(a)?;
    let hash_b = hash_file(b)?;
    Ok(hash_a == hash_b)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_file_returns_32_bytes() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "hello world").unwrap();

        let hash = hash_file(file.path()).unwrap();
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_identical_content_same_hash() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();

        writeln!(file1, "identical content").unwrap();
        writeln!(file2, "identical content").unwrap();

        let hash1 = hash_file(file1.path()).unwrap();
        let hash2 = hash_file(file2.path()).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_different_content_different_hash() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();

        writeln!(file1, "content A").unwrap();
        writeln!(file2, "content B").unwrap();

        let hash1 = hash_file(file1.path()).unwrap();
        let hash2 = hash_file(file2.path()).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_files_match_identical() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();

        writeln!(file1, "same content").unwrap();
        writeln!(file2, "same content").unwrap();

        assert!(files_match(file1.path(), file2.path()).unwrap());
    }

    #[test]
    fn test_files_match_different() {
        let mut file1 = NamedTempFile::new().unwrap();
        let mut file2 = NamedTempFile::new().unwrap();

        writeln!(file1, "content X").unwrap();
        writeln!(file2, "content Y").unwrap();

        assert!(!files_match(file1.path(), file2.path()).unwrap());
    }

    #[test]
    fn test_hash_nonexistent_file_errors() {
        let result = hash_file(Path::new("/nonexistent/file.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_file() {
        let file = NamedTempFile::new().unwrap();
        // Don't write anything — empty file

        let hash = hash_file(file.path()).unwrap();
        assert_eq!(hash.0.len(), 32);
    }
}
