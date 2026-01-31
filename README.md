# icloud-dedupe

A CLI tool for detecting and removing iCloud sync conflict duplicates on macOS.

## Problem

iCloud sync conflicts produce duplicate files with predictable naming patterns:

- `foo Copy.txt`, `foo Copy 2.txt`, `foo Copy 3.txt`
- `foo 2.txt`, `foo 3.txt` (less common)

These accumulate in both user-visible directories (`~/Documents`, `~/Desktop`) and hidden locations (`~/Library/Mobile Documents/`).

## Approach

**Pattern-first, hash-validated.**

Unlike general deduplication tools (fdupes, jdupes) that hash everything to find collisions, this tool:

1. **Pattern match** — find files matching iCloud conflict naming conventions
2. **Derive original** — infer what the non-conflict filename should be
3. **Verify existence** — check the presumed original exists
4. **Hash validate** — confirm content is identical before flagging as duplicate

The hash is validation, not discovery. This is faster and semantically precise — we're finding *iCloud artifacts*, not *all duplicates*.

## Design Principles

- **Type-driven** — model the domain with precise types, let the compiler guide implementation
- **Effects at the edge** — pure detection logic, effects (file moves, deletes) isolated and explicit
- **Reversible operations** — quarantine before delete, manifest for restore
- **Fail-fast** — no silent swallowing of errors, clear contracts

## Quarantine Model

Files are not deleted immediately. Instead:

1. `scan` — produces a report (pure, no side effects)
2. `quarantine` — moves duplicates to local staging area with manifest
3. `purge` — permanently deletes after user confirmation
4. `restore` — moves files back if needed

Quarantine location: `~/Library/Application Support/icloud-dedupe/quarantine/`

This is outside iCloud sync scope — files moved here won't re-sync.

## Status

**Milestones 1-4 complete.** Core scanning works.

- [x] Domain types (`src/types.rs`)
- [x] Pattern detection (`src/pattern.rs`) — pure, no I/O
- [x] Content hashing (`src/hash.rs`) — BLAKE3
- [x] Scanner (`src/scanner.rs`) — parallel verification with rayon
- [ ] Reporting (`src/report.rs`) — in progress
- [ ] Quarantine (`src/quarantine.rs`)
- [ ] CLI (`src/main.rs`)

## Usage

```bash
# Full scan with progress bar and human-readable output
cargo run --example scan ~/Library/Mobile\ Documents/

# Pattern-only discovery (no hashing, fast)
cargo run --example candidates ~/Documents/
```

## API Design

The scanner provides a **decoupled two-phase API** for testability:

```rust
// Phase 1: Pattern matching only (fast, no hashing)
let candidates = find_candidates(&config)?;

// Phase 2: Hash verification (can be parallelized)
for candidate in &candidates {
    match verify_candidate(candidate)? {
        VerificationResult::ConfirmedDuplicate { keep, remove, hash } => { ... }
        VerificationResult::OrphanedConflict { path, .. } => { ... }
        VerificationResult::ContentDiverged { .. } => { ... }
    }
}
```

This separation allows:
- Testing pattern detection without touching the filesystem
- Parallel hash verification with rayon
- Progress reporting between phases

## Path Handling

The scanner handles common path issues:
- Expands `~` to home directory
- Removes redundant `\ ` escapes from shell copy-paste
- Handles macOS bundles (`.pages`, `.logicx`) that appear as directories

## License

TBD
