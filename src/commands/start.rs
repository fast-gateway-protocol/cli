//! Start a daemon service.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

use super::{fgp_services_dir, service_socket_path};

pub fn run(service: &str, foreground: bool) -> Result<()> {
    let service_dir = fgp_services_dir().join(service);

    // Check if service is installed
    let manifest_path = service_dir.join("manifest.json");
    if !manifest_path.exists() {
        bail!(
            "Service '{}' is not installed. Run 'fgp install <path>' first.",
            service
        );
    }

    // Check if already running
    let socket_path = service_socket_path(service);
    if socket_path.exists() {
        // Try to connect to see if it's actually running
        if fgp_daemon::FgpClient::new(&socket_path)
            .map(|c| c.is_running())
            .unwrap_or(false)
        {
            println!(
                "{} Service '{}' is already running.",
                "!".yellow().bold(),
                service
            );
            return Ok(());
        } else {
            // Stale socket, remove it
            let _ = fs::remove_file(&socket_path);
        }
    }

    // Read manifest to get entrypoint
    let manifest_content =
        fs::read_to_string(&manifest_path).context("Failed to read manifest.json")?;
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_content).context("Failed to parse manifest.json")?;

    let entrypoint = manifest["daemon"]["entrypoint"]
        .as_str()
        .context("manifest.json missing daemon.entrypoint")?;

    let entrypoint_path = service_dir.join(entrypoint);
    if !entrypoint_path.exists() {
        bail!("Daemon entrypoint not found: {}", entrypoint_path.display());
    }

    println!("{} Starting {}...", "→".blue().bold(), service.bold());

    if foreground {
        // Run in foreground (blocking)
        let status = Command::new(&entrypoint_path)
            .current_dir(&service_dir)
            .status()
            .context("Failed to start daemon")?;

        if !status.success() {
            bail!("Daemon exited with status: {}", status);
        }
    } else {
        // Start as background process
        let child = Command::new(&entrypoint_path)
            .current_dir(&service_dir)
            .spawn()
            .context("Failed to start daemon")?;

        // Wait a moment for socket to appear
        std::thread::sleep(std::time::Duration::from_millis(500));

        if socket_path.exists() {
            println!(
                "{} {} started (PID: {})",
                "✓".green().bold(),
                service.bold(),
                child.id()
            );
            println!("  Socket: {}", socket_path.display().to_string().dimmed());
        } else {
            println!(
                "{} Daemon started but socket not found yet. Check logs.",
                "?".yellow().bold()
            );
        }
    }

    Ok(())
}

/// Check if a path looks like a valid FGP service directory.
#[allow(dead_code)]
pub fn is_valid_service_dir(path: &Path) -> bool {
    path.join("manifest.json").exists()
}
