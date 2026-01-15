//! Show status of all running daemons.

use anyhow::Result;
use colored::Colorize;
use std::fs;
use tabled::{Table, Tabled};

use super::{fgp_services_dir, service_socket_path};

#[derive(Tabled)]
struct ServiceStatus {
    #[tabled(rename = "Service")]
    name: String,
    #[tabled(rename = "Status")]
    status: String,
    #[tabled(rename = "Version")]
    version: String,
    #[tabled(rename = "Uptime")]
    uptime: String,
}

pub fn run(verbose: bool) -> Result<()> {
    let services_dir = fgp_services_dir();

    if !services_dir.exists() {
        println!(
            "{} No FGP services directory found at {}",
            "!".yellow().bold(),
            services_dir.display()
        );
        println!("  Run 'fgp install <package>' to install a service.");
        return Ok(());
    }

    let entries = fs::read_dir(&services_dir)?;
    let mut statuses: Vec<ServiceStatus> = Vec::new();
    let mut any_service = false;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let service_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        any_service = true;
        let socket_path = service_socket_path(service_name);

        let (status, version, uptime) = if socket_path.exists() {
            // Try to get health info
            match fgp_daemon::FgpClient::new(&socket_path) {
                Ok(client) => match client.health() {
                    Ok(response) if response.ok => {
                        let result = response.result.unwrap_or_default();
                        let version = result["version"].as_str().unwrap_or("?").to_string();
                        let uptime_secs = result["uptime_seconds"].as_u64().unwrap_or(0);
                        let uptime = format_uptime(uptime_secs);
                        let status_str = result["status"].as_str().unwrap_or("running");

                        let status_colored = match status_str {
                            "healthy" => "● running".green().to_string(),
                            "degraded" => "◐ degraded".yellow().to_string(),
                            _ => format!("● {}", status_str).green().to_string(),
                        };

                        (status_colored, version, uptime)
                    }
                    _ => (
                        "○ not responding".red().to_string(),
                        "-".to_string(),
                        "-".to_string(),
                    ),
                },
                Err(_) => (
                    "○ socket error".red().to_string(),
                    "-".to_string(),
                    "-".to_string(),
                ),
            }
        } else {
            (
                "○ stopped".dimmed().to_string(),
                "-".to_string(),
                "-".to_string(),
            )
        };

        statuses.push(ServiceStatus {
            name: service_name.to_string(),
            status,
            version,
            uptime,
        });

        if verbose && socket_path.exists() {
            // Print detailed health info
            if let Ok(client) = fgp_daemon::FgpClient::new(&socket_path) {
                if let Ok(response) = client.health() {
                    if response.ok {
                        if let Some(result) = response.result {
                            println!("\n{} {} health details:", "→".blue(), service_name.bold());
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&result)
                                    .unwrap_or_default()
                                    .dimmed()
                            );
                        }
                    }
                }
            }
        }
    }

    if !any_service {
        println!("{} No services installed.", "!".yellow().bold());
        println!("  Run 'fgp install <package>' to install a service.");
        return Ok(());
    }

    println!("{}", "FGP Services".bold());
    println!();

    let table = Table::new(&statuses).to_string();
    println!("{}", table);

    Ok(())
}

/// Format uptime seconds into human-readable string.
fn format_uptime(secs: u64) -> String {
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
