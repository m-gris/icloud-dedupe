# icloud-dedupe: Development Plan

## Philosophy

**Type-Driven**: Define types first. Let the compiler guide us. Make illegal states unrepresentable.

**Whole-Driven**: Understand the complete domain before coding parts. Top-down design, bottom-up implementation.

**FP-Unix**: Small composable pieces. Pure core, effects at edges. Each module does one thing.

**Test-Driven**: Tests as specification. Write tests alongside (or before) implementation.

---

## Architecture: Separation of Concerns

```
┌─────────────────────────────────────────────────────────┐
│                        CLI (glue)                       │
├─────────────────────────────────────────────────────────┤
│  Reporting (pure)  │  Quarantine (effect)  │  Config   │
├────────────────────┴───────────────────────┴───────────┤
│              Scanner (read-only effect)                 │
├─────────────────────────────────────────────────────────┤
│   Pattern Detection (pure)  │  Hashing (read effect)   │
├─────────────────────────────┴───────────────────────────┤
│                    Domain Types (data)                  │
└─────────────────────────────────────────────────────────┘
```

- **Bottom layers**: Pure, easily testable
- **Middle layers**: Read-only effects (scanning, hashing)
- **Top layers**: Write effects (quarantine), pushed to the edge

---

## Incremental Milestones

### Milestone 1: Domain Types
**Goal**: Define the language of the problem. No logic yet.

Files:
- `src/types.rs` — core domain types

Deliverables:
- `ConflictPattern` enum
- `ConflictCandidate` struct
- `ContentHash` newtype
- `VerificationResult` enum (ConfirmedDuplicate, Orphaned, Diverged)
- `FileKind` enum (Regular, Bundle, CloudPlaceholder)
- `QuarantineReceipt` struct
- `ScanReport` struct

Verification:
- `cargo check` passes
- Types compile and are well-documented

---

### Milestone 2: Pattern Detection (Pure)
**Goal**: Given a filename, detect conflict patterns and derive presumed original.

Files:
- `src/pattern.rs` — pattern matching logic

Functions:
- `fn detect_pattern(filename: &str) -> Option<ConflictPattern>`
- `fn derive_original(path: &Path, pattern: &ConflictPattern) -> PathBuf`
- `fn is_conflict_file(filename: &str) -> bool`

Verification:
- Unit tests for all patterns:
  - "foo Copy.txt" → Copy { index: None }, original: "foo.txt"
  - "foo Copy 2.txt" → Copy { index: Some(2) }, original: "foo.txt"
  - "foo 2.txt" → Numbered { index: 2 }, original: "foo.txt"
  - Edge cases: "Copy.txt", "foo copy.txt" (lowercase), etc.

---

### Milestone 3: File Hashing (Read Effect)
**Goal**: Compute content hash for a file.

Files:
- `src/hash.rs` — hashing logic

Dependencies:
- `blake3` (fast, modern)

Functions:
- `fn hash_file(path: &Path) -> io::Result<ContentHash>`
- `fn files_match(a: &Path, b: &Path) -> io::Result<bool>`

Verification:
- Unit tests with temp files
- Test that identical content → identical hash
- Test that different content → different hash

---

### Milestone 4: Scanner (Read Effect)
**Goal**: Walk directory tree, find conflict candidates, verify originals, validate hashes.

Files:
- `src/scanner.rs` — directory traversal and candidate discovery

Dependencies:
- `walkdir` (directory traversal)
- `ignore` (respect .gitignore patterns, optional)

Functions:
- `fn scan(root: &Path, config: &ScanConfig) -> ScanResult`
- Internal: candidate discovery, original existence check, hash validation

Types:
- `ScanConfig` (roots, max_depth, patterns to match)
- `ScanResult` (candidates found, verified, report)

Verification:
- Integration test with temp directory structure
- Create known conflict files, verify detection
- Test bundle detection (.pages, .app directories)
- Test .icloud placeholder detection

---

### Milestone 5: Reporting (Pure)
**Goal**: Transform scan results into human-readable output.

Files:
- `src/report.rs` — output formatting

Functions:
- `fn format_report(report: &ScanReport, format: OutputFormat) -> String`
- Formats: Human (pretty), JSON (machine-readable)

Verification:
- Unit tests: given ScanReport, verify output string
- No side effects — pure transformation

---

### Milestone 6: Quarantine (Write Effect)
**Goal**: Move duplicates to quarantine, write manifest, support restore.

Files:
- `src/quarantine.rs` — quarantine operations

Functions:
- `fn quarantine_files(duplicates: &[ConfirmedDuplicate], config: &QuarantineConfig) -> Result<Manifest>`
- `fn restore_file(receipt: &QuarantineReceipt) -> Result<()>`
- `fn purge_quarantine(manifest: &Manifest) -> Result<()>`
- `fn quarantine_path() -> PathBuf` (~/Library/Application Support/icloud-dedupe/quarantine/)

Verification:
- Integration tests with temp directories
- Verify manifest written correctly
- Verify restore works
- Verify purge deletes files

---

### Milestone 7: CLI
**Goal**: Wire everything together with a proper CLI interface.

Files:
- `src/main.rs` — CLI entry point
- `src/cli.rs` — argument parsing (optional, could be in main)

Dependencies:
- `clap` (argument parsing)

Subcommands:
- `scan <path>` — detect and report (no modifications)
- `quarantine <path>` — move duplicates to quarantine
- `restore [--all | <id>]` — restore from quarantine
- `purge` — permanently delete quarantined files
- `status` — show quarantine contents

Verification:
- End-to-end test: create test directory, run scan, verify output
- Manual testing on real iCloud directories

---

## File Structure (Target)

```
src/
  main.rs        # CLI entry point
  lib.rs         # Library root, re-exports
  types.rs       # Domain types
  pattern.rs     # Pattern detection (pure)
  hash.rs        # Content hashing
  scanner.rs     # Directory scanning
  report.rs      # Output formatting (pure)
  quarantine.rs  # Quarantine operations
tests/
  pattern_tests.rs
  scanner_tests.rs
  integration_tests.rs
```

---

## Dependency Strategy

Minimal, well-chosen dependencies:

| Crate | Purpose | Phase |
|-------|---------|-------|
| `blake3` | Fast hashing | Milestone 3 |
| `walkdir` | Directory traversal | Milestone 4 |
| `clap` | CLI parsing | Milestone 7 |
| `serde`, `serde_json` | Manifest serialization | Milestone 6 |
| `dirs` | Platform paths | Milestone 6 |
| `thiserror` | Error types | Milestone 1 |

Add dependencies only when needed, not upfront.

---

## Testing Strategy

1. **Unit tests**: In each module, test pure functions
2. **Integration tests**: In `tests/`, test with real filesystem (temp dirs)
3. **Property tests** (optional): Use `proptest` for pattern detection edge cases
