//! TUI state algebra: pure types, zero effects.
//!
//! These types define the entire TUI state space. They are the spec:
//! illegal states should be unrepresentable. The transition function
//! (th0.2) and rendering layer (th0.3) both program against these types.
//!
//! Design principle: Screen variants carry only per-screen transient state
//! (cursor positions, selections). Shared data (ScanReport) lives in App.
//! Viewport scroll offsets are derived during rendering, not stored here.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crossterm::event::KeyEvent;

use crate::types::ScanReport;

// ============================================================================
// APP EVENTS
// ============================================================================

/// Everything the event loop can receive from its channel.
///
/// Two producers feed a single mpsc channel:
/// - A key reader thread sends `Key` variants
/// - Worker threads (scanner, quarantine) send background variants
///
/// The event loop dispatches: Key events go through `map_key → update`,
/// background events go through a separate pure handler.
#[derive(Debug)]
pub enum AppEvent {
    /// A terminal key event from the crossterm reader thread.
    Key(KeyEvent),
    /// Scanner progress: files scanned so far, candidates found so far.
    ScanProgress { files_scanned: usize, candidates_found: usize },
    /// Scanner finished successfully with a complete report.
    ScanComplete(ScanReport),
    /// Scanner failed with an error message.
    ScanError(String),
}

// ============================================================================
// APPLICATION STATE
// ============================================================================

/// Top-level TUI model.
///
/// Owns the shared data (scan report) and the current screen.
/// The effects layer reads this to know what to render.
#[derive(Debug)]
pub struct App {
    /// Current screen — carries per-screen navigation/selection state.
    pub screen: Screen,

    /// Scan results, shared across screens. None while scanning.
    pub report: Option<ScanReport>,

    /// Set to true when the app should exit on the next tick.
    pub should_quit: bool,
}

// ============================================================================
// SCREENS
// ============================================================================

/// The current TUI screen.
///
/// Each variant is a state in the navigation state machine.
/// Variants carry only per-screen transient state — cursors, selections,
/// counters. Shared data (the scan report) lives in [`App::report`].
#[derive(Debug, PartialEq)]
pub enum Screen {
    /// Scan in progress. Counter updated via callback.
    Scanning {
        candidates_found: usize,
    },

    /// Summary dashboard after scan completes.
    /// No per-screen state — everything derived from App.report.
    Overview,

    /// Browsable list of duplicate groups with checkbox selection.
    DuplicateList {
        /// Focused row index.
        cursor: usize,
        /// Indices of groups selected for quarantine.
        selected: BTreeSet<usize>,
    },

    /// Detail view for a single duplicate group.
    DuplicateDetail {
        /// Index into report.confirmed_duplicates.
        group_index: usize,
    },

    /// Orphaned conflict files (informational in v1).
    OrphanList {
        cursor: usize,
    },

    /// Diverged file pairs (informational in v1).
    DivergedList {
        cursor: usize,
    },

    /// Skipped files with error reasons.
    SkippedList {
        cursor: usize,
    },

    /// Confirmation gate before quarantine.
    Confirm {
        /// Duplicate group indices being quarantined.
        group_indices: Vec<usize>,
    },

    /// Quarantine in progress.
    Progress {
        done: usize,
        total: usize,
        current: Option<PathBuf>,
        errors: Vec<(PathBuf, String)>,
    },

    /// Operation complete.
    Done {
        quarantined: usize,
        failed: usize,
        bytes_recovered: u64,
        errors: Vec<(PathBuf, String)>,
    },
}

/// Default screen is Overview (used as placeholder during transitions).
impl Default for Screen {
    fn default() -> Self {
        Screen::Overview
    }
}

// ============================================================================
// ACTIONS
// ============================================================================

/// Semantic user action, decoupled from raw key events.
///
/// The effects layer maps key presses to Actions.
/// The transition function decides what each Action means per Screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Move cursor up in a list.
    MoveUp,
    /// Move cursor down in a list.
    MoveDown,
    /// Toggle checkbox on current item.
    ToggleSelection,
    /// Select all items in current list.
    SelectAll,
    /// Deselect all items in current list.
    SelectNone,
    /// Drill into detail / enter a category.
    Enter,
    /// Navigate back to previous screen.
    Back,
    /// Navigate to a category by number (1-4 on Overview).
    NumberKey(u8),
    /// Initiate quarantine (capital Q — deliberate).
    Quarantine,
    /// Skip current item.
    Skip,
    /// Open containing folder in system file manager.
    OpenFolder,
    /// Confirm action (Y on confirmation screen).
    ConfirmYes,
    /// Decline action (N on confirmation screen).
    ConfirmNo,
    /// Quit the application.
    Quit,
}

