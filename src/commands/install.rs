//! Install a package from local path.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::fgp_services_dir;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    protocol: String,
    daemon: DaemonConfig,
    #[serde(default)]
    skills: HashMap<String, SkillConfig>,
    #[serde(default)]
    auth: Option<AuthConfig>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct DaemonConfig {
    entrypoint: String,
    #[serde(default)]
    socket: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SkillConfig {
    source: String,
    target: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AuthConfig {
    #[serde(rename = "type")]
    auth_type: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    credentials_path: String,
    #[serde(default)]
    token_path: String,
}

/// Known AI agent configurations for skill distribution.
const AGENT_CONFIGS: &[(&str, &str, &str)] = &[
    ("claude-code", "~/.claude/skills", "Claude Code"),
    ("cursor", "~/.cursor/rules", "Cursor"),
    ("windsurf", "~/.windsurf/workflows", "Windsurf"),
    ("continue", "~/.continue/rules", "Continue"),
];

pub fn run(path: &str) -> Result<()> {
    let package_path = Path::new(path);

    // Support both directory and manifest.json path
    let (package_dir, manifest_path) = if package_path.is_dir() {
        (
            package_path.to_path_buf(),
            package_path.join("manifest.json"),
        )
    } else if package_path
        .file_name()
        .map(|f| f == "manifest.json")
        .unwrap_or(false)
    {
        (
            package_path
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf(),
            package_path.to_path_buf(),
        )
    } else {
        bail!("Expected a directory or manifest.json path");
    };

    if !manifest_path.exists() {
        bail!("manifest.json not found at {}", manifest_path.display());
    }

    // Parse manifest
    let manifest_content =
        fs::read_to_string(&manifest_path).context("Failed to read manifest.json")?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_content).context("Failed to parse manifest.json")?;

    println!();
    println!(
        "{} Installing {} v{}...",
        "→".blue().bold(),
        manifest.name.bold(),
        manifest.version
    );

    // Step 1: Detect installed agents
    let detected_agents = detect_agents();
    if !detected_agents.is_empty() {
        println!(
            "  {} Detected agents: {}",
            "✓".green(),
            detected_agents
                .iter()
                .map(|(_, name)| *name)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Step 2: Create service directory and copy daemon files
    let service_dir = fgp_services_dir().join(&manifest.name);
    fs::create_dir_all(&service_dir).context("Failed to create service directory")?;

    println!(
        "  {} Daemon installed to {}",
        "✓".green(),
        format!("~/.fgp/services/{}/", manifest.name).dimmed()
    );
    copy_dir_contents(&package_dir, &service_dir).context("Failed to copy daemon files")?;

    // Step 3: Install skill files for detected agents
    let mut installed_skills = Vec::new();
    for (agent_id, agent_name) in &detected_agents {
        // Check if we have skills for this agent
        if let Some(skill_config) = manifest.skills.get(*agent_id) {
            let source_path = package_dir.join(&skill_config.source);
            if !source_path.exists() {
                continue;
            }

            // Expand target path
            let target_expanded = shellexpand::tilde(&skill_config.target);
            let target_path = Path::new(target_expanded.as_ref());

            // Create target directory
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Copy skill files
            copy_dir_contents(&source_path, target_path)
                .with_context(|| format!("Failed to install {} skill", agent_id))?;

            println!("  {} {} skill installed", "✓".green(), agent_name);
            installed_skills.push(*agent_name);
        }
    }

    // Step 4: Auth configuration
    if let Some(auth) = &manifest.auth {
        let creds_expanded = shellexpand::tilde(&auth.credentials_path);
        let creds_path = Path::new(creds_expanded.as_ref());

        if creds_path.exists() {
            println!("  {} OAuth configured ({})", "✓".green(), auth.provider);
        } else {
            println!(
                "  {} OAuth credentials needed at {}",
                "!".yellow(),
                auth.credentials_path
            );
        }
    }

    // Summary
    println!();
    println!(
        "  {} {} is now available in all your AI agents!",
        "✓".green().bold(),
        manifest.name.bold()
    );
    println!();

    // Socket path for reference
    let socket_path = format!("~/.fgp/services/{}/daemon.sock", manifest.name);
    println!("  Socket: {}", socket_path.dimmed());
    println!();

    // Next steps
    println!("{}", "Next steps:".bold());
    println!(
        "  1. Start daemon: {}",
        format!("fgp start {}", manifest.name).cyan()
    );
    println!(
        "  2. Test: {}",
        format!("fgp methods {}", manifest.name).cyan()
    );
    println!();

    Ok(())
}

/// Detect which AI agents are installed on the system.
fn detect_agents() -> Vec<(&'static str, &'static str)> {
    let mut agents = Vec::new();

    for (agent_id, path, name) in AGENT_CONFIGS {
        let expanded = shellexpand::tilde(path);
        let agent_path = Path::new(expanded.as_ref());

        if agent_path.exists() {
            agents.push((*agent_id, *name));
        }
    }

    agents
}

/// Copy directory contents recursively.
fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
