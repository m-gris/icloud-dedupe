//! Pure rendering: map App state to ratatui widget trees.
//!
//! Each screen has a dedicated render function. The main `render()`
//! dispatches based on the current Screen variant. Widget-building
//! functions are pure (state in, widgets out); the only effect is
//! Frame::render_widget() which writes to the terminal buffer.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use humansize::{format_size, BINARY};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::types::{DuplicateGroup, ScanReport};

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
        Screen::DuplicateList { cursor, selected } => {
            if let Some(report) = &app.report {
                render_duplicate_list(report, *cursor, selected, frame, content_area);
            }
        }
        Screen::DuplicateDetail { group_index } => {
            if let Some(report) = &app.report {
                render_duplicate_detail(report, *group_index, frame, content_area);
            }
        }
        Screen::OrphanList { cursor } => {
            if let Some(report) = &app.report {
                render_simple_list(
                    &report.orphaned_conflicts.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                    *cursor,
                    frame,
                    content_area,
                );
            }
        }
        Screen::DivergedList { cursor } => {
            if let Some(report) = &app.report {
                let items: Vec<String> = report
                    .content_diverged
                    .iter()
                    .map(|(c, o)| format!("{} ≠ {}", c.display(), o.display()))
                    .collect();
                render_simple_list(&items, *cursor, frame, content_area);
            }
        }
        Screen::SkippedList { cursor } => {
            if let Some(report) = &app.report {
                let items: Vec<String> = report
                    .skipped
                    .iter()
                    .map(|(p, e)| format!("{}: {}", p.display(), e))
                    .collect();
                render_simple_list(&items, *cursor, frame, content_area);
            }
        }
        Screen::Confirm { group_indices } => {
            if let Some(report) = &app.report {
                render_confirm(report, group_indices, frame, content_area);
            }
        }
        Screen::Progress { done, total, current, errors } => {
            render_progress(*done, *total, current.as_deref(), errors, frame, content_area);
        }
        Screen::Done { quarantined, failed, bytes_recovered, errors } => {
            render_done(*quarantined, *failed, *bytes_recovered, errors, frame, content_area);
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
// SCREEN: DUPLICATE LIST
// ============================================================================

fn render_duplicate_list(
    report: &ScanReport,
    cursor: usize,
    selected: &BTreeSet<usize>,
    frame: &mut Frame,
    area: Rect,
) {
    let groups = &report.confirmed_duplicates;

    // Split: list area + status bar
    let chunks = Layout::vertical([
        Constraint::Min(0),   // list
        Constraint::Length(1), // selection tally
    ])
    .split(area);

    // Build list items
    let mut lines: Vec<Line> = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        let is_selected = selected.contains(&i);
        let is_cursor = i == cursor;

        let checkbox = if is_selected {
            Span::styled("[x] ", theme::STYLE_CHECKED)
        } else {
            Span::styled("[ ] ", theme::STYLE_UNCHECKED)
        };

        let name = group_display_name(group);
        let copies = group.duplicates.len();
        let size: u64 = group
            .duplicates
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .sum();

        let info = format!(
            "  {} cop{}, {}",
            copies,
            if copies == 1 { "y" } else { "ies" },
            format_size(size, BINARY)
        );

        let spans = vec![
            Span::raw("  "),
            checkbox,
            Span::styled(name, theme::STYLE_IMPORTANT),
            Span::styled(info, theme::STYLE_DIM),
        ];

        let line = if is_cursor {
            Line::from(spans).style(theme::STYLE_CURSOR)
        } else {
            Line::from(spans)
        };
        lines.push(line);
    }

    // Scroll: if cursor is beyond visible area, offset the view
    let visible_height = chunks[0].height as usize;
    let scroll_offset = if cursor >= visible_height {
        cursor - visible_height + 1
    } else {
        0
    };

    let list = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
    frame.render_widget(list, chunks[0]);

    // Selection tally
    let selected_count = selected.len();
    let selected_size: u64 = selected
        .iter()
        .filter_map(|&i| groups.get(i))
        .flat_map(|g| &g.duplicates)
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    let tally = if selected_count > 0 {
        format!(
            "  Selected: {} group{} ({})",
            selected_count,
            if selected_count == 1 { "" } else { "s" },
            format_size(selected_size, BINARY)
        )
    } else {
        "  Nothing selected".to_string()
    };

    let tally_widget = Paragraph::new(Span::styled(tally, theme::STYLE_DIM));
    frame.render_widget(tally_widget, chunks[1]);
}

/// Extract a display name from a duplicate group (filename of original).
fn group_display_name(group: &DuplicateGroup) -> String {
    group
        .original
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| group.original.display().to_string())
}

// ============================================================================
// SCREEN: DUPLICATE DETAIL
// ============================================================================

