//! TUI module for interactive terminal interface.
//!
//! Organized along FP/Unix boundaries:
//! - `state`: Pure data types (Screen, Action, Transition)
//! - `update`: Pure state transitions (Screen, Action) → Transition
//! - `theme`: Color semantics and style constants
//! - Future: `view` (pure rendering), `run` (effects)

pub mod run;
pub mod state;
pub mod theme;
pub mod update;
pub mod view;
