//! Stop a running daemon.

use anyhow::{bail, Result};
use colored::Colorize;

use super::service_socket_path;

pub fn run(service: &str) -> Result<()> {
    let socket_path = service_socket_path(service);

    if !socket_path.exists() {
        println!(
            "{} Service '{}' is not running (no socket found).",
            "!".yellow().bold(),
            service
        );
        return Ok(());
    }

    println!("{} Stopping {}...", "→".blue().bold(), service.bold());

    // Connect and send stop command
    let client = match fgp_daemon::FgpClient::new(&socket_path) {
        Ok(c) => c,
        Err(e) => {
            // Socket exists but can't connect - probably stale
            println!("{} Could not connect to daemon: {}", "!".yellow().bold(), e);
            println!("  Removing stale socket...");
            let _ = std::fs::remove_file(&socket_path);
            return Ok(());
        }
    };

    match client.stop() {
        Ok(response) => {
            if response.ok {
                println!("{} {} stopped.", "✓".green().bold(), service.bold());
            } else {
                bail!(
                    "Stop command returned error: {}",
                    response.error.map(|e| e.message).unwrap_or_default()
                );
            }
        }
        Err(e) => {
            // Connection error might mean daemon stopped already
            println!(
                "{} Connection lost (daemon may have stopped): {}",
                "?".yellow().bold(),
                e
            );
        }
    }

    Ok(())
}
