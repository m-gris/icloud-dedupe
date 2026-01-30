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

Early development. Type design phase.

## License

TBD
