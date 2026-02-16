# icloud-dedupe TUI: Design Document

## User Psychology

**Who uses this?**
- Terminal-comfortable users
- People who want control, not magic
- Users burned by "one-click cleaners" that deleted wrong things

**Core anxiety:**
> "Will this delete something important?"

**Design response:**
- Show everything before acting
- Never auto-select
- Make reversal obvious and easy

---

## Design Principles

| Principle | Meaning |
|-----------|---------|
| **Progressive disclosure** | Summary → List → Detail (drill down) |
| **Explicit consent** | User selects, user confirms, then we act |
| **Reversibility as feature** | "Quarantine" not "delete" — undo is first-class |
| **One decision at a time** | Don't overwhelm with choices |

---

## Screen Flow

```
┌──────────┐     ┌───────────┐     ┌────────────┐
│ Scanning │────▶│  Overview │────▶│ List View  │
└──────────┘     └───────────┘     └─────┬──────┘
                      ▲                   │
                      │              ┌────▼──────┐
                      │              │Detail View│
                 ┌────┴───┐         └────┬──────┘
                 │  Done  │              │
                 └────▲───┘         ┌────▼──────┐
                      │             │  Confirm  │
                 ┌────┴───┐         └────┬──────┘
                 │Progress│◀─────────────┘
                 └────────┘
```

---

## Screen 1: Scanning (Transient)

```
┌─────────────────────────────────────────────────────────┐
│  icloud-dedupe                                          │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ◐ Discovering conflict patterns...                    │
│                                                         │
│    Scanning: ~/Library/Mobile Documents/               │
│    Found: 234 candidates                               │
│                                                         │
│                                                         │
│                                                    ^C   │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Spinner shows activity
- Counter provides feedback (not frozen)
- ^C always available to abort

---

## Screen 2: Overview (Landing)

```
┌─────────────────────────────────────────────────────────┐
│  icloud-dedupe                              [q] quit    │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Scan Complete                                          │
│  ═════════════                                          │
│                                                         │
│  ✓  47 confirmed duplicates     3.2 GB recoverable     │
│  ⚠  12 orphaned conflicts       needs review           │
│  ≠   3 diverged files           different content      │
│  ─   5 skipped                  read errors            │
│                                                         │
│  ─────────────────────────────────────────────────────  │
│                                                         │
│  [1] Review duplicates    [2] Review orphans           │
│  [3] Review diverged      [4] View skipped             │
│                                                         │
│                           [q] Quit                      │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Summary first — user sees the whole picture
- Numbers are big and clear (this is the value prop)
- Color coding: ✓ green, ⚠ yellow, ≠ red, ─ dim
- Number keys for quick navigation

---

## Screen 3: Duplicate List (Selection)

```
┌─────────────────────────────────────────────────────────┐
│  Duplicates (47 groups, 3.2 GB)             [Esc] back  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  [x] ▸ report.pdf ─────────────────── 3 copies, 45 MB  │
│  [x]   presentation.key ─────────────  2 copies, 120 MB │
│  [ ]   notes.txt ────────────────────  5 copies, 12 KB  │
│  [x] ▸ project.pages ────────────────  2 copies, 89 MB  │
│  [ ]   budget.numbers ───────────────  2 copies, 34 MB  │
│  [ ]   photo.jpg ────────────────────  4 copies, 8 MB   │
│  ...                                                    │
│                                                         │
│  ─────────────────────────────────────────────────────  │
│  Selected: 3 groups (254 MB)                           │
│                                                         │
│  [Space] toggle  [a] all  [n] none  [Enter] details    │
│  [Q] Quarantine selected                               │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Checkbox metaphor — familiar, clear
- ▸ indicates expandable (has detail view)
- Size always shown (users optimize for space)
- Running tally of selection at bottom
- Capital Q for destructive action (deliberate)

**Navigation:**
- j/k or ↑/↓ — move cursor
- Space — toggle selection
- Enter — drill into details
- a/n — select all / none
- Q — proceed to quarantine (only selected)
- Esc — back to overview

---

## Screen 4: Duplicate Detail

```
┌─────────────────────────────────────────────────────────┐
│  report.pdf                                 [Esc] back  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  KEEP (original):                                       │
│  ┌─────────────────────────────────────────────────┐   │
│  │ ~/Documents/Projects/Q4/report.pdf              │   │
│  │ Modified: 2024-01-15 14:32                      │   │
│  │ Size: 45.2 MB                                   │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  REMOVE (duplicates):                                   │
│  ┌─────────────────────────────────────────────────┐   │
│  │ • report Copy.pdf          ← iCloud conflict    │   │
│  │ • report Copy 2.pdf        ← iCloud conflict    │   │
│  │ • report 2.pdf             ← numbered copy      │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  Hash: 7a3b2c4d...f8e9d0a1 (BLAKE3, verified ✓)        │
│                                                         │
│  [Q] Quarantine  [s] Skip  [o] Open folder  [Esc] back │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Clear visual separation: KEEP vs REMOVE
- Pattern annotation ("iCloud conflict", "numbered copy")
- Hash shown but not emphasized (technical proof)
- "Open folder" for manual verification if paranoid

---

## Screen 5: Confirm (Gate)

