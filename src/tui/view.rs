//! Pure rendering: map App state to ratatui widget trees.
//!
//! Each screen has a dedicated render function. The main `render()`
//! dispatches based on the current Screen variant. Widget-building
//! functions are pure (state in, widgets out); the only effect is
//! Frame::render_widget() which writes to the terminal buffer.

use humansize::{format_size, BINARY};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::types::ScanReport;

use super::state::{App, Screen};
use super::theme;

// ============================================================================
// DISPATCH
// ============================================================================

/// Render the current screen to the terminal frame.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();

    // Common layout: title bar at top, content in middle, help at bottom
    let chunks = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(0),   // content
        Constraint::Length(1), // help
    ])
    .split(area);

    let title = render_title(&app.screen);
    frame.render_widget(title, chunks[0]);

    let help = render_help(&app.screen);
    frame.render_widget(help, chunks[2]);

    let content_area = chunks[1];

    match &app.screen {
        Screen::Scanning { candidates_found } => {
            render_scanning(*candidates_found, frame, content_area);
        }
        Screen::Overview => {
            if let Some(report) = &app.report {
                render_overview(report, frame, content_area);
            }
        }
        // Remaining screens will be added in th0.3.2 and th0.3.3
        _ => {
            let placeholder = Paragraph::new("Screen not yet implemented")
                .style(theme::STYLE_DIM);
            frame.render_widget(placeholder, content_area);
        }
    }
}

// ============================================================================
// SHARED LAYOUT
// ============================================================================

/// Title bar showing the app name and screen-specific context.
fn render_title(screen: &Screen) -> Paragraph<'static> {
    let title_text = match screen {
        Screen::Scanning { .. } => "icloud-dedupe",
        Screen::Overview => "icloud-dedupe",
        Screen::DuplicateList { .. } => "Duplicates",
        Screen::DuplicateDetail { .. } => "Duplicate Detail",
        Screen::OrphanList { .. } => "Orphaned Conflicts",
        Screen::DivergedList { .. } => "Diverged Files",
        Screen::SkippedList { .. } => "Skipped Files",
        Screen::Confirm { .. } => "Confirm Quarantine",
        Screen::Progress { .. } => "Quarantining...",
        Screen::Done { .. } => "Complete",
    };

    Paragraph::new(Line::from(vec![
        Span::styled(title_text, theme::STYLE_TITLE),
    ]))
}

/// Help line showing available keybindings for the current screen.
fn render_help(screen: &Screen) -> Paragraph<'static> {
    let help_text = match screen {
        Screen::Scanning { .. } => "^C quit",
        Screen::Overview => "[1-4] navigate  [q] quit",
        Screen::DuplicateList { .. } => {
            "[j/k] move  [Space] toggle  [a] all  [n] none  [Enter] details  [Q] quarantine  [Esc] back"
        }
        Screen::DuplicateDetail { .. } => "[Q] quarantine  [s] skip  [o] open folder  [Esc] back",
        Screen::OrphanList { .. } | Screen::DivergedList { .. } | Screen::SkippedList { .. } => {
            "[j/k] move  [Esc] back"
        }
        Screen::Confirm { .. } => "[Y] yes, quarantine  [N] no, go back",
        Screen::Progress { .. } => "",
        Screen::Done { .. } => "[Enter] overview  [q] quit",
    };

    Paragraph::new(Span::styled(help_text, theme::STYLE_HELP))
}

// ============================================================================
// SCREEN: SCANNING
// ============================================================================

fn render_scanning(candidates_found: usize, frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Discovering conflict patterns...",
            theme::STYLE_INTERACTIVE,
        )),
        Line::from(""),
        Line::from(format!("    Found: {} candidates", candidates_found)),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

// ============================================================================
// SCREEN: OVERVIEW
// ============================================================================

