//! TUI module for interactive terminal interface.
//!
//! Organized along FP/Unix boundaries:
//! - `state`: Pure data types (Screen, Action, Transition)
//! - `update`: Pure state transitions (Screen, Action) → Transition
//! - Future: `view` (pure rendering), `run` (effects)

pub mod state;
pub mod update;
