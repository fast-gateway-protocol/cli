//! Check health of a specific service.

use anyhow::{bail, Context, Result};
use colored::Colorize;

use super::service_socket_path;

pub fn run(service: &str) -> Result<()> {
    let socket_path = service_socket_path(service);

    if !socket_path.exists() {
        bail!(
            "Service '{}' is not running. Run 'fgp start {}' first.",
            service,
            service
        );
    }

    let client = fgp_daemon::FgpClient::new(&socket_path).context("Failed to connect to daemon")?;

    let start = std::time::Instant::now();
    let response = client.health().context("Failed to get health")?;
    let elapsed = start.elapsed();

    if response.ok {
        let result = response.result.unwrap_or_default();

        let status = result["status"].as_str().unwrap_or("unknown");
        let version = result["version"].as_str().unwrap_or("?");
        let uptime = result["uptime_seconds"].as_u64().unwrap_or(0);
        let pid = result["pid"].as_u64().unwrap_or(0);

        let status_icon = match status {
            "healthy" => "●".green(),
            "degraded" => "◐".yellow(),
            "unhealthy" => "○".red(),
            _ => "?".dimmed(),
        };

        println!("{} {} {}", status_icon, service.bold(), status);
        println!();
        println!("  Version:  {}", version);
        println!("  PID:      {}", pid);
        println!("  Uptime:   {}", format_uptime(uptime));
        println!("  Latency:  {:.1}ms", elapsed.as_secs_f64() * 1000.0);

        // Print sub-services if any
        if let Some(services) = result["services"].as_object() {
            if !services.is_empty() {
                println!();
                println!("  Sub-services:");
                for (name, status) in services {
                    let ok = status["ok"].as_bool().unwrap_or(false);
                    let icon = if ok { "✓".green() } else { "✗".red() };
                    let msg = status["message"].as_str().unwrap_or("");
                    println!("    {} {}: {}", icon, name, msg);
                }
            }
        }
    } else {
        let error = response.error.unwrap_or_default();
        eprintln!(
            "{} {} - Error ({}): {}",
            "○".red(),
            service.bold(),
            error.code,
            error.message
        );
        std::process::exit(1);
    }

    Ok(())
}

/// Format uptime seconds into human-readable string.
fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{} seconds", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let secs = secs % 60;
        format!("{} min {} sec", mins, secs)
    } else if secs < 86400 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{} hours {} min", hours, mins)
    } else {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        format!("{} days {} hours", days, hours)
    }
}