fn render_duplicate_detail(
    report: &ScanReport,
    group_index: usize,
    frame: &mut Frame,
    area: Rect,
) {
    let Some(group) = report.confirmed_duplicates.get(group_index) else {
        let err = Paragraph::new("Group not found").style(theme::STYLE_DANGER);
        frame.render_widget(err, area);
        return;
    };

    let mut lines = vec![Line::from("")];

    // KEEP section
    lines.push(Line::from(Span::styled(
        "  KEEP (original):",
        theme::STYLE_SAFE,
    )));

    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("  {}", group.original.display()),
            theme::STYLE_IMPORTANT,
        ),
    ]));

    if let Ok(meta) = std::fs::metadata(&group.original) {
        lines.push(Line::from(Span::styled(
            format!("    Size: {}", format_size(meta.len(), BINARY)),
            theme::STYLE_DIM,
        )));
        if let Ok(modified) = meta.modified() {
            lines.push(Line::from(Span::styled(
                format!("    Modified: {}", format_system_time(modified)),
                theme::STYLE_DIM,
            )));
        }
    }

    lines.push(Line::from(""));

    // REMOVE section
    lines.push(Line::from(Span::styled(
        "  REMOVE (duplicates):",
        theme::STYLE_DANGER,
    )));

    for dup in &group.duplicates {
        let dup_name = dup
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dup.display().to_string());
        lines.push(Line::from(vec![
            Span::raw("    "),
            Span::styled("• ", theme::STYLE_DANGER),
            Span::raw(dup_name),
        ]));
    }

    lines.push(Line::from(""));

    // Hash
    lines.push(Line::from(Span::styled(
        format!("  Hash: {} (BLAKE3)", truncate_hash(&group.hash.to_hex())),
        theme::STYLE_DIM,
    )));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

/// Extract filename for display, falling back to full path.
fn file_display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Truncate a hash string for display: "abcd1234...ef567890"
fn truncate_hash(hex: &str) -> String {
    if hex.len() > 16 {
        format!("{}...{}", &hex[..8], &hex[hex.len() - 8..])
    } else {
        hex.to_string()
    }
}

/// Format a SystemTime as a human-readable string (without chrono dep).
fn format_system_time(time: std::time::SystemTime) -> String {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            // Simple date format: days since epoch
            let days = secs / 86400;
            let years = 1970 + days / 365;
            format!("~{}", years)
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

// ============================================================================
// SCREEN: SIMPLE LIST (orphans, diverged, skipped)
// ============================================================================

fn render_simple_list(items: &[String], cursor: usize, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let is_cursor = i == cursor;
        let line = if is_cursor {
            Line::from(format!("  > {}", item)).style(theme::STYLE_CURSOR)
        } else {
            Line::from(format!("    {}", item))
        };
        lines.push(line);
    }

    if items.is_empty() {
        lines.push(Line::from(Span::styled("  (empty)", theme::STYLE_DIM)));
    }

    let visible_height = area.height as usize;
    let scroll_offset = if cursor >= visible_height {
        cursor - visible_height + 1
    } else {
        0
    };

    let paragraph = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
    frame.render_widget(paragraph, area);
}

// ============================================================================
// SCREEN: CONFIRM
// ============================================================================

fn render_confirm(
    report: &ScanReport,
    group_indices: &[usize],
    frame: &mut Frame,
    area: Rect,
) {
    let files: Vec<&std::path::Path> = group_indices
        .iter()
        .filter_map(|&i| report.confirmed_duplicates.get(i))
        .flat_map(|g| g.duplicates.iter())
        .map(|p| p.as_path())
        .collect();

    let total_size: u64 = files
        .iter()
        .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
        .sum();

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled("  You're about to quarantine:", theme::STYLE_WARNING)),
        Line::from(""),
        Line::from(format!(
            "    {} files ({}) from {} duplicate group{}",
            files.len(),
            format_size(total_size, BINARY),
            group_indices.len(),
            if group_indices.len() == 1 { "" } else { "s" }
        )),
        Line::from(""),
    ];

    // File list
    let max_show = 10;
    for (i, path) in files.iter().enumerate() {
        if i >= max_show {
            lines.push(Line::from(Span::styled(
                format!("    ...{} more", files.len() - max_show),
                theme::STYLE_DIM,
            )));
            break;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        lines.push(Line::from(format!("    {}", name)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  This is REVERSIBLE. Run `restore --all` to undo.",
        theme::STYLE_SAFE,
    )));

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

// ============================================================================
// SCREEN: PROGRESS
// ============================================================================

fn render_progress(
    done: usize,
    total: usize,
    current: Option<&Path>,
    errors: &[(PathBuf, String)],
    frame: &mut Frame,
    area: Rect,
) {
    let pct = if total > 0 {
        (done * 100) / total
    } else {
        0
    };

    // Build a text-based progress bar
    let bar_width = 40;
    let filled = if total > 0 {
        (done * bar_width) / total
    } else {
        0
    };
    let empty = bar_width - filled;
    let bar = format!(
        "[{}{}] {}%",
        "█".repeat(filled),
        "░".repeat(empty),
        pct
    );

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {}", bar), theme::STYLE_PROGRESS)),
        Line::from(""),
    ];

    if let Some(path) = current {
        lines.push(Line::from(format!("  Current: {}", file_display_name(path))));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!("  ✓  {} moved", done), theme::STYLE_SAFE),
    ]));

    let remaining = total.saturating_sub(done);
    lines.push(Line::from(Span::styled(
        format!("  ◦  {} remaining", remaining),
        theme::STYLE_DIM,
    )));

    for (path, err) in errors {
        let name = file_display_name(path);
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ⚠  ", theme::STYLE_WARNING),
            Span::styled(format!("Failed: {} ({})", name, err), theme::STYLE_WARNING),
        ]));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

