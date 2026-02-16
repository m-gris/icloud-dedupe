//! Pure state transitions: (Screen, Action) → Transition.
//!
//! This is the core logic of the TUI. Fully testable without a terminal.
//! Each screen defines which actions it accepts. Unhandled actions
//! return the current screen unchanged (no-op).

use crate::types::ScanReport;

use super::state::{Action, Effect, Screen, Transition};

/// Pure state transition function.
///
/// Given the current screen, an action, and a read-only view of the
/// scan report, produces the next transition. The effects boundary
/// interprets the result.
pub fn update(screen: Screen, action: &Action, report: &ScanReport) -> Transition {
    match screen {
        Screen::Scanning { .. } => update_scanning(screen, action),
        Screen::Overview => update_overview(action, report),
        Screen::DuplicateList { cursor, selected } => {
            update_duplicate_list(cursor, selected, action, report)
        }
        Screen::DuplicateDetail { group_index } => {
            update_duplicate_detail(group_index, action, report)
        }
        Screen::OrphanList { cursor } => update_simple_list(
            cursor,
            report.orphaned_conflicts.len(),
            action,
            |c| Screen::OrphanList { cursor: c },
        ),
        Screen::DivergedList { cursor } => update_simple_list(
            cursor,
            report.content_diverged.len(),
            action,
            |c| Screen::DivergedList { cursor: c },
        ),
        Screen::SkippedList { cursor } => update_simple_list(
            cursor,
            report.skipped.len(),
            action,
            |c| Screen::SkippedList { cursor: c },
        ),
        Screen::Confirm { group_indices } => update_confirm(group_indices, action),
        // Progress and Done are driven by the effects layer, not user actions
        // (except Quit and navigation)
        Screen::Progress { .. } => noop(screen, action),
        Screen::Done { .. } => update_done(screen, action),
    }
}

// ============================================================================
// PER-SCREEN HANDLERS
// ============================================================================

/// Scanning: only Quit is meaningful. Everything else is a no-op.
fn update_scanning(screen: Screen, action: &Action) -> Transition {
    match action {
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(screen),
    }
}

/// Overview: number keys navigate to category lists.
fn update_overview(action: &Action, report: &ScanReport) -> Transition {
    match action {
        Action::NumberKey(1) | Action::Enter => {
            if report.confirmed_duplicates.is_empty() {
                Transition::Screen(Screen::Overview)
            } else {
                Transition::Screen(Screen::duplicate_list())
            }
        }
        Action::NumberKey(2) => {
            if report.orphaned_conflicts.is_empty() {
                Transition::Screen(Screen::Overview)
            } else {
                Transition::Screen(Screen::OrphanList { cursor: 0 })
            }
        }
        Action::NumberKey(3) => {
            if report.content_diverged.is_empty() {
                Transition::Screen(Screen::Overview)
            } else {
                Transition::Screen(Screen::DivergedList { cursor: 0 })
            }
        }
        Action::NumberKey(4) => {
            if report.skipped.is_empty() {
                Transition::Screen(Screen::Overview)
            } else {
                Transition::Screen(Screen::SkippedList { cursor: 0 })
            }
        }
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(Screen::Overview),
    }
}

/// DuplicateList: cursor movement, selection, drill-down, quarantine.
fn update_duplicate_list(
    cursor: usize,
    selected: std::collections::BTreeSet<usize>,
    action: &Action,
    report: &ScanReport,
) -> Transition {
    let len = report.confirmed_duplicates.len();

    match action {
        Action::MoveUp => {
            let new_cursor = cursor.saturating_sub(1);
            Transition::Screen(Screen::DuplicateList {
                cursor: new_cursor,
                selected,
            })
        }
        Action::MoveDown => {
            let new_cursor = if len == 0 { 0 } else { (cursor + 1).min(len - 1) };
            Transition::Screen(Screen::DuplicateList {
                cursor: new_cursor,
                selected,
            })
        }
        Action::Enter => {
            if cursor < len {
                Transition::Screen(Screen::duplicate_detail(cursor))
            } else {
                Transition::Screen(Screen::DuplicateList { cursor, selected })
            }
        }
        Action::ToggleSelection => {
            let mut new_selected = selected;
            if new_selected.contains(&cursor) {
                new_selected.remove(&cursor);
            } else {
                new_selected.insert(cursor);
            }
            Transition::Screen(Screen::DuplicateList {
                cursor,
                selected: new_selected,
            })
        }
        Action::SelectAll => {
            let all: std::collections::BTreeSet<usize> = (0..len).collect();
            Transition::Screen(Screen::DuplicateList {
                cursor,
                selected: all,
            })
        }
        Action::SelectNone => Transition::Screen(Screen::DuplicateList {
            cursor,
            selected: std::collections::BTreeSet::new(),
        }),
        Action::Quarantine => {
            if selected.is_empty() {
                // Nothing selected — no-op
                Transition::Screen(Screen::DuplicateList { cursor, selected })
            } else {
                let group_indices: Vec<usize> = selected.into_iter().collect();
                Transition::Screen(Screen::confirm(group_indices))
            }
        }
        Action::Back => Transition::Screen(Screen::Overview),
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(Screen::DuplicateList { cursor, selected }),
    }
}

