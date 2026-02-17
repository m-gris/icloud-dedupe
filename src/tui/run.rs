//! TUI effects boundary: event loop, terminal lifecycle, key mapping.
//!
//! This is the only module with side effects. It wires the pure layers
//! (state, update, view) to the real terminal via crossterm and ratatui.
//! Kept minimal — all intelligence lives in the pure layers.
//!
//! Architecture: two producer threads feed a single mpsc channel.
//! - Key reader thread: forwards crossterm key events
//! - Scanner thread: sends progress updates and the final report
//! The event loop consumes from the channel, dispatching to pure handlers.

use std::io;
use std::sync::mpsc;
use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::scanner::{assemble_report, find_candidates_with_progress, verify_candidate};
use crate::types::ScanConfig;

use super::state::{Action, App, AppEvent, Screen, Transition};
use super::update::{handle_background_event, update};
use super::view::render;

// ============================================================================
// KEY MAPPING
// ============================================================================

/// Map a crossterm key event to a semantic Action.
///
/// Returns None for keys that don't map to any action.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    // Ctrl+C always quits
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Action::Quit);
    }

    match key.code {
        // Navigation
        KeyCode::Up | KeyCode::Char('k') => Some(Action::MoveUp),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::MoveDown),
        KeyCode::Enter => Some(Action::Enter),
        KeyCode::Esc => Some(Action::Back),

        // Selection
        KeyCode::Char(' ') => Some(Action::ToggleSelection),
        KeyCode::Char('a') => Some(Action::SelectAll),
        KeyCode::Char('n') => Some(Action::SelectNone),

        // Actions
        KeyCode::Char('Q') => Some(Action::Quarantine),
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('s') => Some(Action::Skip),
        KeyCode::Char('o') => Some(Action::OpenFolder),

        // Confirm
        KeyCode::Char('Y') | KeyCode::Char('y') => Some(Action::ConfirmYes),
        KeyCode::Char('N') => Some(Action::ConfirmNo),

        // Number keys for overview navigation
        KeyCode::Char(c @ '1'..='4') => Some(Action::NumberKey(c as u8 - b'0')),

        _ => None,
    }
}

// ============================================================================
// TERMINAL LIFECYCLE
// ============================================================================

/// Set up the terminal for TUI mode.
fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to normal mode.
fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Install a panic hook that restores the terminal before printing the panic.
fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Best-effort terminal restoration
        let _ = restore_terminal();
        original_hook(panic_info);
    }));
}

// ============================================================================
// BACKGROUND THREADS
// ============================================================================

/// Spawn a thread that reads crossterm events and forwards key events to the channel.
fn spawn_key_reader(tx: mpsc::Sender<AppEvent>) {
    thread::spawn(move || {
        loop {
            match event::read() {
                Ok(Event::Key(key)) => {
                    if tx.send(AppEvent::Key(key)).is_err() {
                        break; // receiver dropped, TUI is shutting down
                    }
                }
                Ok(_) => {} // ignore mouse, resize, etc.
                Err(_) => break,
            }
        }
    });
}

/// Spawn a thread that runs the scanner and sends progress/completion events.
fn spawn_scanner(config: ScanConfig, tx: mpsc::Sender<AppEvent>) {
    thread::spawn(move || {
        // Phase 1: Discovery
        let tx_progress = tx.clone();
        let candidates = match find_candidates_with_progress(&config, move |scanned, found| {
            // Best-effort: if the receiver is gone, we'll notice on next send
            let _ = tx_progress.send(AppEvent::ScanProgress {
                files_scanned: scanned,
                candidates_found: found,
            });
        }) {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(AppEvent::ScanError(e.to_string()));
                return;
            }
        };

        // Phase 2: Verification (parallel with rayon)
        use rayon::prelude::*;

        let results: Vec<_> = candidates
            .par_iter()
            .map(|c| (c.path.clone(), verify_candidate(c)))
            .collect();

        // Phase 3: Assemble report
        let report = assemble_report(results);

        let _ = tx.send(AppEvent::ScanComplete(report));
    });
}

