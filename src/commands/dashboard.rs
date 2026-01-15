//! Launch the FGP dashboard web UI.

use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

pub fn run(port: u16, open: bool) -> Result<()> {
    // Find the dashboard binary
    let dashboard_bin = find_dashboard_binary()?;

    println!(
        "{} Starting FGP Dashboard on port {}...",
        "→".blue().bold(),
        port
    );

    let url = format!("http://localhost:{}", port);

    // Build command
    let mut cmd = Command::new(&dashboard_bin);
    cmd.arg("--port").arg(port.to_string());

    if open {
        cmd.arg("--open");
    }

    println!("{}", format!("Dashboard URL: {}", url).dimmed());
    println!("{}", "Press Ctrl+C to stop".dimmed());
    println!();

    // Run the dashboard (blocks until interrupted)
    let status = cmd.status().context("Failed to start dashboard")?;

    if !status.success() {
        anyhow::bail!("Dashboard exited with status: {}", status);
    }

    Ok(())
}

/// Find the fgp-dashboard binary
fn find_dashboard_binary() -> Result<std::path::PathBuf> {
    // Try several locations:
    // 1. Same directory as the current executable
    // 2. In PATH
    // 3. Relative to current directory (for development)

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    // Check same directory as fgp binary
    if let Some(dir) = exe_dir {
        let dashboard_path = dir.join("fgp-dashboard");
        if dashboard_path.exists() {
            return Ok(dashboard_path);
        }
    }

    // Check if in PATH
    if let Ok(output) = Command::new("which").arg("fgp-dashboard").output() {
        if output.status.success() {
            let path_str = String::from_utf8_lossy(&output.stdout);
            let path = std::path::PathBuf::from(path_str.trim());
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // Development fallback: check common build directories
    let dev_paths = [
        "../dashboard/target/release/fgp-dashboard",
        "../dashboard/target/debug/fgp-dashboard",
        "./target/release/fgp-dashboard",
        "./target/debug/fgp-dashboard",
    ];

    for path in dev_paths {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p.canonicalize()?);
        }
    }

    anyhow::bail!(
        "Could not find fgp-dashboard binary. \
        Install it with: cargo install --path ~/Projects/fgp/dashboard"
    )
}
