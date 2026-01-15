//! TUI Dashboard for FGP daemon monitoring.
//!
//! Interactive terminal UI with real-time service status updates.

pub mod app;
pub mod event;
pub mod ui;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::time::Duration;

use app::App;
use event::{Event, EventHandler};

/// Run the TUI dashboard.
pub fn run(poll_interval: Duration) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state and event handler
    let mut app = App::new();
    let mut events = EventHandler::new(Duration::from_millis(100), poll_interval);

    // Initial service scan
    app.refresh_services();

    // Main loop
    let result = run_app(&mut terminal, &mut app, &mut events);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Main application loop.
fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    events: &mut EventHandler,
) -> Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| ui::draw(frame, app))?;

        // Handle events
        match events.next()? {
            Event::Tick => {
                app.tick();
            }
            Event::Key(key) => {
                use crossterm::event::KeyCode;

                match key.code {
                    // Quit / Close overlays
                    KeyCode::Esc => {
                        if app.show_detail {
                            app.show_detail = false;
                        } else if app.show_help {
                            app.show_help = false;
                        } else {
                            app.should_quit = true;
                        }
                    }
                    KeyCode::Char('q') => {
                        if !app.show_detail && !app.show_help {
                            app.should_quit = true;
                        }
                    }
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        if !app.show_detail {
                            app.select_previous();
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if !app.show_detail {
                            app.select_next();
                        }
                    }
                    KeyCode::Home => {
                        if !app.show_detail {
                            app.select_first();
                        }
                    }
                    KeyCode::End => {
                        if !app.show_detail {
                            app.select_last();
                        }
                    }
                    // Actions
                    KeyCode::Char('s') => {
                        if !app.show_detail && !app.show_help {
                            app.start_selected();
                        }
                    }
                    KeyCode::Enter | KeyCode::Char('d') => {
                        if !app.show_help {
                            app.toggle_detail();
                        }
                    }
                    KeyCode::Char('x') => {
                        if !app.show_detail && !app.show_help {
                            app.stop_selected();
                        }
                    }
                    KeyCode::Char('R') => {
                        if !app.show_detail && !app.show_help {
                            app.restart_selected();
                        }
                    }
                    KeyCode::Char('r') => {
                        if !app.show_detail && !app.show_help {
                            app.refresh_services();
                        }
                    }
                    KeyCode::Char('?') => {
                        if !app.show_detail {
                            app.toggle_help();
                        }
                    }
                    _ => {}
                }
            }
            Event::Refresh => {
                app.refresh_services();
            }
            Event::Resize(_, _) => {
                // Terminal will redraw automatically
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