// ============================================================================
// TRANSITIONS
// ============================================================================

/// Result of a pure state transition.
///
/// The update function returns this. The effects boundary inspects it
/// to decide what to render and which side effects to execute.
/// Follows the Elm/TEA pattern: pure code describes WHAT should happen,
/// effectful code decides HOW.
#[derive(Debug, PartialEq)]
pub enum Transition {
    /// Render this screen (may be the same or a different screen).
    Screen(Screen),
    /// Quit the application.
    Quit,
    /// Execute a side effect. The effects layer handles it
    /// and updates App state as the effect progresses.
    Effect(Effect),
}

/// Side effect requested by a pure transition.
///
/// Pure code never executes these — it only describes them.
/// The effects boundary interprets them.
#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Move selected duplicate groups to quarantine.
    StartQuarantine {
        /// Indices into report.confirmed_duplicates.
        group_indices: Vec<usize>,
    },
    /// Open a path in the system file manager.
    OpenFolder {
        path: PathBuf,
    },
}

// ============================================================================
// CONSTRUCTORS
// ============================================================================

impl App {
    /// Create an App in the Scanning state (before scan completes).
    pub fn scanning() -> Self {
        App {
            screen: Screen::Scanning { candidates_found: 0 },
            report: None,
            should_quit: false,
        }
    }

    /// Create an App with a completed scan report, landing on Overview.
    pub fn with_report(report: ScanReport) -> Self {
        App {
            screen: Screen::Overview,
            report: Some(report),
            should_quit: false,
        }
    }
}

impl Screen {
    /// Create a DuplicateList with cursor at top and nothing selected.
    pub fn duplicate_list() -> Self {
        Screen::DuplicateList {
            cursor: 0,
            selected: BTreeSet::new(),
        }
    }

    /// Create a detail view for a specific group.
    pub fn duplicate_detail(group_index: usize) -> Self {
        Screen::DuplicateDetail { group_index }
    }

    /// Create a Confirm screen for the given group indices.
    pub fn confirm(group_indices: Vec<usize>) -> Self {
        Screen::Confirm { group_indices }
    }

    /// Create a Progress screen with the given total.
    pub fn progress(total: usize) -> Self {
        Screen::Progress {
            done: 0,
            total,
            current: None,
            errors: Vec::new(),
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_scanning_starts_with_zero_candidates() {
        let app = App::scanning();
        assert_eq!(app.screen, Screen::Scanning { candidates_found: 0 });
        assert!(app.report.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn app_with_report_lands_on_overview() {
        let report = ScanReport::default();
        let app = App::with_report(report);
        assert_eq!(app.screen, Screen::Overview);
        assert!(app.report.is_some());
    }

    #[test]
    fn duplicate_list_starts_empty() {
        let screen = Screen::duplicate_list();
        assert_eq!(
            screen,
            Screen::DuplicateList {
                cursor: 0,
                selected: BTreeSet::new(),
            }
        );
    }

    #[test]
    fn progress_screen_starts_at_zero() {
        let screen = Screen::progress(12);
        assert_eq!(
            screen,
            Screen::Progress {
                done: 0,
                total: 12,
                current: None,
                errors: Vec::new(),
            }
        );
    }

    #[test]
    fn screen_default_is_overview() {
        assert_eq!(Screen::default(), Screen::Overview);
    }

    #[test]
    fn action_equality_for_matching() {
        // Actions need Eq for the transition function to pattern-match
        assert_eq!(Action::MoveUp, Action::MoveUp);
        assert_ne!(Action::MoveUp, Action::MoveDown);
        assert_eq!(Action::NumberKey(1), Action::NumberKey(1));
        assert_ne!(Action::NumberKey(1), Action::NumberKey(2));
    }

    #[test]
    fn transition_variants_are_distinguishable() {
        let t1 = Transition::Screen(Screen::Overview);
        let t2 = Transition::Quit;
        let t3 = Transition::Effect(Effect::OpenFolder {
            path: PathBuf::from("/tmp"),
        });

        assert_ne!(t1, t2);
        assert_ne!(t2, t3);
    }

    #[test]
    fn confirm_screen_carries_group_indices() {
        let screen = Screen::confirm(vec![0, 3, 7]);
        match screen {
            Screen::Confirm { group_indices } => {
                assert_eq!(group_indices, vec![0, 3, 7]);
            }
            _ => panic!("Expected Confirm variant"),
        }
    }
}
