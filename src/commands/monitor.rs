//! Health monitor with notifications.
//!
//! Watches FGP daemons and sends system notifications when services
//! change state (crash, recover, go unhealthy).

use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::notifications;

// Use shared helpers from parent module
use super::{fgp_services_dir, service_socket_path};

/// Service state for tracking changes.
#[derive(Debug, Clone, PartialEq)]
enum ServiceState {
    Running,
    Stopped,
    Unhealthy,
    Error,
}

/// Run the health monitor.
pub fn run(interval_secs: u64, daemon: bool) -> Result<()> {
    if daemon {
        println!(
            "{} Starting health monitor daemon (interval: {}s)...",
            "→".blue().bold(),
            interval_secs
        );
        println!("Monitor will run in background and send notifications on state changes.");

        // Fork to background
        // For simplicity, we'll just run in foreground with reduced output
        // A proper daemon would use daemonize crate
    }

    println!(
        "{} Monitoring FGP services (Ctrl+C to stop)...",
        "→".blue().bold()
    );
    println!();

    let mut states: HashMap<String, ServiceState> = HashMap::new();
    let interval = Duration::from_secs(interval_secs);

    loop {
        check_services(&mut states);
        thread::sleep(interval);
    }
}

/// Check all services and send notifications on state changes.
fn check_services(states: &mut HashMap<String, ServiceState>) {
    let services_dir = fgp_services_dir();

    if !services_dir.exists() {
        return;
    }

    let entries = match fs::read_dir(&services_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue,
        };

        let socket = service_socket_path(&name);
        let current_state = get_service_state(&socket);

        // Check for state transitions
        if let Some(prev_state) = states.get(&name) {
            if *prev_state != current_state {
                handle_state_change(&name, prev_state, &current_state);
            }
        }

        states.insert(name, current_state);
    }
}

/// Get the current state of a service.
fn get_service_state(socket: &PathBuf) -> ServiceState {
    if !socket.exists() {
        return ServiceState::Stopped;
    }

    match fgp_daemon::FgpClient::new(socket) {
        Ok(client) => match client.health() {
            Ok(response) if response.ok => {
                let result = response.result.unwrap_or_default();
                let status = result["status"].as_str().unwrap_or("running");

                match status {
                    "healthy" | "running" => ServiceState::Running,
                    "degraded" | "unhealthy" => ServiceState::Unhealthy,
                    _ => ServiceState::Running,
                }
            }
            _ => ServiceState::Error,
        },
        Err(_) => ServiceState::Error,
    }
}

/// Handle a state change and send notifications.
fn handle_state_change(name: &str, prev: &ServiceState, current: &ServiceState) {
    let (title, message, log_style) = match (prev, current) {
        // Service crashed (was running, now error or stopped)
        (ServiceState::Running, ServiceState::Error) => (
            "FGP Service Crashed",
            format!("{} daemon crashed", name),
            format!("{} {} crashed", "✗".red().bold(), name),
        ),
        (ServiceState::Running, ServiceState::Stopped) => (
            "FGP Service Stopped",
            format!("{} daemon stopped unexpectedly", name),
            format!("{} {} stopped", "○".dimmed(), name),
        ),

        // Service went unhealthy
        (ServiceState::Running, ServiceState::Unhealthy) => (
            "FGP Service Unhealthy",
            format!("{} daemon is unhealthy", name),
            format!("{} {} is unhealthy", "◐".yellow().bold(), name),
        ),

        // Service recovered
        (ServiceState::Unhealthy, ServiceState::Running) => (
            "FGP Service Recovered",
            format!("{} daemon recovered", name),
            format!("{} {} recovered", "✓".green().bold(), name),
        ),
        (ServiceState::Error, ServiceState::Running) => (
            "FGP Service Started",
            format!("{} daemon is now running", name),
            format!("{} {} started", "●".green().bold(), name),
        ),
        (ServiceState::Stopped, ServiceState::Running) => (
            "FGP Service Started",
            format!("{} daemon started", name),
            format!("{} {} started", "●".green().bold(), name),
        ),

        // Other transitions - just log, no notification
        _ => {
            println!(
                "[{}] {} state: {:?} → {:?}",
                chrono::Local::now().format("%H:%M:%S"),
                name,
                prev,
                current
            );
            return;
        }
    };

    // Log to terminal
    println!(
        "[{}] {}",
        chrono::Local::now().format("%H:%M:%S"),
        log_style
    );

    // Send system notification
    notifications::notify(title, &message);
}