/// DuplicateDetail: back to list, quarantine single group, open folder.
fn update_duplicate_detail(
    group_index: usize,
    action: &Action,
    report: &ScanReport,
) -> Transition {
    match action {
        Action::Back | Action::Skip => Transition::Screen(Screen::duplicate_list()),
        Action::Quarantine => {
            // Quarantine this single group directly (skip confirm? or go to confirm)
            // Design says Q on detail screen quarantines — go through confirm gate.
            Transition::Screen(Screen::confirm(vec![group_index]))
        }
        Action::OpenFolder => {
            if let Some(group) = report.confirmed_duplicates.get(group_index) {
                // Open the folder containing the original file
                if let Some(parent) = group.original.parent() {
                    Transition::Effect(Effect::OpenFolder {
                        path: parent.to_path_buf(),
                    })
                } else {
                    Transition::Screen(Screen::DuplicateDetail { group_index })
                }
            } else {
                Transition::Screen(Screen::DuplicateDetail { group_index })
            }
        }
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(Screen::DuplicateDetail { group_index }),
    }
}

/// Simple list (orphans, diverged, skipped): cursor movement + back.
///
/// The `make_screen` closure reconstructs the correct Screen variant
/// with an updated cursor, preserving the list type identity.
fn update_simple_list(
    cursor: usize,
    len: usize,
    action: &Action,
    make_screen: impl Fn(usize) -> Screen,
) -> Transition {
    match action {
        Action::MoveUp => {
            Transition::Screen(make_screen(cursor.saturating_sub(1)))
        }
        Action::MoveDown => {
            let new_cursor = if len == 0 { 0 } else { (cursor + 1).min(len - 1) };
            Transition::Screen(make_screen(new_cursor))
        }
        Action::Back => Transition::Screen(Screen::Overview),
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(make_screen(cursor)),
    }
}

/// Confirm: yes triggers quarantine effect, no goes back to list.
fn update_confirm(group_indices: Vec<usize>, action: &Action) -> Transition {
    match action {
        Action::ConfirmYes => Transition::Effect(Effect::StartQuarantine { group_indices }),
        Action::ConfirmNo | Action::Back => Transition::Screen(Screen::duplicate_list()),
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(Screen::Confirm { group_indices }),
    }
}

/// Done: Enter returns to overview, quit exits.
fn update_done(screen: Screen, action: &Action) -> Transition {
    match action {
        Action::Enter => Transition::Screen(Screen::Overview),
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(screen),
    }
}