// ============================================================================
// SCREEN: DONE
// ============================================================================

fn render_done(
    quarantined: usize,
    failed: usize,
    bytes_recovered: u64,
    errors: &[(PathBuf, String)],
    frame: &mut Frame,
    area: Rect,
) {
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!(
                    "  ✓ {} file{} quarantined ({})",
                    quarantined,
                    if quarantined == 1 { "" } else { "s" },
                    format_size(bytes_recovered, BINARY)
                ),
                theme::STYLE_SAFE,
            ),
        ]),
    ];

    if failed > 0 {
        lines.push(Line::from(vec![
            Span::styled(
                format!("  ⚠  {} file{} skipped", failed, if failed == 1 { "" } else { "s" }),
                theme::STYLE_WARNING,
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(format!(
        "  Space recovered: {}",
        format_size(bytes_recovered, BINARY)
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  ─────────────────────────────────────────────────────",
        theme::STYLE_DIM,
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  To undo:   icloud-dedupe restore --all",
        theme::STYLE_DIM,
    )));
    lines.push(Line::from(Span::styled(
        "  To purge:  icloud-dedupe purge",
        theme::STYLE_DIM,
    )));

    for (path, err) in errors {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Error: {} — {}", path.display(), err),
            theme::STYLE_WARNING,
        )));
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
    fn duplicate_list_renders_without_panic() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::DuplicateList {
            cursor: 0,
            selected: Default::default(),
        };
        terminal
            .draw(|frame| render(&app, frame))
            .expect("render should not panic");
    }

    #[test]
    fn duplicate_list_shows_checkbox_and_filename() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        let mut selected = BTreeSet::new();
        selected.insert(0);
        app.screen = Screen::DuplicateList { cursor: 0, selected };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("[x]"), "Should show checked checkbox");
        assert!(content.contains("report.pdf"), "Should show filename");
    }

    #[test]
    fn duplicate_detail_renders_without_panic() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::DuplicateDetail { group_index: 0 };
        terminal
            .draw(|frame| render(&app, frame))
            .expect("render should not panic");
    }

    #[test]
    fn duplicate_detail_shows_keep_and_remove_sections() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::DuplicateDetail { group_index: 0 };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("KEEP"), "Should show KEEP section");
        assert!(content.contains("REMOVE"), "Should show REMOVE section");
    }

    #[test]
    fn truncate_hash_works() {
        let full = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let short = truncate_hash(full);
        assert_eq!(short, "abcdef01...23456789");

        let tiny = "abcdef01";
        assert_eq!(truncate_hash(tiny), "abcdef01");
    }

    #[test]
    fn orphan_list_renders() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::OrphanList { cursor: 0 };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("orphan.txt"));
    }

    #[test]
    fn confirm_screen_renders() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::Confirm {
            group_indices: vec![0],
        };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("quarantine"), "Should mention quarantine");
        assert!(content.contains("REVERSIBLE"), "Should mention reversibility");
    }

    #[test]
    fn progress_screen_renders() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::Progress {
            done: 3,
            total: 10,
            current: Some(PathBuf::from("test.pdf")),
            errors: vec![],
        };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("30%"), "Should show percentage");
        assert!(content.contains("test.pdf"), "Should show current file");
    }

    #[test]
    fn done_screen_renders() {
        let mut terminal = make_terminal();
        let mut app = App::with_report(report_with_data());
        app.screen = Screen::Done {
            quarantined: 5,
            failed: 1,
            bytes_recovered: 1_048_576,
            errors: vec![],
        };
        terminal.draw(|frame| render(&app, frame)).unwrap();

        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer
            .content()
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect();
        assert!(content.contains("5 files quarantined"), "Should show count");
        assert!(content.contains("restore"), "Should mention restore");
    }

    #[test]
    fn all_screens_render_without_panic() {
        let mut terminal = make_terminal();
        let report = report_with_data();
        let screens = vec![
            Screen::Scanning { candidates_found: 10 },
            Screen::Overview,
            Screen::DuplicateList { cursor: 0, selected: Default::default() },
            Screen::DuplicateDetail { group_index: 0 },
            Screen::OrphanList { cursor: 0 },
            Screen::DivergedList { cursor: 0 },
            Screen::SkippedList { cursor: 0 },
            Screen::Confirm { group_indices: vec![0] },
            Screen::Progress { done: 5, total: 10, current: None, errors: vec![] },
            Screen::Done { quarantined: 5, failed: 0, bytes_recovered: 0, errors: vec![] },
        ];
        for screen in screens {
            let app = App {
                screen,
                report: Some(report.clone()),
                should_quit: false,
            };
            terminal
                .draw(|frame| render(&app, frame))
                .expect("every screen should render without panic");
        }
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
