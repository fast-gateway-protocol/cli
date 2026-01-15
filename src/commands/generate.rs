//! Generate command - scaffolds new FGP daemons from templates.
//!
//! Uses the Python generator script from the generator/ directory.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::PathBuf;
use std::process::Command;

/// Get the path to the generator script.
fn generator_script_path() -> Result<PathBuf> {
    // Try relative to the CLI binary first (installed location)
    let exe_path = std::env::current_exe().context("Failed to get executable path")?;
    let exe_dir = exe_path.parent().unwrap();

    // Check various possible locations
    let candidates = [
        // Relative to binary (installed)
        exe_dir.join("../lib/fgp/generator/generate.py"),
        exe_dir.join("../../generator/generate.py"),
        // Development location
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../generator/generate.py"),
        // Absolute fallback
        PathBuf::from(shellexpand::tilde("~/.fgp/generator/generate.py").as_ref()),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.canonicalize().unwrap_or_else(|_| path.clone()));
        }
    }

    bail!(
        "Generator script not found. Looked in:\n{}",
        candidates
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// List all available service presets.
pub fn list() -> Result<()> {
    let script_path = generator_script_path()?;

    let output = Command::new("python3")
        .arg(&script_path)
        .arg("--list-presets")
        .output()
        .context("Failed to run generator script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Generator failed:\n{}", stderr);
    }

    // Print the output directly
    print!("{}", String::from_utf8_lossy(&output.stdout));

    Ok(())
}

/// Generate a new daemon from a service preset.
pub fn new_daemon(
    service: &str,
    preset: bool,
    display_name: Option<&str>,
    api_url: Option<&str>,
    env_token: Option<&str>,
    output_dir: Option<&str>,
    author: &str,
) -> Result<()> {
    let script_path = generator_script_path()?;

    println!();
    println!(
        "{} Generating FGP daemon: {}",
        "→".blue(),
        service.bold()
    );

    // Build command arguments
    let mut args = vec![script_path.to_string_lossy().to_string(), service.to_string()];

    if preset {
        args.push("--preset".to_string());
    }

    if let Some(name) = display_name {
        args.push("--display-name".to_string());
        args.push(name.to_string());
    }

    if let Some(url) = api_url {
        args.push("--api-url".to_string());
        args.push(url.to_string());
    }

    if let Some(token) = env_token {
        args.push("--env-token".to_string());
        args.push(token.to_string());
    }

    if let Some(dir) = output_dir {
        args.push("--output-dir".to_string());
        args.push(dir.to_string());
    }

    args.push("--author".to_string());
    args.push(author.to_string());

    let output = Command::new("python3")
        .args(&args[..])
        .output()
        .context("Failed to run generator script")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.is_empty() {
            print!("{}", stdout);
        }
        bail!("Generator failed:\n{}", stderr);
    }

    // Print the output directly
    print!("{}", String::from_utf8_lossy(&output.stdout));

    Ok(())
}