/// No-op handler: only Quit is accepted.
fn noop(screen: Screen, action: &Action) -> Transition {
    match action {
        Action::Quit => Transition::Quit,
        _ => Transition::Screen(screen),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DuplicateGroup, ContentHash, ScanReport};
    use std::path::PathBuf;

    fn empty_report() -> ScanReport {
        ScanReport::default()
    }

    fn report_with_duplicates(n: usize) -> ScanReport {
        let mut report = ScanReport::default();
        for i in 0..n {
            report.confirmed_duplicates.push(DuplicateGroup {
                original: PathBuf::from(format!("original_{}.txt", i)),
                hash: ContentHash([0u8; 32]),
                duplicates: vec![PathBuf::from(format!("copy_{}.txt", i))],
            });
        }
        report
    }

    fn report_with_all_categories() -> ScanReport {
        let mut report = report_with_duplicates(3);
        report.orphaned_conflicts = vec![PathBuf::from("orphan.txt")];
        report.content_diverged = vec![(
            PathBuf::from("conflict.txt"),
            PathBuf::from("original.txt"),
        )];
        report.skipped = vec![(PathBuf::from("bad.txt"), "permission denied".into())];
        report
    }

    // -- Scanning --

    #[test]
    fn scanning_quit() {
        let screen = Screen::Scanning { candidates_found: 42 };
        assert_eq!(update(screen, &Action::Quit, &empty_report()), Transition::Quit);
    }

    #[test]
    fn scanning_ignores_other_actions() {
        let screen = Screen::Scanning { candidates_found: 42 };
        let result = update(screen, &Action::MoveUp, &empty_report());
        assert_eq!(result, Transition::Screen(Screen::Scanning { candidates_found: 42 }));
    }

    // -- Overview --

    #[test]
    fn overview_quit() {
        assert_eq!(
            update(Screen::Overview, &Action::Quit, &empty_report()),
            Transition::Quit
        );
    }

    #[test]
    fn overview_number1_opens_duplicate_list() {
        let report = report_with_duplicates(3);
        let result = update(Screen::Overview, &Action::NumberKey(1), &report);
        assert_eq!(result, Transition::Screen(Screen::duplicate_list()));
    }

    #[test]
    fn overview_number1_noop_when_no_duplicates() {
        let result = update(Screen::Overview, &Action::NumberKey(1), &empty_report());
        assert_eq!(result, Transition::Screen(Screen::Overview));
    }

    #[test]
    fn overview_number2_opens_orphan_list() {
        let report = report_with_all_categories();
        let result = update(Screen::Overview, &Action::NumberKey(2), &report);
        assert_eq!(result, Transition::Screen(Screen::OrphanList { cursor: 0 }));
    }

    #[test]
    fn overview_number3_opens_diverged_list() {
        let report = report_with_all_categories();
        let result = update(Screen::Overview, &Action::NumberKey(3), &report);
        assert_eq!(result, Transition::Screen(Screen::DivergedList { cursor: 0 }));
    }

    #[test]
    fn overview_number4_opens_skipped_list() {
        let report = report_with_all_categories();
        let result = update(Screen::Overview, &Action::NumberKey(4), &report);
        assert_eq!(result, Transition::Screen(Screen::SkippedList { cursor: 0 }));
    }

    // -- DuplicateList --

    #[test]
    fn duplicate_list_cursor_down() {
        let report = report_with_duplicates(5);
        let screen = Screen::duplicate_list();
        let result = update(screen, &Action::MoveDown, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { cursor, .. }) => assert_eq!(cursor, 1),
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_cursor_up_at_top_stays() {
        let report = report_with_duplicates(5);
        let screen = Screen::duplicate_list();
        let result = update(screen, &Action::MoveUp, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { cursor, .. }) => assert_eq!(cursor, 0),
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_cursor_down_clamps_at_end() {
        let report = report_with_duplicates(3);
        let screen = Screen::DuplicateList {
            cursor: 2,
            selected: Default::default(),
        };
        let result = update(screen, &Action::MoveDown, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { cursor, .. }) => assert_eq!(cursor, 2),
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_enter_drills_into_detail() {
        let report = report_with_duplicates(3);
        let screen = Screen::DuplicateList {
            cursor: 1,
            selected: Default::default(),
        };
        let result = update(screen, &Action::Enter, &report);
        assert_eq!(result, Transition::Screen(Screen::DuplicateDetail { group_index: 1 }));
    }

    #[test]
    fn duplicate_list_back_returns_to_overview() {
        let report = report_with_duplicates(3);
        let screen = Screen::duplicate_list();
        let result = update(screen, &Action::Back, &report);
        assert_eq!(result, Transition::Screen(Screen::Overview));
    }

    #[test]
    fn duplicate_list_toggle_selection() {
        let report = report_with_duplicates(3);
        let screen = Screen::DuplicateList {
            cursor: 1,
            selected: Default::default(),
        };
        // Toggle on
        let result = update(screen, &Action::ToggleSelection, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { cursor, selected }) => {
                assert_eq!(cursor, 1);
                assert!(selected.contains(&1));
                assert_eq!(selected.len(), 1);

                // Toggle off
                let result2 = update(
                    Screen::DuplicateList { cursor, selected },
                    &Action::ToggleSelection,
                    &report,
                );
                match result2 {
                    Transition::Screen(Screen::DuplicateList { selected, .. }) => {
                        assert!(selected.is_empty());
                    }
                    other => panic!("Expected DuplicateList, got {:?}", other),
                }
            }
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_select_all() {
        let report = report_with_duplicates(3);
        let screen = Screen::duplicate_list();
        let result = update(screen, &Action::SelectAll, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { selected, .. }) => {
                assert_eq!(selected.len(), 3);
                assert!(selected.contains(&0));
                assert!(selected.contains(&1));
                assert!(selected.contains(&2));
            }
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_select_none() {
        let report = report_with_duplicates(3);
        let mut initial_selected = std::collections::BTreeSet::new();
        initial_selected.insert(0);
        initial_selected.insert(2);
        let screen = Screen::DuplicateList {
            cursor: 0,
            selected: initial_selected,
        };
        let result = update(screen, &Action::SelectNone, &report);
        match result {
            Transition::Screen(Screen::DuplicateList { selected, .. }) => {
                assert!(selected.is_empty());
            }
            other => panic!("Expected DuplicateList, got {:?}", other),
        }
    }

    #[test]
    fn duplicate_list_quarantine_with_selection_goes_to_confirm() {
        let report = report_with_duplicates(3);
        let mut selected = std::collections::BTreeSet::new();
        selected.insert(0);
        selected.insert(2);
        let screen = Screen::DuplicateList { cursor: 0, selected };
        let result = update(screen, &Action::Quarantine, &report);
        assert_eq!(result, Transition::Screen(Screen::confirm(vec![0, 2])));
    }

    #[test]
    fn duplicate_list_quarantine_without_selection_is_noop() {
        let report = report_with_duplicates(3);
        let screen = Screen::duplicate_list();
        let result = update(screen, &Action::Quarantine, &report);
        assert_eq!(result, Transition::Screen(Screen::duplicate_list()));
    }

    // -- Simple lists (orphan, diverged, skipped) --

    #[test]
    fn orphan_list_cursor_down_preserves_variant() {
        let report = report_with_all_categories();
        let result = update(Screen::OrphanList { cursor: 0 }, &Action::MoveDown, &report);
        // Must stay OrphanList, not morph into another list type
        assert_eq!(result, Transition::Screen(Screen::OrphanList { cursor: 0 }));
        // Only 1 orphan, so cursor stays at 0
    }

    #[test]
    fn diverged_list_back_returns_to_overview() {
        let report = report_with_all_categories();
        let result = update(Screen::DivergedList { cursor: 0 }, &Action::Back, &report);
        assert_eq!(result, Transition::Screen(Screen::Overview));
    }

    #[test]
    fn skipped_list_preserves_variant_on_noop() {
        let report = report_with_all_categories();
        let result = update(Screen::SkippedList { cursor: 0 }, &Action::Enter, &report);
        assert_eq!(result, Transition::Screen(Screen::SkippedList { cursor: 0 }));
    }

    // -- DuplicateDetail --

    #[test]
    fn detail_back_returns_to_list() {
        let report = report_with_duplicates(3);
        let result = update(
            Screen::DuplicateDetail { group_index: 1 },
            &Action::Back,
            &report,
        );
        assert_eq!(result, Transition::Screen(Screen::duplicate_list()));
    }

    #[test]
    fn detail_skip_returns_to_list() {
        let report = report_with_duplicates(3);
        let result = update(
            Screen::DuplicateDetail { group_index: 1 },
            &Action::Skip,
            &report,
        );
        assert_eq!(result, Transition::Screen(Screen::duplicate_list()));
    }

    #[test]
    fn detail_quarantine_goes_to_confirm() {
        let report = report_with_duplicates(3);
        let result = update(
            Screen::DuplicateDetail { group_index: 1 },
            &Action::Quarantine,
            &report,
        );
        assert_eq!(result, Transition::Screen(Screen::confirm(vec![1])));
    }

    #[test]
    fn detail_open_folder_emits_effect() {
        let report = report_with_duplicates(3);
        let result = update(
            Screen::DuplicateDetail { group_index: 0 },
            &Action::OpenFolder,
            &report,
        );
        // The original path is "original_0.txt" — parent is "" (current dir)
        // In real usage paths are absolute; this tests the plumbing.
        assert!(matches!(result, Transition::Effect(Effect::OpenFolder { .. })));
    }

    // -- Confirm --

    #[test]
    fn confirm_yes_triggers_quarantine_effect() {
        let screen = Screen::confirm(vec![0, 2]);
        let result = update(screen, &Action::ConfirmYes, &empty_report());
        assert_eq!(
            result,
            Transition::Effect(Effect::StartQuarantine {
                group_indices: vec![0, 2]
            })
        );
    }

    #[test]
    fn confirm_no_returns_to_list() {
        let screen = Screen::confirm(vec![0, 2]);
        let result = update(screen, &Action::ConfirmNo, &empty_report());
        assert_eq!(result, Transition::Screen(Screen::duplicate_list()));
    }

    // -- Done --

    #[test]
    fn done_enter_returns_to_overview() {
        let screen = Screen::Done {
            quarantined: 5,
            failed: 1,
            bytes_recovered: 1024,
            errors: vec![],
        };
        let result = update(screen, &Action::Enter, &empty_report());
        assert_eq!(result, Transition::Screen(Screen::Overview));
    }

    #[test]
    fn done_quit() {
        let screen = Screen::Done {
            quarantined: 5,
            failed: 0,
            bytes_recovered: 0,
            errors: vec![],
        };
        assert_eq!(update(screen, &Action::Quit, &empty_report()), Transition::Quit);
    }
}