fn render_overview(report: &ScanReport, frame: &mut Frame, area: Rect) {
    let dup_count = report.confirmed_duplicates.len();
    let dup_files: usize = report
        .confirmed_duplicates
        .iter()
        .map(|g| g.duplicates.len())
        .sum();
    let orphan_count = report.orphaned_conflicts.len();
    let diverged_count = report.content_diverged.len();
    let skipped_count = report.skipped.len();
    let recoverable = format_size(report.bytes_recoverable, BINARY);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled("  Scan Complete", theme::STYLE_TITLE)),
        Line::from(Span::styled(
            "  ═══════════════",
            theme::STYLE_DIM,
        )),
        Line::from(""),
    ];

    // Duplicates line
    if dup_count > 0 {
        lines.push(Line::from(vec![
            Span::styled("  ✓  ", theme::STYLE_SAFE),
            Span::styled(
                format!("{} confirmed duplicates ({} files)", dup_count, dup_files),
                theme::STYLE_SAFE,
            ),
            Span::styled(format!("     {} recoverable", recoverable), theme::STYLE_DIM),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "  ✓  No duplicates found",
            theme::STYLE_DIM,
        )));
    }

    // Orphans line
    if orphan_count > 0 {
        lines.push(Line::from(vec![
            Span::styled("  ⚠  ", theme::STYLE_WARNING),
            Span::styled(
                format!("{} orphaned conflicts", orphan_count),
                theme::STYLE_WARNING,
            ),
            Span::styled("       needs review", theme::STYLE_DIM),
        ]));
    }

    // Diverged line
    if diverged_count > 0 {
        lines.push(Line::from(vec![
            Span::styled("  ≠  ", theme::STYLE_DANGER),
            Span::styled(
                format!("{} diverged files", diverged_count),
                theme::STYLE_DANGER,
            ),
            Span::styled("         different content", theme::STYLE_DIM),
        ]));
    }

    // Skipped line
    if skipped_count > 0 {
        lines.push(Line::from(vec![
            Span::styled("  ─  ", theme::STYLE_DIM),
            Span::styled(format!("{} skipped", skipped_count), theme::STYLE_DIM),
            Span::styled("                read errors", theme::STYLE_DIM),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────────────────────────",
        theme::STYLE_DIM,
    )));
    lines.push(Line::from(""));

    // Navigation hints — only show for non-empty categories
    let mut nav_items = Vec::new();
    if dup_count > 0 {
        nav_items.push(Span::styled("  [1] ", theme::STYLE_INTERACTIVE));
        nav_items.push(Span::raw("Review duplicates    "));
    }
    if orphan_count > 0 {
        nav_items.push(Span::styled("[2] ", theme::STYLE_INTERACTIVE));
        nav_items.push(Span::raw("Review orphans"));
    }
    if !nav_items.is_empty() {
        lines.push(Line::from(nav_items));
    }

    let mut nav_items2 = Vec::new();
    if diverged_count > 0 {
        nav_items2.push(Span::styled("  [3] ", theme::STYLE_INTERACTIVE));
        nav_items2.push(Span::raw("Review diverged      "));
    }
    if skipped_count > 0 {
        nav_items2.push(Span::styled("[4] ", theme::STYLE_INTERACTIVE));
        nav_items2.push(Span::raw("View skipped"));
    }
    if !nav_items2.is_empty() {
        lines.push(Line::from(nav_items2));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ContentHash, DuplicateGroup, ScanReport};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn make_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(60, 20);
        Terminal::new(backend).unwrap()
    }

    fn report_with_data() -> ScanReport {
        let mut report = ScanReport::default();
        report.confirmed_duplicates.push(DuplicateGroup {
            original: PathBuf::from("/docs/report.pdf"),
            hash: ContentHash([0u8; 32]),
            duplicates: vec![
                PathBuf::from("/docs/report Copy.pdf"),
                PathBuf::from("/docs/report Copy 2.pdf"),
            ],
        });
        report.bytes_recoverable = 45_000_000;
        report.orphaned_conflicts = vec![PathBuf::from("orphan.txt")];
        report.content_diverged = vec![(
            PathBuf::from("conflict.txt"),
            PathBuf::from("original.txt"),
        )];
        report.skipped = vec![(PathBuf::from("bad.txt"), "permission denied".into())];
        report
    }

    #[test]
    fn scanning_screen_renders_without_panic() {
        let mut terminal = make_terminal();
        let app = App::scanning();
        terminal
            .draw(|frame| render(&app, frame))
            .expect("render should not panic");
    }

    #[test]
    fn overview_screen_renders_without_panic() {
        let mut terminal = make_terminal();
        let app = App::with_report(report_with_data());
        terminal
            .draw(|frame| render(&app, frame))
            .expect("render should not panic");
    }

    #[test]
    fn overview_with_empty_report_renders() {
        let mut terminal = make_terminal();
        let app = App::with_report(ScanReport::default());
        terminal
            .draw(|frame| render(&app, frame))
            .expect("render should not panic");
    }

    #[test]
    fn scanning_screen_shows_candidate_count() {
        let mut terminal = make_terminal();
        let mut app = App::scanning();
        app.screen = Screen::Scanning {
            candidates_found: 42,
        };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        // Check the buffer contains the count
        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("42"), "Buffer should contain candidate count 42");
    }

    #[test]
    fn title_renders_for_each_screen_variant() {
        // Verify render_title doesn't panic for any variant
        let screens = vec![
            Screen::Overview,
            Screen::Scanning { candidates_found: 0 },
            Screen::DuplicateList { cursor: 0, selected: Default::default() },
            Screen::DuplicateDetail { group_index: 0 },
            Screen::OrphanList { cursor: 0 },
            Screen::DivergedList { cursor: 0 },
            Screen::SkippedList { cursor: 0 },
            Screen::Confirm { group_indices: vec![] },
            Screen::Progress { done: 0, total: 0, current: None, errors: vec![] },
            Screen::Done { quarantined: 0, failed: 0, bytes_recovered: 0, errors: vec![] },
        ];
        for screen in &screens {
            let _ = render_title(screen);
            let _ = render_help(screen);
        }
    }
}
