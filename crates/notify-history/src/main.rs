use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

mod app;
mod config;
mod filter;
mod notification;
mod ui;

use app::{App, Mode};
use config::Config;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "notify-history", about = "Notification history viewer TUI")]
struct Args {
    /// Generate shell completions and print to stdout
    #[arg(long = "generate", value_name = "SHELL", hide = true)]
    generate: Option<Shell>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if let Some(shell) = args.generate {
        let mut cmd = Args::command();
        generate(shell, &mut cmd, "notify-history", &mut io::stdout());
        return Ok(());
    }

    let config = Config::load();
    let mut app = App::new(config);

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_event_loop(&mut terminal, &mut app);

    // Always restore terminal even on error
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if handle_key(key.code, key.modifiers, app) {
                    break;
                }
            }
            Event::Resize(_, _) => {
                // Handled automatically on the next draw call
            }
            _ => {}
        }
    }
    Ok(())
}

// ── Key dispatch ─────────────────────────────────────────────────────────────

/// Returns `true` if the application should quit.
fn handle_key(code: KeyCode, modifiers: KeyModifiers, app: &mut App) -> bool {
    match app.mode {
        Mode::Normal => handle_normal(code, modifiers, app),
        Mode::Filter => {
            handle_filter(code, app);
            false
        }
        Mode::ConfirmClearAll => {
            handle_confirm(code, app, false);
            false
        }
        Mode::ConfirmClearSelected => {
            handle_confirm(code, app, true);
            false
        }
        Mode::Help => {
            handle_help(code, app);
            false
        }
    }
}

// ── Normal mode ───────────────────────────────────────────────────────────────

fn handle_normal(code: KeyCode, _mods: KeyModifiers, app: &mut App) -> bool {
    match code {
        KeyCode::Char('q') => return true,

        KeyCode::Char('r') => app.load_notifications(),

        KeyCode::Char('c') => app.mode = Mode::ConfirmClearAll,

        KeyCode::Char('x') => app.remove_current_notification(),

        KeyCode::Char('g') => app.go_to_start(),

        KeyCode::Char('G') => app.go_to_end(),

        KeyCode::Char('/') => {
            app.mode = Mode::Filter;
        }

        KeyCode::Char('s') => app.toggle_select_current(),

        KeyCode::Char('S') => {
            if !app.multi_selected.is_empty() {
                app.mode = Mode::ConfirmClearSelected;
            }
        }

        KeyCode::Char('?') => app.mode = Mode::Help,

        KeyCode::F(1) => app.toggle_hints(),

        KeyCode::Up | KeyCode::Char('k') => app.move_up(),

        KeyCode::Down | KeyCode::Char('j') => app.move_down(),

        KeyCode::Left | KeyCode::Char('h') | KeyCode::PageUp => app.prev_page(),

        KeyCode::Right | KeyCode::Char('l') | KeyCode::PageDown => app.next_page(),

        _ => {}
    }
    false
}

// ── Filter mode ───────────────────────────────────────────────────────────────

fn handle_filter(code: KeyCode, app: &mut App) {
    match code {
        KeyCode::Esc => {
            app.filter_input.clear();
            app.update_display_list();
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            // Confirm filter — keep query active and return to Normal
            app.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            app.filter_input.pop();
            app.update_display_list();
        }
        KeyCode::Char(c) => {
            app.filter_input.push(c);
            app.update_display_list();
        }
        _ => {}
    }
}

// ── Confirm dialog ────────────────────────────────────────────────────────────

fn handle_confirm(code: KeyCode, app: &mut App, is_selected: bool) {
    match code {
        KeyCode::Char('y') | KeyCode::Enter => {
            if is_selected {
                app.clear_selected_notifications();
            } else {
                app.clear_all_notifications();
            }
            app.mode = Mode::Normal;
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        _ => {}
    }
}

// ── Help popup ────────────────────────────────────────────────────────────────

fn handle_help(code: KeyCode, app: &mut App) {
    match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => {
            app.mode = Mode::Normal;
        }
        _ => {}
    }
}
