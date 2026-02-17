> **ATOMIZED** → Epic: icloud-dedupe-kiy.2 | 2026-02-17

# kiy.2 + xv2: Channel-Based Event Architecture + Wire Scanner

## Context

Running `icloud-dedupe` launches the TUI, but scanning still happens in CLI mode with indicatif spinners *before* the TUI starts. The TUI should own the entire lifecycle — start immediately on the Scanning screen, with scanning in a background thread sending progress through a channel.

## Problem

The event loop in `run.rs` blocks on `crossterm::event::read()` — it can't receive background events (scan progress, quarantine progress, etc).

## FP/Unix Separation

Three layers, same principle as before:

| Layer | Module | Responsibility |
|-------|--------|---------------|
| **Pure data** | `state.rs` | Event type that represents all things that can happen to the app |
| **Pure logic** | `update.rs` | Existing user-action transitions stay unchanged. New pure function to handle background events — maps (App state + event) → new App state |
| **Effects shell** | `run.rs` | Channel plumbing, thread spawning, terminal lifecycle. Thin — just wires events to the pure handler |

The key insight: background events (scan progress, scan complete) are *not* user actions, but they still transform App state. That transformation should be a **pure, testable function** — not inlined in the effectful loop.

## Design

### 1. Unified Event Type (state.rs)

A type representing everything the event loop can receive from its channel — key events from the terminal reader thread, and background events from worker threads (scanner now, quarantine later).

### 2. Pure Event Handler (update.rs or new module)

A pure function that takes the current App state and a background event, and returns the new App state. Testable without threads or channels. Handles scan progress updates, scan completion (stores report, transitions to Overview), scan errors.

### 3. Channel Architecture (run.rs)

Two producer threads feeding a single mpsc channel:
- Terminal key reader thread
- Scanner worker thread (discovery + verification, sends progress events)

Event loop: `rx.recv()` in a loop. Key events go through existing `map_key → update` path. Background events go through the new pure handler.

### 4. run() Signature Change

Takes `ScanConfig` instead of `ScanReport`. Starts on Scanning screen, spawns scanner thread immediately.

### 5. main.rs

Default mode (no subcommand) builds `ScanConfig` and passes it to `tui::run::run()`. No more CLI scanning before TUI launch.

## Dual-TDD Execution Sequence

### Phase 1: Types
Define the event type in state.rs. Define the pure handler's signature. Use `todo!()` for the handler body. Compile check.

### Phase 2: Tests
Write tests for the pure event handler — scan progress updates the screen, scan complete stores report and transitions to Overview, scan error is handled. All against the type signatures from Phase 1.

### Phase 3: Red
Run tests, confirm they fail (handler is `todo!()`).

### Phase 4: Impl
Fill in the pure handler. Wire the channel architecture in run.rs. Update main.rs.

### Phase 5: Green
All tests pass — old (128) and new.

### Phase 6: Refactor
Clean up if needed.

## Files to Modify

| File | Change |
|------|--------|
| `src/tui/state.rs` | Add event type |
| `src/tui/update.rs` | Add pure background event handler |
| `src/tui/run.rs` | Channel-based event loop, key reader thread, scanner thread |
| `src/main.rs` | Default mode passes `ScanConfig` to run() |

## Files Unchanged

- `view.rs`, `theme.rs` — pure rendering/style, untouched
- All 128 existing tests pass

## Verification

- `cargo test` — all existing + new tests pass
- `cargo run` — TUI appears immediately, Scanning screen shows live progress, transitions to Overview
- `cargo run -- scan` — batch mode unchanged
