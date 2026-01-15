//! FGP skill management - install, update, and manage FGP skills from marketplaces.
//!
//! This module provides Claude Code plugin-like functionality for FGP daemons.
//! Skills are distributed via git-based marketplaces with automatic updates.
//!
//! # Directory Structure
//!
//! ```text
//! ~/.fgp/
//! ├── skills/
//! │   ├── installed_skills.json    # Track installed skills + versions
//! │   ├── known_marketplaces.json  # Track marketplace sources
//! │   ├── cache/                   # Installed skill files
//! │   │   └── <marketplace>/<skill>/<version>/
//! │   └── marketplaces/            # Cloned marketplace repos
//! │       └── <marketplace-name>/
//! └── services/                    # Running daemon sockets
//! ```

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::skill_tap;

/// Skill manifest format (skill.json)
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: Author,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub binary: Option<BinaryConfig>,
    #[serde(default)]
    pub distribution: Option<DistributionConfig>,
    #[serde(default)]
    pub daemon: Option<DaemonConfig>,
    #[serde(default)]
    pub methods: Vec<MethodDef>,
    #[serde(default)]
    pub mcp_bridge: Option<McpBridgeConfig>,
    #[serde(default)]
    pub requirements: HashMap<String, Requirement>,
    /// Multi-ecosystem export configuration
    #[serde(default)]
    pub exports: Option<ExportsConfig>,
}

/// Multi-ecosystem export configuration
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportsConfig {
    #[serde(default)]
    pub mcp: Option<McpExportConfig>,
    #[serde(default)]
    pub claude: Option<ClaudeExportConfig>,
    #[serde(default)]
    pub cursor: Option<CursorExportConfig>,
    #[serde(default)]
    pub continue_dev: Option<ContinueExportConfig>,
    #[serde(default)]
    pub windsurf: Option<WindsurfExportConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpExportConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tools_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeExportConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Skill name in Claude Code (defaults to <daemon>-fgp)
    #[serde(default)]
    pub skill_name: Option<String>,
    /// Keywords that trigger this skill
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Tools required by the skill (default: ["Bash"])
    #[serde(default = "default_bash_tools")]
    pub tools: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CursorExportConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Server name in mcp.json (defaults to fgp-<daemon>)
    #[serde(default)]
    pub server_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContinueExportConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Provider type (command, custom)
    #[serde(default = "default_command")]
    pub provider_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WindsurfExportConfig {
    #[serde(default)]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_bash_tools() -> Vec<String> {
    vec!["Bash".to_string()]
}

fn default_command() -> String {
    "command".to_string()
}

/// Export target types for multi-ecosystem registration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportTarget {
    Mcp,
    Claude,
    Cursor,
    ContinueDev,
    Windsurf,
    All,
}