// ============================================================================
// EVENT LOOP
// ============================================================================

/// Run the TUI event loop, scanning in the background.
///
/// This is the main entry point for the TUI. It sets up the terminal,
/// spawns a scanner thread, and runs the event loop until the user quits.
pub fn run(config: ScanConfig) -> io::Result<()> {
    install_panic_hook();
    let mut terminal = setup_terminal()?;
    let mut app = App::scanning();

    let (tx, rx) = mpsc::channel::<AppEvent>();

    // Spawn producer threads
    spawn_key_reader(tx.clone());
    spawn_scanner(config, tx);

    loop {
        // Render
        terminal.draw(|frame| render(&app, frame))?;

        // Check quit flag
        if app.should_quit {
            break;
        }

        // Block on next event from any producer
        let event = match rx.recv() {
            Ok(e) => e,
            Err(_) => break, // all senders dropped
        };

        match event {
            AppEvent::Key(key) => {
                if let Some(action) = map_key(key) {
                    // During scanning, only Quit is meaningful (no report yet)
                    if app.report.is_none() {
                        if action == Action::Quit {
                            app.should_quit = true;
                        }
                        // All other actions ignored during scanning
                    } else {
                        let screen = std::mem::take(&mut app.screen);
                        let report_ref = app.report.as_ref().expect("checked above");
                        let transition = update(screen, &action, report_ref);

                        match transition {
                            Transition::Screen(new_screen) => {
                                app.screen = new_screen;
                            }
                            Transition::Quit => {
                                app.should_quit = true;
                            }
                            Transition::Effect(effect) => {
                                handle_effect(effect, &mut app);
                            }
                        }
                    }
                }
            }
            background_event => {
                handle_background_event(&mut app, background_event);
            }
        }
    }

    restore_terminal()?;
    Ok(())
}

// ============================================================================
// EFFECT HANDLING
// ============================================================================

use super::state::Effect;

/// Handle a side effect requested by a pure transition.
fn handle_effect(effect: Effect, app: &mut App) {
    match effect {
        Effect::StartQuarantine { group_indices } => {
            // For now, transition to the progress screen.
            // Actual quarantine execution will be wired in k0r.
            let total: usize = group_indices
                .iter()
                .filter_map(|&i| app.report.as_ref()?.confirmed_duplicates.get(i))
                .map(|g| g.duplicates.len())
                .sum();
            app.screen = Screen::progress(total);
        }
        Effect::OpenFolder { path } => {
            // macOS: open the folder in Finder
            let _ = std::process::Command::new("open")
                .arg(&path)
                .spawn();
            // Stay on current screen (already set before effect dispatch)
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
    fn ctrl_c_maps_to_quit() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(map_key(key), Some(Action::Quit));
    }

    #[test]
    fn vim_keys_map_to_movement() {
        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        let k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(map_key(j), Some(Action::MoveDown));
        assert_eq!(map_key(k), Some(Action::MoveUp));
    }

    #[test]
    fn arrow_keys_map_to_movement() {
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(map_key(up), Some(Action::MoveUp));
        assert_eq!(map_key(down), Some(Action::MoveDown));
    }

    #[test]
    fn space_toggles_selection() {
        let space = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(map_key(space), Some(Action::ToggleSelection));
    }

    #[test]
    fn capital_q_maps_to_quarantine() {
        let q = KeyEvent::new(KeyCode::Char('Q'), KeyModifiers::SHIFT);
        assert_eq!(map_key(q), Some(Action::Quarantine));
    }

    #[test]
    fn number_keys_map_to_number_actions() {
        for n in 1..=4u8 {
            let key = KeyEvent::new(KeyCode::Char((b'0' + n) as char), KeyModifiers::NONE);
            assert_eq!(map_key(key), Some(Action::NumberKey(n)));
        }
    }

    #[test]
    fn unmapped_key_returns_none() {
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(map_key(key), None);
    }

    #[test]
    fn enter_maps_to_enter_action() {
        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(map_key(key), Some(Action::Enter));
    }

    #[test]
    fn esc_maps_to_back() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(map_key(key), Some(Action::Back));
    }
}
