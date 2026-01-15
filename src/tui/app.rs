//! Application state for the TUI dashboard.

use std::fs;
use std::time::{Duration, Instant};

/// Service status information.
#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub status: ServiceStatus,
    pub version: Option<String>,
    pub uptime_seconds: Option<u64>,
}

/// Service health states.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Unhealthy,
    Error,
    Starting,
    Stopping,
}

impl ServiceStatus {
    /// Get the status symbol for display.
    pub fn symbol(&self) -> &'static str {
        match self {
            ServiceStatus::Running => "●",
            ServiceStatus::Stopped => "○",
            ServiceStatus::Unhealthy => "◐",
            ServiceStatus::Error => "●",
            ServiceStatus::Starting => "◑",
            ServiceStatus::Stopping => "◑",
        }
    }

    /// Get the status text for display.
    #[allow(dead_code)]
    pub fn text(&self) -> &'static str {
        match self {
            ServiceStatus::Running => "running",
            ServiceStatus::Stopped => "stopped",
            ServiceStatus::Unhealthy => "unhealthy",
            ServiceStatus::Error => "error",
            ServiceStatus::Starting => "starting",
            ServiceStatus::Stopping => "stopping",
        }
    }
}

/// Message type for display.
#[derive(Debug, Clone)]
pub enum MessageType {
    Success,
    Error,
}

/// Main application state.
pub struct App {
    /// List of discovered services.
    pub services: Vec<ServiceInfo>,

    /// Currently selected service index.
    pub selected: usize,

    /// Last refresh timestamp.
    pub last_refresh: Instant,

    /// Whether app should quit.
    pub should_quit: bool,

    /// Message to display (auto-clears after timeout).
    pub message: Option<(String, MessageType, Instant)>,

    /// Message display duration.
    pub message_timeout: Duration,

    /// Whether help overlay is visible.
    pub show_help: bool,

    /// Whether detail overlay is visible.
    pub show_detail: bool,

    /// Methods for the currently selected service (for detail view).
    pub detail_methods: Vec<String>,
}