impl ExportTarget {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "mcp" => Some(Self::Mcp),
            "claude" | "claude-code" => Some(Self::Claude),
            "cursor" => Some(Self::Cursor),
            "continue" | "continue-dev" => Some(Self::ContinueDev),
            "windsurf" => Some(Self::Windsurf),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn all_targets() -> Vec<Self> {
        vec![Self::Mcp, Self::Claude, Self::Cursor, Self::ContinueDev, Self::Windsurf]
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BinaryConfig {
    #[serde(rename = "type")]
    pub binary_type: String,
    #[serde(default)]
    pub cargo_package: Option<String>,
    #[serde(default)]
    pub build_command: Option<String>,
    #[serde(default)]
    pub executable: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DistributionConfig {
    #[serde(default)]
    pub prebuilt: HashMap<String, String>,
    #[serde(default)]
    pub homebrew: Option<HomebrewConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HomebrewConfig {
    pub tap: String,
    pub formula: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub name: String,
    #[serde(default)]
    pub socket_path: Option<String>,
    #[serde(default)]
    pub pid_file: Option<String>,
    #[serde(default)]
    pub log_file: Option<String>,
    #[serde(default)]
    pub start_command: Vec<String>,
    #[serde(default)]
    pub stop_command: Vec<String>,
    #[serde(default)]
    pub health_method: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MethodDef {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub params: HashMap<String, ParamDef>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ParamDef {
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpBridgeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub tools_prefix: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Requirement {
    #[serde(rename = "type")]
    pub req_type: String,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub min_version: Option<String>,
    #[serde(default)]
    pub install_hint: Option<String>,
}

/// Marketplace manifest format (marketplace.json)
#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceManifest {
    pub name: String,
    pub description: String,
    pub owner: Author,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    pub skills: Vec<MarketplaceSkill>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceSkill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Author,
    pub source: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// Installed skills tracking
#[derive(Debug, Serialize, Deserialize)]
pub struct InstalledSkills {
    pub version: u32,
    pub skills: HashMap<String, Vec<InstalledSkill>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledSkill {
    pub scope: String,
    #[serde(rename = "installPath")]
    pub install_path: String,
    pub version: String,
    #[serde(rename = "installedAt")]
    pub installed_at: String,
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
    #[serde(rename = "gitCommitSha")]
    pub git_commit_sha: Option<String>,
    #[serde(rename = "binaryPath")]
    pub binary_path: Option<String>,
}

/// Known marketplaces tracking
#[derive(Debug, Serialize, Deserialize)]
pub struct KnownMarketplaces {
    #[serde(flatten)]
    pub marketplaces: HashMap<String, MarketplaceEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    pub source: MarketplaceSource,
    #[serde(rename = "installLocation")]
    pub install_location: Option<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MarketplaceSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub repo: String,
}

/// Get the FGP home directory
fn fgp_home() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".fgp")
}

/// Get the skills directory
fn skills_dir() -> PathBuf {
    fgp_home().join("skills")
}

/// Get the installed skills file path
fn installed_skills_path() -> PathBuf {
    skills_dir().join("installed_skills.json")
}

/// Get the known marketplaces file path
fn known_marketplaces_path() -> PathBuf {
    skills_dir().join("known_marketplaces.json")
}

/// Get the marketplaces directory
fn marketplaces_dir() -> PathBuf {
    skills_dir().join("marketplaces")
}

/// Get the cache directory
fn cache_dir() -> PathBuf {
    skills_dir().join("cache")
}

/// Load installed skills
fn load_installed_skills() -> Result<InstalledSkills> {
    let path = installed_skills_path();
    if !path.exists() {
        return Ok(InstalledSkills {
            version: 1,
            skills: HashMap::new(),
        });
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Save installed skills
fn save_installed_skills(skills: &InstalledSkills) -> Result<()> {
    let path = installed_skills_path();
    fs::create_dir_all(path.parent().unwrap())?;
    let content = serde_json::to_string_pretty(skills)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Load known marketplaces
fn load_known_marketplaces() -> Result<KnownMarketplaces> {
    let path = known_marketplaces_path();
    if !path.exists() {
        return Ok(KnownMarketplaces {
            marketplaces: HashMap::new(),
        });
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Save known marketplaces
fn save_known_marketplaces(marketplaces: &KnownMarketplaces) -> Result<()> {
    let path = known_marketplaces_path();
    fs::create_dir_all(path.parent().unwrap())?;
    let content = serde_json::to_string_pretty(marketplaces)?;
    fs::write(&path, content)?;
    Ok(())
}

/// List all installed skills
pub fn list() -> Result<()> {
    let installed = load_installed_skills()?;

    if installed.skills.is_empty() {
        println!("{}", "No skills installed.".yellow());
        println!();
        println!("Install a skill with:");
        println!("  fgp skill install browser-gateway");
        println!();
        println!("Or add a marketplace first:");
        println!("  fgp skill marketplace add https://github.com/fast-gateway-protocol/fgp");
        return Ok(());
    }

    println!("{}", "Installed FGP Skills".bold());
    println!();

    for (skill_key, entries) in &installed.skills {
        for entry in entries {
            let status = if check_daemon_running(&skill_key.split('@').next().unwrap_or(skill_key))
            {
                "● running".green()
            } else {
                "○ stopped".dimmed()
            };

            println!(
                "  {} {} {} {}",
                skill_key.cyan(),
                format!("v{}", entry.version).dimmed(),
                status,
                format!("({})", entry.scope).dimmed()
            );
        }
    }

    Ok(())
}

/// Check if a daemon is running
fn check_daemon_running(service: &str) -> bool {
    let socket_path = fgp_home()
        .join("services")
        .join(service)
        .join("daemon.sock");
    socket_path.exists()
}

/// Search for skills in taps and marketplaces
pub fn search(query: &str) -> Result<()> {
    println!(
        "{} {}",
        "Searching for:".bold(),
        query.cyan()
    );
    println!();

    let mut found = false;

    // First search taps (new skill.yaml format)
    match skill_tap::search_taps(query) {
        Ok(results) => {
            if !results.is_empty() {
                println!("{}", "From taps:".bold().underline());
                for (tap_name, _path, manifest) in &results {
                    found = true;
                    println!(
                        "  {} {} (from {})",
                        manifest.name.cyan().bold(),
                        format!("v{}", manifest.version).dimmed(),
                        tap_name.dimmed()
                    );
                    println!("    {}", manifest.description);
                    if !manifest.keywords.is_empty() {
                        println!("    Keywords: {}", manifest.keywords.join(", ").dimmed());
                    }
                    if !manifest.daemons.is_empty() {
                        let daemon_names: Vec<_> = manifest.daemons.iter().map(|d| d.name.as_str()).collect();
                        println!("    Daemons: {}", daemon_names.join(", ").dimmed());
                    }
                    println!();
                }
            }
        }
        Err(_) => {} // Ignore tap search errors, continue with marketplaces
    }

    // Also search legacy marketplaces
    let marketplaces = load_known_marketplaces()?;
    if !marketplaces.marketplaces.is_empty() {
        let mut marketplace_found = false;
        for (name, entry) in &marketplaces.marketplaces {
            if let Some(ref location) = entry.install_location {
                let manifest_path = Path::new(location).join(".fgp").join("marketplace.json");
                if manifest_path.exists() {
                    let content = fs::read_to_string(&manifest_path)?;
                    let manifest: MarketplaceManifest = serde_json::from_str(&content)?;

                    for skill in &manifest.skills {
                        let query_lower = query.to_lowercase();
                        if skill.name.to_lowercase().contains(&query_lower)
                            || skill.description.to_lowercase().contains(&query_lower)
                            || skill.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                        {
                            if !marketplace_found {
                                println!("{}", "From marketplaces (legacy):".bold().underline());
                                marketplace_found = true;
                            }
                            found = true;
                            println!(
                                "  {} {} (from {})",
                                skill.name.cyan().bold(),
                                format!("v{}", skill.version).dimmed(),
                                name.dimmed()
                            );
                            println!("    {}", skill.description);
                            if !skill.tags.is_empty() {
                                println!("    Tags: {}", skill.tags.join(", ").dimmed());
                            }
                            println!();
                        }
                    }
                }
            }
        }
    }

    if !found {
        println!("{}", "No skills found matching your query.".yellow());
        println!();
        println!("Add a tap to search more skills:");
        println!(
            "  {}",
            "fgp skill tap add fast-gateway-protocol/official-skills".cyan()
        );
    }

    Ok(())
}

/// Install a skill
pub fn install(name: &str, from_marketplace: Option<&str>) -> Result<()> {
    println!(
        "{} {}...",
        "Installing skill:".bold(),
        name.cyan()
    );

    // First, try to find the skill in taps (new skill.yaml format)
    if from_marketplace.is_none() {
        if let Ok(Some((tap_name, skill_path, manifest))) = skill_tap::find_skill(name) {
            return install_from_tap(&tap_name, &skill_path, &manifest);
        }
    }

    // Fall back to legacy marketplaces
    let marketplaces = load_known_marketplaces()?;
    let mut skill_info: Option<(String, MarketplaceSkill, PathBuf)> = None;

    for (mp_name, entry) in &marketplaces.marketplaces {
        // Skip if specific marketplace requested and this isn't it
        if let Some(req_mp) = from_marketplace {
            if mp_name != req_mp {
                continue;
            }
        }

        if let Some(ref location) = entry.install_location {
            let manifest_path = Path::new(location).join(".fgp").join("marketplace.json");
            if manifest_path.exists() {
                let content = fs::read_to_string(&manifest_path)?;
                let manifest: MarketplaceManifest = serde_json::from_str(&content)?;

                for skill in manifest.skills {
                    if skill.name == name {
                        let source_path = Path::new(location).join(&skill.source);
                        skill_info = Some((mp_name.clone(), skill, source_path));
                        break;
                    }
                }
            }
        }

        if skill_info.is_some() {
            break;
        }
    }

    let (marketplace_name, skill, source_path) = match skill_info {
        Some(info) => info,
        None => {
            bail!(
                "Skill '{}' not found. Add a tap first:\n  fgp skill tap add fast-gateway-protocol/official-skills",
                name
            );
        }
    };

    println!("  Found in marketplace: {}", marketplace_name.green());
    println!("  Version: {}", skill.version);
    println!("  Source: {}", source_path.display());

    // Check for skill.json in the source
    let skill_manifest_path = source_path.join(".fgp").join("skill.json");
    if !skill_manifest_path.exists() {
        bail!(
            "Skill manifest not found at {}",
            skill_manifest_path.display()
        );
    }

    let skill_content = fs::read_to_string(&skill_manifest_path)?;
    let skill_manifest: SkillManifest = serde_json::from_str(&skill_content)?;

    // Create cache directory for this skill
    let cache_path = cache_dir()
        .join(&marketplace_name)
        .join(&skill.name)
        .join(&skill.version);
    fs::create_dir_all(&cache_path)?;

    // Copy skill files to cache (or symlink for development)
    println!("  Copying to cache...");

    // For now, just symlink for faster development iteration
    let source_link = cache_path.join("source");
    if source_link.exists() {
        fs::remove_file(&source_link)?;
    }
    std::os::unix::fs::symlink(&source_path, &source_link)?;

    // Build the binary if needed
    let binary_path = if let Some(ref binary) = skill_manifest.binary {
        if binary.binary_type == "rust" {
            println!("  Building Rust binary...");

            let build_cmd = binary
                .build_command
                .as_deref()
                .unwrap_or("cargo build --release");

            let status = Command::new("sh")
                .arg("-c")
                .arg(build_cmd)
                .current_dir(&source_path)
                .status()
                .context("Failed to run build command")?;

            if !status.success() {
                bail!("Build failed with exit code: {:?}", status.code());
            }

            if let Some(ref exe) = binary.executable {
                let exe_path = source_path.join(exe);
                if exe_path.exists() {
                    // Copy binary to cache
                    let dest_bin = cache_path.join(skill.name.clone());
                    fs::copy(&exe_path, &dest_bin)?;
                    println!("  Binary: {}", dest_bin.display());
                    Some(dest_bin.to_string_lossy().to_string())
                } else {
                    println!(
                        "  {}",
                        format!("Warning: executable not found at {}", exe_path.display()).yellow()
                    );
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    // Get git commit SHA if available
    let git_sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    // Update installed_skills.json
    let mut installed = load_installed_skills()?;
    let skill_key = format!("{}@{}", skill.name, marketplace_name);
    let now = chrono::Utc::now().to_rfc3339();

    let entry = InstalledSkill {
        scope: "user".to_string(),
        install_path: cache_path.to_string_lossy().to_string(),
        version: skill.version.clone(),
        installed_at: now.clone(),
        last_updated: now,
        git_commit_sha: git_sha,
        binary_path,
    };

    installed.skills.insert(skill_key.clone(), vec![entry.clone()]);
    save_installed_skills(&installed)?;

    // Auto-register with ecosystems based on exports config
    println!("  Registering with ecosystems...");
    let daemon_name = skill_manifest
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Always register with MCP (core FGP functionality)
    if let Some(ref bin_path) = entry.binary_path {
        let manifest = skill_to_daemon_manifest(&skill_manifest, bin_path);
        let services_dir = fgp_home().join("services").join(&daemon_name);
        fs::create_dir_all(&services_dir)?;
        let manifest_path = services_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        fs::write(&manifest_path, &manifest_json)?;
        println!("    {} MCP: {}", "✓".green(), manifest_path.display());
    }

    // Auto-register with other ecosystems based on exports config
    if let Some(ref exports) = skill_manifest.exports {
        // Claude Code
        if exports.claude.as_ref().map(|c| c.enabled).unwrap_or(false) {
            match export_to_claude(&skill_manifest) {
                Ok(()) => {}
                Err(e) => println!("    {} Claude: {}", "✗".red(), e),
            }
        }

        // Cursor
        if exports.cursor.as_ref().map(|c| c.enabled).unwrap_or(false) {
            match export_to_cursor(&skill_manifest) {
                Ok(()) => {}
                Err(e) => println!("    {} Cursor: {}", "✗".red(), e),
            }
        }

        // Windsurf
        if exports.windsurf.as_ref().map(|w| w.enabled).unwrap_or(false) {
            match export_to_windsurf(&skill_manifest) {
                Ok(()) => {}
                Err(e) => println!("    {} Windsurf: {}", "✗".red(), e),
            }
        }
    }

    println!();
    println!(
        "{} {} installed successfully!",
        "✓".green().bold(),
        skill.name.cyan()
    );
    println!();
    println!("Start the daemon with:");
    println!(
        "  {}",
        format!("fgp start {}", daemon_name)
    );
    println!();
    println!("To register with additional ecosystems:");
    println!(
        "  {}",
        format!("fgp skill mcp register {} --target=claude,cursor", skill.name)
    );

    Ok(())
}

/// Install a skill from a tap (skill.yaml format)
fn install_from_tap(
    tap_name: &str,
    skill_path: &Path,
    manifest: &super::skill_validate::SkillManifest,
) -> Result<()> {
    println!("  Found in tap: {}", tap_name.green());
    println!("  Version: {}", manifest.version);
    println!("  Path: {}", skill_path.display());

    // Check daemon dependencies
    if !manifest.daemons.is_empty() {
        println!();
        println!("  {}:", "Required daemons".bold());
        for daemon in &manifest.daemons {
            let optional = if daemon.optional { " (optional)" } else { "" };
            println!("    - {}{}", daemon.name.cyan(), optional.dimmed());
        }
    }

    // Create skills directory
    let skills_install_dir = skills_dir().join("installed").join(&manifest.name);
    fs::create_dir_all(&skills_install_dir)?;

    // Symlink skill to installed directory
    let source_link = skills_install_dir.join("source");
    if source_link.exists() || source_link.read_link().is_ok() {
        let _ = fs::remove_file(&source_link);
    }
    std::os::unix::fs::symlink(skill_path, &source_link)?;

    // Get git commit SHA if available
    let git_sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(skill_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    // Update installed_skills.json
    let mut installed = load_installed_skills()?;
    let skill_key = format!("{}@{}", manifest.name, tap_name);
    let now = chrono::Utc::now().to_rfc3339();

    let entry = InstalledSkill {
        scope: "tap".to_string(),
        install_path: skills_install_dir.to_string_lossy().to_string(),
        version: manifest.version.clone(),
        installed_at: now.clone(),
        last_updated: now,
        git_commit_sha: git_sha,
        binary_path: None, // skill.yaml packages typically don't have binaries
    };

    installed.skills.insert(skill_key.clone(), vec![entry]);
    save_installed_skills(&installed)?;

    // Export to agents if instructions are available
    println!();
    println!("  {}:", "Exporting to agents".bold());

    if let Some(ref instructions) = manifest.instructions {
        // Claude Code
        if instructions.claude_code.is_some() || instructions.core.is_some() {
            export_tap_skill_to_claude(skill_path, manifest)?;
        }

        // Cursor
        if instructions.cursor.is_some() {
            export_tap_skill_to_cursor(skill_path, manifest)?;
        }

        // Codex
        if instructions.codex.is_some() {
            println!("    {} Codex: available (use 'fgp skill export codex {}')", "○".dimmed(), manifest.name);
        }

        // MCP
        if instructions.mcp.is_some() {
            println!("    {} MCP: available (use 'fgp skill export mcp {}')", "○".dimmed(), manifest.name);
        }
    }

    println!();
    println!(
        "{} {} installed successfully!",
        "✓".green().bold(),
        manifest.name.cyan()
    );
    println!();
    println!("Use the skill by invoking its triggers:");
    if let Some(ref triggers) = manifest.triggers {
        if !triggers.keywords.is_empty() {
            println!("  Keywords: {}", triggers.keywords.join(", ").cyan());
        }
    }

    Ok(())
}

/// Export a tap skill to Claude Code
fn export_tap_skill_to_claude(
    skill_path: &Path,
    manifest: &super::skill_validate::SkillManifest,
) -> Result<()> {
    let claude_skills_dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".claude")
        .join("skills")
        .join(format!("{}-fgp", manifest.name));

    fs::create_dir_all(&claude_skills_dir)?;
    let skill_md_path = claude_skills_dir.join("SKILL.md");

    // Try to read instruction file or generate from manifest
    let content = if let Some(ref instructions) = manifest.instructions {
        if let Some(ref claude_file) = instructions.claude_code {
            let src_path = skill_path.join(claude_file);
            if src_path.exists() {
                fs::read_to_string(&src_path)?
            } else if let Some(ref core_file) = instructions.core {
                let core_path = skill_path.join(core_file);
                if core_path.exists() {
                    fs::read_to_string(&core_path)?
                } else {
                    generate_skill_md_from_manifest(manifest)
                }
            } else {
                generate_skill_md_from_manifest(manifest)
            }
        } else if let Some(ref core_file) = instructions.core {
            let core_path = skill_path.join(core_file);
            if core_path.exists() {
                fs::read_to_string(&core_path)?
            } else {
                generate_skill_md_from_manifest(manifest)
            }
        } else {
            generate_skill_md_from_manifest(manifest)
        }
    } else {
        generate_skill_md_from_manifest(manifest)
    };

    fs::write(&skill_md_path, &content)?;
    println!("    {} Claude: {}", "✓".green(), skill_md_path.display());

    Ok(())
}

/// Export a tap skill to Cursor
fn export_tap_skill_to_cursor(
    skill_path: &Path,
    manifest: &super::skill_validate::SkillManifest,
) -> Result<()> {
    if let Some(ref instructions) = manifest.instructions {
        if let Some(ref cursor_file) = instructions.cursor {
            let src_path = skill_path.join(cursor_file);
            if src_path.exists() {
                // Read and copy to .cursorrules in current project
                // or to a global location
                println!("    {} Cursor: {} (copy to project)", "✓".green(), cursor_file);
            } else {
                println!("    {} Cursor: file not found ({})", "⚠".yellow(), cursor_file);
            }
        }
    }
    Ok(())
}

/// Generate SKILL.md content from manifest
fn generate_skill_md_from_manifest(manifest: &super::skill_validate::SkillManifest) -> String {
    let mut md = String::new();

    // Frontmatter
    md.push_str("---\n");
    md.push_str(&format!("name: {}-fgp\n", manifest.name));
    md.push_str(&format!("description: {}\n", manifest.description));
    md.push_str("tools: [\"Bash\"]\n");
    if let Some(ref triggers) = manifest.triggers {
        if !triggers.keywords.is_empty() {
            md.push_str("triggers:\n");
            for kw in &triggers.keywords {
                md.push_str(&format!("  - \"{}\"\n", kw));
            }
        }
    }
    md.push_str("---\n\n");

    // Title
    md.push_str(&format!("# {} - FGP Skill\n\n", manifest.name));
    md.push_str(&format!("{}\n\n", manifest.description));

    // Daemons
    if !manifest.daemons.is_empty() {
        md.push_str("## Required Daemons\n\n");
        for daemon in &manifest.daemons {
            let optional = if daemon.optional { " (optional)" } else { "" };
            md.push_str(&format!("- `{}`{}\n", daemon.name, optional));
        }
        md.push_str("\n");
    }

    // Triggers
    if let Some(ref triggers) = manifest.triggers {
        md.push_str("## Trigger Detection\n\n");
        md.push_str("When user mentions:\n");
        for kw in &triggers.keywords {
            md.push_str(&format!("- \"{}\"\n", kw));
        }
        md.push_str("\n");
    }

    // Workflows
    if !manifest.workflows.is_empty() {
        md.push_str("## Workflows\n\n");
        for (name, workflow) in &manifest.workflows {
            md.push_str(&format!("### {}\n", name));
            if let Some(ref desc) = workflow.description {
                md.push_str(&format!("{}\n", desc));
            }
            md.push_str(&format!("```bash\nfgp workflow run {} --file {}\n```\n\n", name, workflow.file));
        }
    }

    md
}

/// Update marketplaces (git pull)
pub fn marketplace_update() -> Result<()> {
    let mut marketplaces = load_known_marketplaces()?;

    if marketplaces.marketplaces.is_empty() {
        println!("{}", "No marketplaces configured.".yellow());
        println!();
        println!("Add a marketplace first:");
        println!("  fgp skill marketplace add https://github.com/fast-gateway-protocol/fgp");
        return Ok(());
    }

    println!("{}", "Updating marketplaces...".bold());
    println!();

    for (name, entry) in marketplaces.marketplaces.iter_mut() {
        print!("  {} ", name.cyan());

        if let Some(ref location) = entry.install_location {
            // Git pull
            let output = Command::new("git")
                .args(["pull", "--quiet"])
                .current_dir(location)
                .output()?;

            if output.status.success() {
                // Get new commit SHA
                let sha = Command::new("git")
                    .args(["rev-parse", "--short", "HEAD"])
                    .current_dir(location)
                    .output()
                    .ok()
                    .and_then(|o| {
                        if o.status.success() {
                            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default();

                entry.last_updated = Some(chrono::Utc::now().to_rfc3339());
                println!("{} ({})", "✓ updated".green(), sha.dimmed());
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("{} {}", "✗ failed:".red(), stderr.trim());
            }
        } else {
            println!("{}", "not cloned yet".yellow());
        }
    }

    save_known_marketplaces(&marketplaces)?;

    Ok(())
}

/// Add a marketplace
pub fn marketplace_add(url: &str) -> Result<()> {
    println!(
        "{} {}",
        "Adding marketplace:".bold(),
        url.cyan()
    );

    // Parse URL to get repo name
    let repo_name = url
        .trim_end_matches('/')
        .split('/')
        .last()
        .unwrap_or("marketplace")
        .trim_end_matches(".git");

    // Check if already exists
    let mut marketplaces = load_known_marketplaces()?;
    if marketplaces.marketplaces.contains_key(repo_name) {
        println!(
            "{}",
            format!("Marketplace '{}' already exists.", repo_name).yellow()
        );
        return Ok(());
    }

    // Clone the repository
    let install_location = marketplaces_dir().join(repo_name);
    fs::create_dir_all(&install_location.parent().unwrap())?;

    println!("  Cloning repository...");
    let status = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(&install_location)
        .status()?;

    if !status.success() {
        bail!("Failed to clone repository");
    }

    // Extract owner/repo from URL
    let repo = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .split("github.com/")
        .last()
        .unwrap_or(url)
        .to_string();

    // Add to known marketplaces
    marketplaces.marketplaces.insert(
        repo_name.to_string(),
        MarketplaceEntry {
            source: MarketplaceSource {
                source_type: "github".to_string(),
                repo,
            },
            install_location: Some(install_location.to_string_lossy().to_string()),
            last_updated: Some(chrono::Utc::now().to_rfc3339()),
        },
    );

    save_known_marketplaces(&marketplaces)?;

    println!();
    println!(
        "{} Marketplace '{}' added successfully!",
        "✓".green().bold(),
        repo_name.cyan()
    );

    // Show available skills
    let manifest_path = install_location.join(".fgp").join("marketplace.json");
    if manifest_path.exists() {
        let content = fs::read_to_string(&manifest_path)?;
        let manifest: MarketplaceManifest = serde_json::from_str(&content)?;

        println!();
        println!("Available skills:");
        for skill in &manifest.skills {
            println!(
                "  {} - {}",
                skill.name.cyan(),
                skill.description.dimmed()
            );
        }
    }

    Ok(())
}

/// List marketplaces
pub fn marketplace_list() -> Result<()> {
    let marketplaces = load_known_marketplaces()?;

    if marketplaces.marketplaces.is_empty() {
        println!("{}", "No marketplaces configured.".yellow());
        println!();
        println!("Add a marketplace:");
        println!("  fgp skill marketplace add https://github.com/fast-gateway-protocol/fgp");
        return Ok(());
    }

    println!("{}", "FGP Skill Marketplaces".bold());
    println!();

    for (name, entry) in &marketplaces.marketplaces {
        let status = if entry.install_location.is_some() {
            "● cloned".green()
        } else {
            "○ not cloned".dimmed()
        };

        println!(
            "  {} {}",
            name.cyan().bold(),
            status
        );
        println!("    Source: {}", entry.source.repo.dimmed());
        if let Some(ref updated) = entry.last_updated {
            println!("    Last updated: {}", updated.dimmed());
        }
        println!();
    }

    Ok(())
}

/// Check for skill updates
pub fn check_updates() -> Result<()> {
    println!("{}", "Checking for skill updates...".bold());
    println!();

    let installed = load_installed_skills()?;
    let marketplaces = load_known_marketplaces()?;

    let mut updates_available = false;

    for (skill_key, entries) in &installed.skills {
        let parts: Vec<&str> = skill_key.split('@').collect();
        if parts.len() != 2 {
            continue;
        }
        let skill_name = parts[0];
        let marketplace_name = parts[1];

        // Find marketplace
        if let Some(mp_entry) = marketplaces.marketplaces.get(marketplace_name) {
            if let Some(ref location) = mp_entry.install_location {
                let manifest_path = Path::new(location).join(".fgp").join("marketplace.json");
                if manifest_path.exists() {
                    let content = fs::read_to_string(&manifest_path)?;
                    let manifest: MarketplaceManifest = serde_json::from_str(&content)?;

                    for skill in &manifest.skills {
                        if skill.name == skill_name {
                            if let Some(entry) = entries.first() {
                                // Compare git SHA if available
                                let current_sha = entry.git_commit_sha.as_deref().unwrap_or("");

                                // Get latest SHA
                                let source_path = Path::new(location).join(&skill.source);
                                let latest_sha = Command::new("git")
                                    .args(["rev-parse", "HEAD"])
                                    .current_dir(&source_path)
                                    .output()
                                    .ok()
                                    .and_then(|o| {
                                        if o.status.success() {
                                            Some(
                                                String::from_utf8_lossy(&o.stdout)
                                                    .trim()
                                                    .to_string(),
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_default();

                                if current_sha != latest_sha && !latest_sha.is_empty() {
                                    updates_available = true;
                                    println!(
                                        "  {} {} → {}",
                                        skill_name.cyan(),
                                        format!("({})", &current_sha[..7.min(current_sha.len())])
                                            .dimmed(),
                                        format!("({})", &latest_sha[..7.min(latest_sha.len())])
                                            .green()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !updates_available {
        println!("{}", "All skills are up to date.".green());
    } else {
        println!();
        println!("Run 'fgp skill upgrade' to update all skills.");
    }

    Ok(())
}

/// Upgrade all skills
pub fn upgrade(skill_name: Option<&str>) -> Result<()> {
    let installed = load_installed_skills()?;

    if installed.skills.is_empty() {
        println!("{}", "No skills installed.".yellow());
        return Ok(());
    }

    let skills_to_upgrade: Vec<_> = if let Some(name) = skill_name {
        installed
            .skills
            .keys()
            .filter(|k| k.starts_with(&format!("{}@", name)))
            .cloned()
            .collect()
    } else {
        installed.skills.keys().cloned().collect()
    };

    if skills_to_upgrade.is_empty() {
        println!(
            "{}",
            format!("Skill '{}' not found.", skill_name.unwrap_or("")).yellow()
        );
        return Ok(());
    }

    println!("{}", "Upgrading skills...".bold());
    println!();

    for skill_key in skills_to_upgrade {
        let parts: Vec<&str> = skill_key.split('@').collect();
        if parts.len() != 2 {
            continue;
        }
        let skill_name = parts[0];
        let marketplace_name = parts[1];

        print!("  {} ", skill_name.cyan());

        // Re-install the skill
        match install(skill_name, Some(marketplace_name)) {
            Ok(()) => println!("{}", "✓ upgraded".green()),
            Err(e) => println!("{} {}", "✗ failed:".red(), e),
        }
    }

    Ok(())
}

/// Remove a skill
pub fn remove(name: &str) -> Result<()> {
    let mut installed = load_installed_skills()?;

    // Find the skill key
    let skill_key = installed
        .skills
        .keys()
        .find(|k| k.starts_with(&format!("{}@", name)))
        .cloned();

    match skill_key {
        Some(key) => {
            if let Some(entries) = installed.skills.remove(&key) {
                // Remove cache directory
                if let Some(entry) = entries.first() {
                    let cache_path = Path::new(&entry.install_path);
                    if cache_path.exists() {
                        fs::remove_dir_all(cache_path)?;
                    }
                }
            }

            save_installed_skills(&installed)?;

            println!(
                "{} Skill '{}' removed successfully.",
                "✓".green().bold(),
                name.cyan()
            );
        }
        None => {
            println!(
                "{}",
                format!("Skill '{}' not found.", name).yellow()
            );
        }
    }

    Ok(())
}

/// Show skill info
pub fn info(name: &str) -> Result<()> {
    let installed = load_installed_skills()?;
    let marketplaces = load_known_marketplaces()?;

    // First check installed skills
    for (skill_key, entries) in &installed.skills {
        if skill_key.starts_with(&format!("{}@", name)) {
            if let Some(entry) = entries.first() {
                let parts: Vec<&str> = skill_key.split('@').collect();
                let marketplace_name = parts.get(1).unwrap_or(&"unknown");

                println!("{}", name.cyan().bold());
                println!();
                println!("  Installed: {}", "yes".green());
                println!("  Version:   {}", entry.version);
                println!("  Scope:     {}", entry.scope);
                println!("  From:      {}", marketplace_name);
                println!("  Path:      {}", entry.install_path.dimmed());
                if let Some(ref sha) = entry.git_commit_sha {
                    println!("  Git SHA:   {}", sha.dimmed());
                }
                if let Some(ref bin) = entry.binary_path {
                    println!("  Binary:    {}", bin.dimmed());
                }
                println!("  Installed: {}", entry.installed_at.dimmed());
                println!("  Updated:   {}", entry.last_updated.dimmed());

                // Try to load skill manifest for more info
                let manifest_path = Path::new(&entry.install_path)
                    .join("source")
                    .join(".fgp")
                    .join("skill.json");
                if manifest_path.exists() {
                    let content = fs::read_to_string(&manifest_path)?;
                    let manifest: SkillManifest = serde_json::from_str(&content)?;
                    println!();
                    println!("  Description:");
                    println!("    {}", manifest.description.dimmed());
                    println!();
                    println!("  Methods: {}", manifest.methods.len());
                    for method in &manifest.methods {
                        println!("    - {}", method.name);
                    }
                }

                return Ok(());
            }
        }
    }

    // Check marketplaces for uninstalled skills
    for (mp_name, entry) in &marketplaces.marketplaces {
        if let Some(ref location) = entry.install_location {
            let manifest_path = Path::new(location).join(".fgp").join("marketplace.json");
            if manifest_path.exists() {
                let content = fs::read_to_string(&manifest_path)?;
                let manifest: MarketplaceManifest = serde_json::from_str(&content)?;

                for skill in &manifest.skills {
                    if skill.name == name {
                        println!("{}", name.cyan().bold());
                        println!();
                        println!("  Installed: {}", "no".yellow());
                        println!("  Version:   {}", skill.version);
                        println!("  From:      {}", mp_name);
                        println!();
                        println!("  Description:");
                        println!("    {}", skill.description.dimmed());
                        if !skill.tags.is_empty() {
                            println!();
                            println!("  Tags: {}", skill.tags.join(", ").dimmed());
                        }
                        println!();
                        println!("Install with:");
                        println!("  fgp skill install {}", name);
                        return Ok(());
                    }
                }
            }
        }
    }

    println!(
        "{}",
        format!("Skill '{}' not found.", name).yellow()
    );

    Ok(())
}

// ============================================================================
// MCP Bridge Registration
// ============================================================================

/// FGP daemon manifest format (for MCP server compatibility)
#[derive(Debug, Serialize, Deserialize)]
struct DaemonManifest {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_protocol")]
    protocol: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    daemon: DaemonManifestConfig,
    #[serde(default)]
    methods: Vec<DaemonManifestMethod>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    auth: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    platforms: Vec<String>,
}

fn default_protocol() -> String {
    "fgp@1".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonManifestConfig {
    entrypoint: String,
    socket: String,
    #[serde(default)]
    dependencies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonManifestMethod {
    name: String,
    description: String,
    #[serde(default)]
    params: Vec<DaemonManifestParam>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonManifestParam {
    name: String,
    #[serde(rename = "type")]
    param_type: String,
    required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<serde_json::Value>,
}

/// Convert skill.json to manifest.json format for MCP server
fn skill_to_daemon_manifest(skill: &SkillManifest, binary_path: &str) -> DaemonManifest {
    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    let methods: Vec<DaemonManifestMethod> = skill
        .methods
        .iter()
        .map(|m| {
            let params: Vec<DaemonManifestParam> = m
                .params
                .iter()
                .map(|(name, def)| DaemonManifestParam {
                    name: name.clone(),
                    param_type: def.param_type.clone(),
                    required: def.required,
                    default: None,
                })
                .collect();

            DaemonManifestMethod {
                name: m.name.clone(),
                description: m.description.clone().unwrap_or_default(),
                params,
            }
        })
        .collect();

    DaemonManifest {
        name: daemon_name.clone(),
        version: skill.version.clone(),
        description: skill.description.clone(),
        protocol: "fgp@1".to_string(),
        author: skill.author.name.clone(),
        license: skill.license.clone(),
        repository: skill.repository.clone(),
        daemon: DaemonManifestConfig {
            entrypoint: binary_path.to_string(),
            socket: format!("{}/daemon.sock", daemon_name),
            dependencies: vec![],
        },
        methods,
        auth: None,
        platforms: vec!["darwin".to_string(), "linux".to_string()],
    }
}

/// Register an installed skill with the MCP server by creating manifest.json
pub fn mcp_register(name: &str) -> Result<()> {
    let installed = load_installed_skills()?;

    // Find the installed skill
    let skill_key = installed
        .skills
        .keys()
        .find(|k| k.starts_with(&format!("{}@", name)))
        .cloned();

    let (_key, entry) = match skill_key {
        Some(k) => {
            let entries = installed.skills.get(&k).unwrap();
            let entry = entries.first().context("No installation entry found")?;
            (k, entry)
        }
        None => {
            bail!("Skill '{}' is not installed. Install it first with: fgp skill install {}", name, name);
        }
    };

    // Load skill.json
    let skill_manifest_path = Path::new(&entry.install_path)
        .join("source")
        .join(".fgp")
        .join("skill.json");

    if !skill_manifest_path.exists() {
        bail!("Skill manifest not found at {}", skill_manifest_path.display());
    }

    let skill_content = fs::read_to_string(&skill_manifest_path)?;
    let skill_manifest: SkillManifest = serde_json::from_str(&skill_content)?;

    // Get the daemon name
    let daemon_name = skill_manifest
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill_manifest.name.replace("-gateway", ""));

    // Get binary path
    let binary_path = entry
        .binary_path
        .as_ref()
        .context("No binary path found. Was the skill built correctly?")?;

    // Create manifest.json for MCP server
    let daemon_manifest = skill_to_daemon_manifest(&skill_manifest, binary_path);

    // Write to services directory
    let services_dir = fgp_home().join("services").join(&daemon_name);
    fs::create_dir_all(&services_dir)?;

    let manifest_path = services_dir.join("manifest.json");
    let manifest_content = serde_json::to_string_pretty(&daemon_manifest)?;
    fs::write(&manifest_path, &manifest_content)?;

    println!(
        "{} Registered '{}' with MCP server",
        "✓".green().bold(),
        daemon_name.cyan()
    );
    println!("  Manifest: {}", manifest_path.display());
    println!();
    println!("The skill is now available via the FGP MCP server.");
    println!("Tools will be named: {}", format!("fgp_{}_*", daemon_name).cyan());

    Ok(())
}

/// Register all installed skills with MCP server
pub fn mcp_register_all() -> Result<()> {
    let installed = load_installed_skills()?;

    if installed.skills.is_empty() {
        println!("{}", "No skills installed.".yellow());
        return Ok(());
    }

    println!("{}", "Registering all skills with MCP server...".bold());
    println!();

    for skill_key in installed.skills.keys() {
        let parts: Vec<&str> = skill_key.split('@').collect();
        if parts.is_empty() {
            continue;
        }
        let skill_name = parts[0];

        print!("  {} ", skill_name.cyan());
        match mcp_register(skill_name) {
            Ok(()) => {} // Already prints success
            Err(e) => println!("{} {}", "✗ failed:".red(), e),
        }
    }

    Ok(())
}

/// List MCP-registered skills
pub fn mcp_list() -> Result<()> {
    let services_dir = fgp_home().join("services");

    if !services_dir.exists() {
        println!("{}", "No services directory found.".yellow());
        return Ok(());
    }

    println!("{}", "MCP-Registered FGP Skills".bold());
    println!();

    let mut found = false;
    for entry in fs::read_dir(&services_dir)? {
        let entry = entry?;
        let manifest_path = entry.path().join("manifest.json");

        if manifest_path.exists() {
            found = true;
            let content = fs::read_to_string(&manifest_path)?;
            let manifest: DaemonManifest = serde_json::from_str(&content)?;

            let socket_path = services_dir.join(&manifest.daemon.socket);
            let is_running = socket_path.exists();

            let status = if is_running {
                "● running".green()
            } else {
                "○ stopped".dimmed()
            };

            println!(
                "  {} {} {}",
                manifest.name.cyan().bold(),
                format!("v{}", manifest.version).dimmed(),
                status
            );
            println!("    {}", manifest.description.dimmed());
            println!("    Methods: {} | Tools: fgp_{}_*", manifest.methods.len(), manifest.name);
            println!();
        }
    }

    if !found {
        println!("{}", "No skills registered with MCP server.".yellow());
        println!();
        println!("Register an installed skill with:");
        println!("  fgp skill mcp-register <skill-name>");
    }

    Ok(())
}

// ============================================================================
// Multi-Ecosystem Export Functions
// ============================================================================

/// Export a skill to multiple ecosystems
pub fn export_skill(name: &str, targets: &[ExportTarget], binary_path: Option<&str>) -> Result<()> {
    let installed = load_installed_skills()?;

    // Find the installed skill
    let skill_key = installed
        .skills
        .keys()
        .find(|k| k.starts_with(&format!("{}@", name)))
        .cloned();

    let entry = match skill_key {
        Some(k) => {
            let entries = installed.skills.get(&k).unwrap();
            entries.first().context("No installation entry found")?
        }
        None => {
            bail!("Skill '{}' is not installed. Install it first with: fgp skill install {}", name, name);
        }
    };

    // Load skill.json
    let skill_manifest_path = Path::new(&entry.install_path)
        .join("source")
        .join(".fgp")
        .join("skill.json");

    if !skill_manifest_path.exists() {
        bail!("Skill manifest not found at {}", skill_manifest_path.display());
    }

    let skill_content = fs::read_to_string(&skill_manifest_path)?;
    let skill: SkillManifest = serde_json::from_str(&skill_content)?;

    let bin_path = binary_path.map(|s| s.to_string()).or(entry.binary_path.clone());

    // Expand 'All' target
    let actual_targets: Vec<ExportTarget> = if targets.contains(&ExportTarget::All) {
        ExportTarget::all_targets()
    } else {
        targets.to_vec()
    };

    for target in actual_targets {
        match target {
            ExportTarget::Mcp => {
                if let Some(ref bp) = bin_path {
                    export_to_mcp(&skill, bp)?;
                }
            }
            ExportTarget::Claude => export_to_claude(&skill)?,
            ExportTarget::Cursor => export_to_cursor(&skill)?,
            ExportTarget::ContinueDev => export_to_continue(&skill)?,
            ExportTarget::Windsurf => export_to_windsurf(&skill)?,
            ExportTarget::All => {} // Already expanded
        }
    }

    Ok(())
}

/// Export to MCP (FGP daemon manifest)
fn export_to_mcp(skill: &SkillManifest, binary_path: &str) -> Result<()> {
    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    let manifest = skill_to_daemon_manifest(skill, binary_path);
    let services_dir = fgp_home().join("services").join(&daemon_name);
    fs::create_dir_all(&services_dir)?;
    let manifest_path = services_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, &manifest_json)?;

    println!("  {} MCP: {}", "✓".green(), manifest_path.display());
    Ok(())
}

/// Export to Claude Code (SKILL.md)
fn export_to_claude(skill: &SkillManifest) -> Result<()> {
    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Get Claude-specific config or use defaults
    let (skill_name, triggers, tools) = if let Some(ref exports) = skill.exports {
        if let Some(ref claude) = exports.claude {
            if !claude.enabled {
                println!("  {} Claude: disabled in skill.json", "○".dimmed());
                return Ok(());
            }
            (
                claude.skill_name.clone().unwrap_or_else(|| format!("{}-fgp", daemon_name)),
                claude.triggers.clone(),
                claude.tools.clone(),
            )
        } else {
            (format!("{}-fgp", daemon_name), vec![], vec!["Bash".to_string()])
        }
    } else {
        (format!("{}-fgp", daemon_name), vec![], vec!["Bash".to_string()])
    };

    // Generate SKILL.md content
    let skill_md = generate_claude_skill_md(skill, &skill_name, &triggers, &tools);

    // Write to ~/.claude/skills/<skill_name>/SKILL.md
    let claude_skills_dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".claude")
        .join("skills")
        .join(&skill_name);

    fs::create_dir_all(&claude_skills_dir)?;
    let skill_md_path = claude_skills_dir.join("SKILL.md");
    fs::write(&skill_md_path, &skill_md)?;

    println!("  {} Claude: {}", "✓".green(), skill_md_path.display());
    Ok(())
}

/// Generate Claude Code SKILL.md content
fn generate_claude_skill_md(skill: &SkillManifest, skill_name: &str, triggers: &[String], tools: &[String]) -> String {
    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Build triggers from keywords if not specified
    let trigger_list = if triggers.is_empty() {
        skill.keywords.clone()
    } else {
        triggers.to_vec()
    };

    let tools_json = serde_json::to_string(&tools).unwrap_or_else(|_| "[\"Bash\"]".to_string());

    let mut md = String::new();

    // Frontmatter
    md.push_str("---\n");
    md.push_str(&format!("name: {}\n", skill_name));
    md.push_str(&format!("description: {}\n", skill.description));
    md.push_str(&format!("tools: {}\n", tools_json));
    if !trigger_list.is_empty() {
        md.push_str("triggers:\n");
        for trigger in &trigger_list {
            md.push_str(&format!("  - \"{}\"\n", trigger));
        }
    }
    md.push_str("---\n\n");

    // Title
    md.push_str(&format!("# {} FGP Skill\n\n", daemon_name.to_uppercase()));
    md.push_str(&format!("{}\n\n", skill.description));

    // Prerequisites
    md.push_str("## Prerequisites\n\n");
    md.push_str(&format!("1. **FGP daemon running**: `fgp start {}` or daemon auto-starts on first call\n", daemon_name));

    if !skill.requirements.is_empty() {
        for (name, req) in &skill.requirements {
            if let Some(ref hint) = req.install_hint {
                md.push_str(&format!("2. **{}**: {}\n", name, hint));
            }
        }
    }
    md.push_str("\n");

    // Available Methods
    md.push_str("## Available Methods\n\n");
    md.push_str("| Method | Description |\n");
    md.push_str("|--------|-------------|\n");
    for method in &skill.methods {
        let desc = method.description.as_deref().unwrap_or("");
        md.push_str(&format!("| `{}` | {} |\n", method.name, desc));
    }
    md.push_str("\n---\n\n");

    // Method details
    for method in &skill.methods {
        let desc = method.description.as_deref().unwrap_or("");
        md.push_str(&format!("### {} - {}\n\n", method.name, desc));

        // Parameters table
        if !method.params.is_empty() {
            md.push_str("**Parameters:**\n");
            md.push_str("| Parameter | Type | Required | Description |\n");
            md.push_str("|-----------|------|----------|-------------|\n");
            for (name, param) in &method.params {
                let param_desc = param.description.as_deref().unwrap_or("-");
                md.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    name,
                    param.param_type,
                    if param.required { "Yes" } else { "No" },
                    param_desc
                ));
            }
            md.push_str("\n");
        }

        // Example command
        md.push_str("```bash\n");
        if method.params.is_empty() {
            md.push_str(&format!("fgp call {}\n", method.name));
        } else {
            // Build example params
            let example_params: Vec<String> = method.params.iter()
                .filter(|(_, p)| p.required)
                .map(|(name, p)| {
                    let val = match p.param_type.as_str() {
                        "string" => format!("\"<{}>\"", name),
                        "integer" | "number" => "0".to_string(),
                        "boolean" => "true".to_string(),
                        _ => "null".to_string(),
                    };
                    format!("\"{}\": {}", name, val)
                })
                .collect();

            if example_params.is_empty() {
                md.push_str(&format!("fgp call {}\n", method.name));
            } else {
                md.push_str(&format!("fgp call {} -p '{{{}}}'\n", method.name, example_params.join(", ")));
            }
        }
        md.push_str("```\n\n---\n\n");
    }

    // Performance note
    md.push_str("## Performance\n\n");
    md.push_str("- Cold start: ~50ms\n");
    md.push_str("- Warm call: ~10-30ms (10x faster than MCP stdio)\n");

    md
}

/// Export to Cursor (mcp.json entry)
fn export_to_cursor(skill: &SkillManifest) -> Result<()> {
    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Get Cursor-specific config or use defaults
    let server_name = if let Some(ref exports) = skill.exports {
        if let Some(ref cursor) = exports.cursor {
            if !cursor.enabled {
                println!("  {} Cursor: disabled in skill.json", "○".dimmed());
                return Ok(());
            }
            cursor.server_name.clone().unwrap_or_else(|| format!("fgp-{}", daemon_name))
        } else {
            format!("fgp-{}", daemon_name)
        }
    } else {
        format!("fgp-{}", daemon_name)
    };

    // Read existing mcp.json or create new
    let cursor_dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".cursor");

    fs::create_dir_all(&cursor_dir)?;
    let mcp_json_path = cursor_dir.join("mcp.json");

    let mut mcp_config: serde_json::Value = if mcp_json_path.exists() {
        let content = fs::read_to_string(&mcp_json_path)?;
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({"mcpServers": {}}))
    } else {
        serde_json::json!({"mcpServers": {}})
    };

    // Add FGP server entry
    let server_entry = serde_json::json!({
        "command": "fgp",
        "args": ["mcp", "--service", &daemon_name],
        "env": {}
    });

    if let Some(servers) = mcp_config.get_mut("mcpServers") {
        if let Some(obj) = servers.as_object_mut() {
            obj.insert(server_name.clone(), server_entry);
        }
    }

    // Write back
    let mcp_json = serde_json::to_string_pretty(&mcp_config)?;
    fs::write(&mcp_json_path, &mcp_json)?;

    println!("  {} Cursor: {} in {}", "✓".green(), server_name, mcp_json_path.display());
    Ok(())
}

/// Export to Continue.dev (config.yaml provider)
fn export_to_continue(skill: &SkillManifest) -> Result<()> {
    // Check if enabled
    if let Some(ref exports) = skill.exports {
        if let Some(ref continue_cfg) = exports.continue_dev {
            if !continue_cfg.enabled {
                println!("  {} Continue: disabled in skill.json", "○".dimmed());
                return Ok(());
            }
        } else {
            println!("  {} Continue: not configured in skill.json", "○".dimmed());
            return Ok(());
        }
    } else {
        println!("  {} Continue: not configured in skill.json", "○".dimmed());
        return Ok(());
    }

    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Continue.dev doesn't have a stable format yet - log as TODO
    println!("  {} Continue: format TBD (daemon: {})", "⚠".yellow(), daemon_name);
    Ok(())
}

/// Export to Windsurf (markdown skill)
fn export_to_windsurf(skill: &SkillManifest) -> Result<()> {
    // Check if enabled
    if let Some(ref exports) = skill.exports {
        if let Some(ref windsurf) = exports.windsurf {
            if !windsurf.enabled {
                println!("  {} Windsurf: disabled in skill.json", "○".dimmed());
                return Ok(());
            }
        } else {
            println!("  {} Windsurf: not configured in skill.json", "○".dimmed());
            return Ok(());
        }
    } else {
        println!("  {} Windsurf: not configured in skill.json", "○".dimmed());
        return Ok(());
    }

    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    // Generate similar markdown to Claude (Windsurf format is similar)
    let skill_md = generate_claude_skill_md(skill, &format!("{}-fgp", daemon_name), &[], &["Bash".to_string()]);

    let windsurf_skills_dir = dirs::home_dir()
        .context("Could not find home directory")?
        .join(".windsurf")
        .join("skills")
        .join(&format!("{}-fgp", daemon_name));

    fs::create_dir_all(&windsurf_skills_dir)?;
    let skill_md_path = windsurf_skills_dir.join("SKILL.md");
    fs::write(&skill_md_path, &skill_md)?;

    println!("  {} Windsurf: {}", "✓".green(), skill_md_path.display());
    Ok(())
}

/// Register skill with multiple targets (CLI entry point)
pub fn register_with_targets(name: &str, target_str: &str) -> Result<()> {
    println!(
        "{} {} to {}...",
        "Registering".bold(),
        name.cyan(),
        target_str.green()
    );
    println!();

    // Parse targets
    let targets: Vec<ExportTarget> = target_str
        .split(',')
        .filter_map(|s| ExportTarget::from_str(s.trim()))
        .collect();

    if targets.is_empty() {
        bail!("No valid targets specified. Valid targets: mcp, claude, cursor, continue, windsurf, all");
    }

    export_skill(name, &targets, None)?;

    println!();
    println!("{} Registration complete!", "✓".green().bold());
    Ok(())
}

/// Show registration status for a skill
pub fn registration_status(name: &str) -> Result<()> {
    let installed = load_installed_skills()?;

    // Find the installed skill
    let skill_key = installed
        .skills
        .keys()
        .find(|k| k.starts_with(&format!("{}@", name)))
        .cloned();

    let entry = match skill_key {
        Some(k) => {
            let entries = installed.skills.get(&k).unwrap();
            entries.first().context("No installation entry found")?
        }
        None => {
            bail!("Skill '{}' is not installed", name);
        }
    };

    // Load skill.json
    let skill_manifest_path = Path::new(&entry.install_path)
        .join("source")
        .join(".fgp")
        .join("skill.json");

    let skill: SkillManifest = if skill_manifest_path.exists() {
        let content = fs::read_to_string(&skill_manifest_path)?;
        serde_json::from_str(&content)?
    } else {
        bail!("Skill manifest not found");
    };

    let daemon_name = skill
        .daemon
        .as_ref()
        .map(|d| d.name.clone())
        .unwrap_or_else(|| skill.name.replace("-gateway", ""));

    println!("{} v{}", name.cyan().bold(), skill.version);
    println!();

    // Check MCP
    let mcp_manifest = fgp_home().join("services").join(&daemon_name).join("manifest.json");
    if mcp_manifest.exists() {
        println!("  ├─ mcp:      {} {}", "✓".green(), mcp_manifest.display());
    } else {
        println!("  ├─ mcp:      {} not registered", "○".dimmed());
    }

    // Check Claude
    let claude_skill = dirs::home_dir()
        .unwrap()
        .join(".claude")
        .join("skills")
        .join(format!("{}-fgp", daemon_name))
        .join("SKILL.md");
    if claude_skill.exists() {
        println!("  ├─ claude:   {} {}", "✓".green(), claude_skill.display());
    } else {
        println!("  ├─ claude:   {} not registered", "○".dimmed());
    }

    // Check Cursor
    let cursor_mcp = dirs::home_dir().unwrap().join(".cursor").join("mcp.json");
    let cursor_registered = if cursor_mcp.exists() {
        let content = fs::read_to_string(&cursor_mcp).unwrap_or_default();
        content.contains(&format!("fgp-{}", daemon_name))
    } else {
        false
    };
    if cursor_registered {
        println!("  ├─ cursor:   {} fgp-{}", "✓".green(), daemon_name);
    } else {
        println!("  ├─ cursor:   {} not registered", "○".dimmed());
    }

    // Check Continue
    println!("  ├─ continue: {} not supported yet", "○".dimmed());

    // Check Windsurf
    let windsurf_skill = dirs::home_dir()
        .unwrap()
        .join(".windsurf")
        .join("skills")
        .join(format!("{}-fgp", daemon_name))
        .join("SKILL.md");
    if windsurf_skill.exists() {
        println!("  └─ windsurf: {} {}", "✓".green(), windsurf_skill.display());
    } else {
        println!("  └─ windsurf: {} not registered", "○".dimmed());
    }

    Ok(())
}