```
┌─────────────────────────────────────────────────────────┐
│  Confirm Quarantine                                     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  You're about to quarantine:                           │
│                                                         │
│    12 files (254 MB) from 3 duplicate groups           │
│                                                         │
│  Files will be moved to:                               │
│  ~/Library/Application Support/icloud-dedupe/quarantine│
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │ report Copy.pdf                                 │   │
│  │ report Copy 2.pdf                               │   │
│  │ report 2.pdf                                    │   │
│  │ presentation Copy.key                           │   │
│  │ presentation Copy 2.key                         │   │
│  │ ...7 more                                       │   │
│  └─────────────────────────────────────────────────┘   │
│                                                         │
│  ℹ This is REVERSIBLE. Run `restore --all` to undo.    │
│                                                         │
│           [Y] Yes, quarantine    [N] No, go back       │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Full list (scrollable) — no hidden surprises
- Destination path shown explicitly
- REVERSIBLE in caps — reduce anxiety
- Binary choice: Y or N, nothing else
- N is safe default (accidental Enter = abort)

---

## Screen 6: Progress

```
┌─────────────────────────────────────────────────────────┐
│  Quarantining...                                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  [████████████████████░░░░░░░░░░░░░░░░░░░░]  58%       │
│                                                         │
│  Current: presentation Copy 2.key                      │
│                                                         │
│  ✓  7 moved                                            │
│  ◦  5 remaining                                        │
│                                                         │
│  ⚠  1 failed: budget Copy.numbers (permission denied)  │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Progress bar (users hate staring at nothing)
- Current file being processed
- Running tally: done / remaining
- Errors shown inline, not hidden

---

## Screen 7: Done

```
┌─────────────────────────────────────────────────────────┐
│  Complete                                               │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ✓ 11 files quarantined (251 MB)                       │
│  ⚠  1 file skipped (permission denied)                 │
│                                                         │
│  Space recovered: 251 MB                               │
│                                                         │
│  ─────────────────────────────────────────────────────  │
│                                                         │
│  To undo:   icloud-dedupe restore --all                │
│  To purge:  icloud-dedupe purge                        │
│                                                         │
│  [Enter] Return to overview    [q] Quit                │
└─────────────────────────────────────────────────────────┘
```

**UX notes:**
- Summary of what happened
- Errors acknowledged (not hidden)
- Next steps clearly documented
- User knows how to undo

---

## Keyboard Philosophy

| Key | Action | Notes |
|-----|--------|-------|
| `j` / `↓` | Move down | Vim + arrows |
| `k` / `↑` | Move up | |
| `Space` | Toggle selection | Checkbox metaphor |
| `Enter` | Drill down / confirm | Context-dependent |
| `Esc` | Go back | Always safe |
| `q` | Quit | With confirmation if unsaved |
| `Q` | Destructive action | Capital = deliberate |
| `a` | Select all | Batch operation |
| `n` | Select none | Clear selection |
| `?` | Help | Show keybindings |

---

## Color Semantics

| Color | Meaning | Example |
|-------|---------|---------|
| **Green** | Safe, keep, success | ✓ checkmarks, "KEEP" |
| **Yellow** | Warning, attention | ⚠ orphaned files |
| **Red** | Will be removed | Files to quarantine |
| **Cyan** | Interactive, action | Keybinding hints |
| **Dim** | De-emphasized | Hash, timestamps |
| **Bold** | Important | Counts, filenames |

---

## Anti-Patterns Avoided

| Anti-Pattern | Our Approach |
|--------------|--------------|
| Auto-select everything | Nothing selected by default |
| Hide skip option | Skip always available |
| Surprise deletions | Quarantine, not delete |
| "Are you sure?" spam | One confirmation, after selection |
| Mouse-required | Full keyboard navigation |
| Information hiding | Details always accessible |

---

## State Model (for implementation)

```rust
enum Screen {
    Scanning { candidates_found: usize },
    Overview { report: ScanReport },
    DuplicateList { selected: HashSet<usize>, cursor: usize },
    DuplicateDetail { group_index: usize },
    OrphanList { cursor: usize },
    DivergedList { cursor: usize },
    SkippedList { cursor: usize },
    Confirm { files: Vec<PathBuf> },
    Progress { done: usize, total: usize, current: Option<PathBuf> },
    Done { result: QuarantineResult },
}

enum Action {
    MoveUp,
    MoveDown,
    ToggleSelection,
    SelectAll,
    SelectNone,
    DrillDown,
    GoBack,
    Confirm,
    Quit,
}
```

---

## Open Questions

1. **Orphan handling**: What action for orphaned conflicts?
   - Option A: Just show them (informational)
   - Option B: Offer to delete (they have no original)
   - Option C: Offer to rename (remove conflict suffix)

2. **Diverged handling**: What action for diverged files?
   - Option A: Just show them (informational)
   - Option B: Offer diff view (compare content)
   - Option C: Offer to keep both explicitly

3. **Filtering**: Should list view support filtering?
   - By extension (.pdf, .pages, etc.)
   - By size (> 100 MB)
   - By location (app container)

4. **Sorting**: Default sort order is **by size (biggest first)** — "quick wins"
   - Future: toggle between size / copy count / path

---

## Summary

The TUI should feel like **a trusted advisor**, not an automated cleaner.

It shows you everything, explains what it found, lets you choose what to do, confirms before acting, and makes recovery easy.

**Mantra**: Show, don't hide. Ask, don't assume. Quarantine, don't delete.
