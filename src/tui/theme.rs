//! TUI color semantics and style constants.
//!
//! Centralized theme definitions encoding the design doc's color system.
//! Pure data — consumed by the rendering layer for visual consistency.
//!
//! Color semantics:
//! - Green: safe, keep, success (checkmarks, "KEEP" labels)
//! - Yellow: warning, attention (orphaned files)
//! - Red: will be removed (files to quarantine)
//! - Cyan: interactive elements (keybinding hints)
//! - Dim: de-emphasized (hashes, timestamps)
//! - Bold: important (counts, filenames)

use ratatui::style::{Color, Modifier, Style};

// ============================================================================
// SEMANTIC STYLES
// ============================================================================

/// Safe / keep / success — green.
pub const STYLE_SAFE: Style = Style::new().fg(Color::Green);

/// Warning / attention needed — yellow.
pub const STYLE_WARNING: Style = Style::new().fg(Color::Yellow);

/// Will be removed / danger — red.
pub const STYLE_DANGER: Style = Style::new().fg(Color::Red);

/// Interactive element / keybinding hint — cyan.
pub const STYLE_INTERACTIVE: Style = Style::new().fg(Color::Cyan);

/// De-emphasized metadata — dark gray.
pub const STYLE_DIM: Style = Style::new().fg(Color::DarkGray);

/// Important text — bold white.
pub const STYLE_IMPORTANT: Style = Style::new().add_modifier(Modifier::BOLD);

// ============================================================================
// UI ELEMENT STYLES
// ============================================================================

/// Title bar / header.
pub const STYLE_TITLE: Style = Style::new().fg(Color::White).add_modifier(Modifier::BOLD);

/// Selected / highlighted list item.
pub const STYLE_SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);

/// Cursor row in a list (not selected, just focused).
pub const STYLE_CURSOR: Style = Style::new().add_modifier(Modifier::REVERSED);

/// Checkbox: checked.
pub const STYLE_CHECKED: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);

/// Checkbox: unchecked.
pub const STYLE_UNCHECKED: Style = Style::new().fg(Color::DarkGray);

/// Progress bar fill.
pub const STYLE_PROGRESS: Style = Style::new().fg(Color::Cyan);

/// Footer / help line.
pub const STYLE_HELP: Style = Style::new().fg(Color::DarkGray);

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_styles_have_expected_colors() {
        assert_eq!(STYLE_SAFE.fg, Some(Color::Green));
        assert_eq!(STYLE_WARNING.fg, Some(Color::Yellow));
        assert_eq!(STYLE_DANGER.fg, Some(Color::Red));
        assert_eq!(STYLE_INTERACTIVE.fg, Some(Color::Cyan));
        assert_eq!(STYLE_DIM.fg, Some(Color::DarkGray));
    }

    #[test]
    fn important_style_is_bold() {
        assert!(STYLE_IMPORTANT.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn cursor_style_is_reversed() {
        assert!(STYLE_CURSOR.add_modifier.contains(Modifier::REVERSED));
    }
}
