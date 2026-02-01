//! Report formatting for scan results.
//!
//! Pure functions — (ScanReport, OutputFormat) → String.
//! No I/O, no side effects.

use humansize::{format_size, BINARY};

use crate::types::{OutputFormat, ScanReport};

/// Format a scan report for output.
///
/// Pure function: takes data, returns formatted string.
pub fn format_report(report: &ScanReport, format: OutputFormat) -> String {
    match format {
        OutputFormat::Human => format_human(report),
        OutputFormat::Json => format_json(report),
    }
}

// ============================================================================
// HUMAN FORMAT
// ============================================================================

fn format_human(report: &ScanReport) -> String {
    let mut out = String::new();

    // Confirmed duplicates
    if !report.confirmed_duplicates.is_empty() {
        out.push_str("=== Confirmed Duplicates ===\n");
        for group in &report.confirmed_duplicates {
            out.push_str(&format!("Original: {}\n", group.original.display()));
            for dup in &group.duplicates {
                out.push_str(&format!("  └─ {}\n", dup.display()));
            }
        }
        out.push('\n');
    }

    // Orphaned conflicts
    if !report.orphaned_conflicts.is_empty() {
        out.push_str("=== Orphaned Conflicts (no original found) ===\n");
        for path in &report.orphaned_conflicts {
            out.push_str(&format!("  {}\n", path.display()));
        }
        out.push('\n');
    }

    // Diverged content
    if !report.content_diverged.is_empty() {
        out.push_str("=== Content Diverged (different content) ===\n");
        for (conflict, original) in &report.content_diverged {
            out.push_str(&format!("  {} ≠ {}\n", conflict.display(), original.display()));
        }
        out.push('\n');
    }

    // Skipped files
    if !report.skipped.is_empty() {
        out.push_str("=== Skipped (read errors) ===\n");
        for (path, error) in &report.skipped {
            out.push_str(&format!("  {} - {}\n", path.display(), error));
        }
        out.push('\n');
    }

    // Summary
    out.push_str(&format_summary(report));

    out
}

fn format_summary(report: &ScanReport) -> String {
    let total_duplicates: usize = report
        .confirmed_duplicates
        .iter()
        .map(|g| g.duplicates.len())
        .sum();

    let mut out = String::new();
    out.push_str("=== Summary ===\n");
    out.push_str(&format!(
        "Duplicate groups:   {}\n",
        report.confirmed_duplicates.len()
    ));
    out.push_str(&format!("Total duplicates:   {}\n", total_duplicates));
    out.push_str(&format!(
        "Orphaned conflicts: {}\n",
        report.orphaned_conflicts.len()
    ));
    out.push_str(&format!(
        "Diverged files:     {}\n",
        report.content_diverged.len()
    ));
    if !report.skipped.is_empty() {
        out.push_str(&format!("Skipped (errors):   {}\n", report.skipped.len()));
    }
    out.push_str(&format!(
        "Space recoverable:  {}\n",
        format_size(report.bytes_recoverable, BINARY)
    ));

    out
}

// ============================================================================
// JSON FORMAT
// ============================================================================

fn format_json(report: &ScanReport) -> String {
    // serde_json::to_string_pretty for readable output
    serde_json::to_string_pretty(report).unwrap_or_else(|e| {
        // This should never happen with our types, but fail explicitly
        panic!("Failed to serialize report to JSON: {}", e)
    })
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContentHash, DuplicateGroup};
    use std::path::PathBuf;

    fn sample_hash() -> ContentHash {
        ContentHash([0xab; 32])
    }

    fn sample_report() -> ScanReport {
        ScanReport {
            confirmed_duplicates: vec![DuplicateGroup {
                original: PathBuf::from("/docs/report.txt"),
                hash: sample_hash(),
                duplicates: vec![
                    PathBuf::from("/docs/report Copy.txt"),
                    PathBuf::from("/docs/report Copy 2.txt"),
                ],
            }],
            orphaned_conflicts: vec![PathBuf::from("/old/orphan Copy.txt")],
            content_diverged: vec![(
                PathBuf::from("/work/draft 2.txt"),
                PathBuf::from("/work/draft.txt"),
            )],
            bytes_recoverable: 1024 * 1024 * 5, // 5 MiB
            skipped: vec![(
                PathBuf::from("/locked/file.txt"),
                "Permission denied".to_string(),
            )],
        }
    }

    // --- Human format tests ---

    #[test]
    fn human_format_includes_duplicates() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Human);

        assert!(output.contains("=== Confirmed Duplicates ==="));
        assert!(output.contains("Original: /docs/report.txt"));
        assert!(output.contains("└─ /docs/report Copy.txt"));
        assert!(output.contains("└─ /docs/report Copy 2.txt"));
    }

    #[test]
    fn human_format_includes_orphans() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Human);

        assert!(output.contains("=== Orphaned Conflicts"));
        assert!(output.contains("/old/orphan Copy.txt"));
    }

    #[test]
    fn human_format_includes_diverged() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Human);

        assert!(output.contains("=== Content Diverged"));
        assert!(output.contains("/work/draft 2.txt"));
        assert!(output.contains("≠"));
    }

    #[test]
    fn human_format_includes_skipped() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Human);

        assert!(output.contains("=== Skipped"));
        assert!(output.contains("/locked/file.txt"));
        assert!(output.contains("Permission denied"));
    }

    #[test]
    fn human_format_includes_summary() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Human);

        assert!(output.contains("=== Summary ==="));
        assert!(output.contains("Duplicate groups:   1"));
        assert!(output.contains("Total duplicates:   2"));
        assert!(output.contains("Orphaned conflicts: 1"));
        assert!(output.contains("Diverged files:     1"));
        assert!(output.contains("5 MiB")); // humansize output
    }

    #[test]
    fn human_format_empty_report() {
        let report = ScanReport::default();
        let output = format_report(&report, OutputFormat::Human);

        // Should only have summary, no sections
        assert!(!output.contains("=== Confirmed Duplicates"));
        assert!(!output.contains("=== Orphaned"));
        assert!(!output.contains("=== Content Diverged"));
        assert!(!output.contains("=== Skipped"));
        assert!(output.contains("=== Summary ==="));
        assert!(output.contains("Duplicate groups:   0"));
    }

    // --- JSON format tests ---

    #[test]
    fn json_format_is_valid_json() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Json);

        // Should parse as valid JSON
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("Invalid JSON");
        assert!(parsed.is_object());
    }

    #[test]
    fn json_format_has_expected_fields() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert!(parsed["confirmed_duplicates"].is_array());
        assert!(parsed["orphaned_conflicts"].is_array());
        assert!(parsed["content_diverged"].is_array());
        assert!(parsed["bytes_recoverable"].is_number());
        assert!(parsed["skipped"].is_array());
    }

    #[test]
    fn json_format_hash_is_hex_string() {
        let report = sample_report();
        let output = format_report(&report, OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        let hash = &parsed["confirmed_duplicates"][0]["hash"];
        assert!(hash.is_string());
        // Our sample_hash is [0xab; 32], so hex is "abab...ab" (64 chars)
        assert_eq!(hash.as_str().unwrap().len(), 64);
        assert!(hash.as_str().unwrap().chars().all(|c| c == 'a' || c == 'b'));
    }

    #[test]
    fn json_format_empty_report() {
        let report = ScanReport::default();
        let output = format_report(&report, OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["confirmed_duplicates"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["bytes_recoverable"], 0);
    }
}
