//! Event handling for the TUI dashboard.

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyEvent};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Application events.
#[derive(Debug)]
#[allow(dead_code)]
pub enum Event {
    /// Terminal tick (for UI refresh).
    Tick,
    /// Key press event.
    Key(KeyEvent),
    /// Refresh services (from polling).
    Refresh,
    /// Terminal resize.
    Resize(u16, u16),
}

/// Event handler that manages input and tick events.
pub struct EventHandler {
    /// Event receiver.
    receiver: mpsc::Receiver<Event>,
    /// Input handler thread.
    #[allow(dead_code)]
    input_handle: thread::JoinHandle<()>,
    /// Tick handler thread.
    #[allow(dead_code)]
    tick_handle: thread::JoinHandle<()>,
    /// Refresh handler thread.
    #[allow(dead_code)]
    refresh_handle: thread::JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler.
    ///
    /// # Arguments
    /// * `tick_rate` - How often to send tick events (for UI refresh)
    /// * `poll_rate` - How often to poll service health
    pub fn new(tick_rate: Duration, poll_rate: Duration) -> Self {
        let (sender, receiver) = mpsc::channel();

        // Input handler thread
        let input_sender = sender.clone();
        let input_handle = thread::spawn(move || {
            loop {
                // Poll for input events with a small timeout
                if event::poll(Duration::from_millis(50)).unwrap_or(false) {
                    if let Ok(evt) = event::read() {
                        let event = match evt {
                            CrosstermEvent::Key(key) => Some(Event::Key(key)),
                            CrosstermEvent::Resize(w, h) => Some(Event::Resize(w, h)),
                            _ => None,
                        };

                        if let Some(event) = event {
                            if input_sender.send(event).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
        });

        // Tick handler thread
        let tick_sender = sender.clone();
        let tick_handle = thread::spawn(move || loop {
            thread::sleep(tick_rate);
            if tick_sender.send(Event::Tick).is_err() {
                break;
            }
        });

        // Refresh handler thread (service polling)
        let refresh_sender = sender;
        let refresh_handle = thread::spawn(move || loop {
            thread::sleep(poll_rate);
            if refresh_sender.send(Event::Refresh).is_err() {
                break;
            }
        });

        Self {
            receiver,
            input_handle,
            tick_handle,
            refresh_handle,
        }
    }

    /// Get the next event, blocking until one is available.
    pub fn next(&self) -> Result<Event> {
        Ok(self.receiver.recv()?)
    }
}
