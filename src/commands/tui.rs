//! TUI command - Interactive terminal dashboard.

use anyhow::Result;
use std::time::Duration;

/// Run the TUI dashboard.
pub fn run(poll_interval_ms: u64) -> Result<()> {
    let poll_interval = Duration::from_millis(poll_interval_ms);
    crate::tui::run(poll_interval)
}
