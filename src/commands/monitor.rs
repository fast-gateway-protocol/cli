//! Health monitor with notifications and auto-restart watchdog.
//!
//! Watches FGP daemons and sends system notifications when services
//! change state (crash, recover, go unhealthy). Optionally auto-restarts
//! crashed services.

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

/// Watchdog configuration for auto-restart.
#[derive(Clone)]
struct WatchdogConfig {
    enabled: bool,
    max_restarts: u32,
    restart_delay: Duration,
}

/// Per-service restart tracking.
#[derive(Default)]
struct RestartTracker {
    attempts: HashMap<String, u32>,
}

/// Run the health monitor.
pub fn run(
    interval_secs: u64,
    daemon: bool,
    auto_restart: bool,
    max_restarts: u32,
    restart_delay_secs: u64,
) -> Result<()> {
    let watchdog = WatchdogConfig {
        enabled: auto_restart,
        max_restarts,
        restart_delay: Duration::from_secs(restart_delay_secs),
    };

    if daemon {
        println!(
            "{} Starting health monitor daemon (interval: {}s)...",
            "→".blue().bold(),
            interval_secs
        );
        println!("Monitor will run in background and send notifications on state changes.");
    }

    println!(
        "{} Monitoring FGP services (Ctrl+C to stop)...",
        "→".blue().bold()
    );

    if watchdog.enabled {
        let max_str = if max_restarts == 0 {
            "unlimited".to_string()
        } else {
            format!("{}", max_restarts)
        };
        println!(
            "{} Auto-restart enabled (max: {}, delay: {}s)",
            "⟳".cyan().bold(),
            max_str,
            restart_delay_secs
        );
    }
    println!();

    let mut states: HashMap<String, ServiceState> = HashMap::new();
    let mut restart_tracker = RestartTracker::default();
    let interval = Duration::from_secs(interval_secs);

    loop {
        check_services(&mut states, &watchdog, &mut restart_tracker);
        thread::sleep(interval);
    }
}

/// Check all services and send notifications on state changes.
fn check_services(
    states: &mut HashMap<String, ServiceState>,
    watchdog: &WatchdogConfig,
    restart_tracker: &mut RestartTracker,
) {
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
                handle_state_change(&name, prev_state, &current_state, watchdog, restart_tracker);

                // Reset restart counter if service came back up
                if current_state == ServiceState::Running {
                    restart_tracker.attempts.remove(&name);
                }
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
fn handle_state_change(
    name: &str,
    prev: &ServiceState,
    current: &ServiceState,
    watchdog: &WatchdogConfig,
    restart_tracker: &mut RestartTracker,
) {
    let should_restart = matches!(
        (prev, current),
        (ServiceState::Running, ServiceState::Error)
            | (ServiceState::Running, ServiceState::Stopped)
    );

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

    // Auto-restart if enabled and service crashed
    if watchdog.enabled && should_restart {
        attempt_restart(name, watchdog, restart_tracker);
    }
}

/// Attempt to restart a crashed service.
fn attempt_restart(name: &str, watchdog: &WatchdogConfig, restart_tracker: &mut RestartTracker) {
    let attempts = restart_tracker.attempts.entry(name.to_string()).or_insert(0);
    *attempts += 1;

    // Check if we've exceeded max restarts (0 = unlimited)
    if watchdog.max_restarts > 0 && *attempts > watchdog.max_restarts {
        println!(
            "[{}] {} {} exceeded max restarts ({}), not restarting",
            chrono::Local::now().format("%H:%M:%S"),
            "⚠".yellow().bold(),
            name,
            watchdog.max_restarts
        );
        notifications::notify(
            "FGP Restart Limit Reached",
            &format!("{} exceeded {} restart attempts", name, watchdog.max_restarts),
        );
        return;
    }

    println!(
        "[{}] {} Restarting {} (attempt {}{})...",
        chrono::Local::now().format("%H:%M:%S"),
        "⟳".cyan().bold(),
        name,
        attempts,
        if watchdog.max_restarts > 0 {
            format!("/{}", watchdog.max_restarts)
        } else {
            String::new()
        }
    );

    // Wait before restarting
    thread::sleep(watchdog.restart_delay);

    // Attempt to start the service
    match fgp_daemon::lifecycle::start_service(name) {
        Ok(()) => {
            println!(
                "[{}] {} {} restart initiated",
                chrono::Local::now().format("%H:%M:%S"),
                "→".blue().bold(),
                name
            );
        }
        Err(e) => {
            println!(
                "[{}] {} Failed to restart {}: {}",
                chrono::Local::now().format("%H:%M:%S"),
                "✗".red().bold(),
                name,
                e
            );
            notifications::notify(
                "FGP Restart Failed",
                &format!("Failed to restart {}: {}", name, e),
            );
        }
    }
}
