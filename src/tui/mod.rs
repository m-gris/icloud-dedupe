//! TUI module for interactive terminal interface.
//!
//! Organized along FP/Unix boundaries:
//! - `state`: Pure data types (Screen, Action, Transition)
//! - Future: `update` (pure transitions), `view` (pure rendering), `run` (effects)

pub mod state;