impl App {
    /// Create a new app instance.
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
            selected: 0,
            last_refresh: Instant::now(),
            should_quit: false,
            message: None,
            message_timeout: Duration::from_secs(3),
            show_help: false,
            show_detail: false,
            detail_methods: Vec::new(),
        }
    }

    /// Tick handler - called on each frame.
    pub fn tick(&mut self) {
        // Clear expired messages
        if let Some((_, _, created)) = &self.message {
            if created.elapsed() >= self.message_timeout {
                self.message = None;
            }
        }
    }

    /// Refresh service list from filesystem.
    pub fn refresh_services(&mut self) {
        self.services = discover_services();
        self.last_refresh = Instant::now();

        // Ensure selection is valid
        if self.selected >= self.services.len() && !self.services.is_empty() {
            self.selected = self.services.len() - 1;
        }
    }

    /// Select the previous service.
    pub fn select_previous(&mut self) {
        if !self.services.is_empty() && self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Select the next service.
    pub fn select_next(&mut self) {
        if !self.services.is_empty() && self.selected < self.services.len() - 1 {
            self.selected += 1;
        }
    }

    /// Select the first service.
    pub fn select_first(&mut self) {
        self.selected = 0;
    }

    /// Select the last service.
    pub fn select_last(&mut self) {
        if !self.services.is_empty() {
            self.selected = self.services.len() - 1;
        }
    }

    /// Get the currently selected service.
    pub fn selected_service(&self) -> Option<&ServiceInfo> {
        self.services.get(self.selected)
    }

    /// Start the selected service.
    pub fn start_selected(&mut self) {
        if let Some(service) = self.selected_service().cloned() {
            if service.status == ServiceStatus::Stopped || service.status == ServiceStatus::Error {
                match fgp_daemon::lifecycle::start_service(&service.name) {
                    Ok(()) => {
                        self.set_message(format!("Started {}", service.name), MessageType::Success);
                        self.refresh_services();
                    }
                    Err(e) => {
                        self.set_message(
                            format!("Failed to start {}: {}", service.name, e),
                            MessageType::Error,
                        );
                    }
                }
            }
        }
    }

    /// Stop the selected service.
    pub fn stop_selected(&mut self) {
        if let Some(service) = self.selected_service().cloned() {
            if service.status == ServiceStatus::Running
                || service.status == ServiceStatus::Unhealthy
            {
                match fgp_daemon::lifecycle::stop_service(&service.name) {
                    Ok(()) => {
                        self.set_message(format!("Stopped {}", service.name), MessageType::Success);
                        self.refresh_services();
                    }
                    Err(e) => {
                        self.set_message(
                            format!("Failed to stop {}: {}", service.name, e),
                            MessageType::Error,
                        );
                    }
                }
            }
        }
    }

    /// Restart the selected service.
    pub fn restart_selected(&mut self) {
        if let Some(service) = self.selected_service().cloned() {
            if service.status == ServiceStatus::Running
                || service.status == ServiceStatus::Unhealthy
            {
                // Stop first
                if let Err(e) = fgp_daemon::lifecycle::stop_service(&service.name) {
                    self.set_message(
                        format!("Failed to stop {}: {}", service.name, e),
                        MessageType::Error,
                    );
                    return;
                }

                // Poll for service to actually stop (max 1 second)
                let socket = fgp_daemon::lifecycle::service_socket_path(&service.name);
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !socket.exists() {
                        break;
                    }
                    // Also check if socket exists but daemon is not responding
                    if let Ok(client) = fgp_daemon::FgpClient::new(&socket) {
                        if client.health().is_err() {
                            break;
                        }
                    } else {
                        break;
                    }
                }

                // Start again
                match fgp_daemon::lifecycle::start_service(&service.name) {
                    Ok(()) => {
                        self.set_message(
                            format!("Restarted {}", service.name),
                            MessageType::Success,
                        );
                        self.refresh_services();
                    }
                    Err(e) => {
                        self.set_message(
                            format!("Failed to restart {}: {}", service.name, e),
                            MessageType::Error,
                        );
                    }
                }
            }
        }
    }

    /// Toggle detail overlay.
    pub fn toggle_detail(&mut self) {
        if !self.show_detail {
            self.load_service_methods();
        }
        self.show_detail = !self.show_detail;
    }

    /// Load methods for the currently selected service.
    fn load_service_methods(&mut self) {
        self.detail_methods.clear();
        if let Some(service) = self.selected_service() {
            let socket = fgp_daemon::lifecycle::service_socket_path(&service.name);
            match fgp_daemon::FgpClient::new(&socket) {
                Ok(client) => match client.methods() {
                    Ok(response) if response.ok => {
                        if let Some(result) = response.result {
                            if let Some(methods) = result["methods"].as_array() {
                                for method in methods {
                                    let name = method["name"].as_str().unwrap_or("unknown");
                                    // Skip internal methods
                                    if name != "health" && name != "stop" && name != "methods" {
                                        self.detail_methods.push(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        self.detail_methods
                            .push("Error loading methods".to_string());
                    }
                },
                Err(_) => {
                    self.detail_methods.push("Daemon not running".to_string());
                }
            }
        }
    }

    /// Set a message to display.
    pub fn set_message(&mut self, text: String, msg_type: MessageType) {
        self.message = Some((text, msg_type, Instant::now()));
    }

    /// Toggle help overlay.
    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// Discover all installed services.
fn discover_services() -> Vec<ServiceInfo> {
    let services_dir = fgp_daemon::lifecycle::fgp_services_dir();

    if !services_dir.exists() {
        return Vec::new();
    }

    let entries = match fs::read_dir(&services_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut services = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let socket_path = fgp_daemon::lifecycle::service_socket_path(&name);
        let (status, version, uptime) = get_service_status(&name, &socket_path);

        services.push(ServiceInfo {
            name,
            status,
            version,
            uptime_seconds: uptime,
        });
    }

    // Sort by name
    services.sort_by(|a, b| a.name.cmp(&b.name));
    services
}

/// Get the status of a service.
fn get_service_status(
    _name: &str,
    socket_path: &std::path::Path,
) -> (ServiceStatus, Option<String>, Option<u64>) {
    if !socket_path.exists() {
        return (ServiceStatus::Stopped, None, None);
    }

    match fgp_daemon::FgpClient::new(socket_path) {
        Ok(client) => match client.health() {
            Ok(response) if response.ok => {
                let result = response.result.unwrap_or_default();
                let version = result["version"].as_str().map(String::from);
                let uptime = result["uptime_seconds"].as_u64();
                let status_str = result["status"].as_str().unwrap_or("running");

                let status = match status_str {
                    "healthy" | "running" => ServiceStatus::Running,
                    "degraded" | "unhealthy" => ServiceStatus::Unhealthy,
                    _ => ServiceStatus::Running,
                };

                (status, version, uptime)
            }
            _ => (ServiceStatus::Error, None, None),
        },
        Err(_) => (ServiceStatus::Error, None, None),
    }
}

/// Format uptime seconds into human-readable string.
pub fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}
