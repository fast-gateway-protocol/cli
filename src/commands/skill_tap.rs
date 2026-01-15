//! FGP skill tap management - GitHub-based skill repositories.
//!
//! Taps are Git repositories containing skill.yaml packages.
//! Similar to Homebrew taps, they enable community distribution.
//!
//! # Directory Structure
//!
//! ```text
//! ~/.fgp/
//! └── taps/
//!     ├── taps.json                    # Track configured taps
//!     └── repos/
//!         ├── fast-gateway-protocol/
//!         │   └── official-skills/     # Cloned tap repo
//!         │       ├── skills/
//!         │       │   ├── research-assistant/
//!         │       │   │   └── skill.yaml
//!         │       │   └── email-triage/
//!         │       │       └── skill.yaml
//!         │       └── tap.yaml          # Tap metadata
//!         └── user/
//!             └── my-skills/
//! ```

use anyhow::{bail, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::skill_validate::SkillManifest;

/// Tap configuration stored in taps.json
#[derive(Debug, Serialize, Deserialize)]
pub struct TapsConfig {
    pub version: u32,
    pub taps: HashMap<String, TapEntry>,
}

impl Default for TapsConfig {
    fn default() -> Self {
        Self {
            version: 1,
            taps: HashMap::new(),
        }
    }
}

/// Individual tap entry
#[derive(Debug, Serialize, Deserialize)]
pub struct TapEntry {
    /// GitHub owner/repo format
    pub repo: String,
    /// Full GitHub URL
    pub url: String,
    /// Local path to cloned repo
    pub path: String,
    /// When the tap was added
    pub added_at: String,
    /// Last update timestamp
    pub updated_at: Option<String>,
    /// Number of skills in this tap
    pub skill_count: usize,
}

/// Tap metadata (tap.yaml in the repo root)
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TapMetadata {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// Get the taps directory
fn taps_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".fgp")
        .join("taps")
}

/// Get the taps config file path
fn taps_config_path() -> PathBuf {
    taps_dir().join("taps.json")
}

/// Get the repos directory
fn repos_dir() -> PathBuf {
    taps_dir().join("repos")
}

/// Load taps configuration
fn load_taps_config() -> Result<TapsConfig> {
    let path = taps_config_path();
    if !path.exists() {
        return Ok(TapsConfig::default());
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content)?)
}

/// Save taps configuration
fn save_taps_config(config: &TapsConfig) -> Result<()> {
    let path = taps_config_path();
    fs::create_dir_all(path.parent().unwrap())?;
    let content = serde_json::to_string_pretty(config)?;
    fs::write(&path, content)?;
    Ok(())
}

/// Convert owner/repo to tap name
fn repo_to_tap_name(repo: &str) -> String {
    repo.replace('/', "-")
}

/// Add a new tap
pub fn add(repo: &str) -> Result<()> {
    // Parse repo format (owner/repo or full URL)
    let (owner, repo_name, url) = parse_repo_input(repo)?;
    let tap_name = format!("{}-{}", owner, repo_name);

    println!(
        "{} {}",
        "→".blue().bold(),
        format!("Adding tap {}...", tap_name.cyan())
    );

    // Check if already exists
    let mut config = load_taps_config()?;
    if config.taps.contains_key(&tap_name) {
        println!(
            "{} Tap '{}' already exists. Use 'fgp skill tap update' to refresh.",
            "⚠".yellow(),
            tap_name
        );
        return Ok(());
    }

    // Create directory structure
    let tap_path = repos_dir().join(&owner).join(&repo_name);
    fs::create_dir_all(tap_path.parent().unwrap())?;

    // Clone the repository
    println!("  Cloning {}...", url);
    let status = Command::new("git")
        .args(["clone", "--depth", "1", &url])
        .arg(&tap_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Failed to clone repository: {}", url);
    }

    // Count skills in the tap
    let skill_count = count_skills(&tap_path)?;

    // Add to config
    let now = chrono::Utc::now().to_rfc3339();
    config.taps.insert(
        tap_name.clone(),
        TapEntry {
            repo: format!("{}/{}", owner, repo_name),
            url: url.clone(),
            path: tap_path.to_string_lossy().to_string(),
            added_at: now.clone(),
            updated_at: Some(now),
            skill_count,
        },
    );

    save_taps_config(&config)?;

    println!(
        "{} Added tap '{}' with {} skill(s)",
        "✓".green().bold(),
        tap_name.cyan(),
        skill_count
    );

    // Show available skills
    if skill_count > 0 {
        println!();
        println!("{}:", "Available skills".bold());
        list_tap_skills(&tap_path, 5)?;
    }

    Ok(())
}

/// Remove a tap
pub fn remove(name: &str) -> Result<()> {
    let mut config = load_taps_config()?;

    // Find the tap (allow partial match)
    let tap_name = find_tap_name(&config, name)?;

    let entry = config.taps.get(&tap_name).unwrap();
    let tap_path = PathBuf::from(&entry.path);

    println!(
        "{} {}",
        "→".blue().bold(),
        format!("Removing tap {}...", tap_name.cyan())
    );

    // Remove the directory
    if tap_path.exists() {
        fs::remove_dir_all(&tap_path)?;

        // Clean up empty parent directories
        if let Some(parent) = tap_path.parent() {
            if parent.read_dir()?.next().is_none() {
                let _ = fs::remove_dir(parent);
            }
        }
    }

    // Remove from config
    config.taps.remove(&tap_name);
    save_taps_config(&config)?;

    println!("{} Removed tap '{}'", "✓".green().bold(), tap_name);

    Ok(())
}

/// List all configured taps
pub fn list() -> Result<()> {
    let config = load_taps_config()?;

    if config.taps.is_empty() {
        println!("{}", "No taps configured.".yellow());
        println!();
        println!("Add a tap with:");
        println!(
            "  {}",
            "fgp skill tap add fast-gateway-protocol/official-skills".cyan()
        );
        return Ok(());
    }

    println!("{}", "Configured Taps".bold());
    println!();

    for (name, entry) in &config.taps {
        let updated = entry
            .updated_at
            .as_ref()
            .map(|s| format_relative_time(s))
            .unwrap_or_else(|| "never".to_string());

        println!(
            "  {} {}",
            name.cyan().bold(),
            format!("({} skills)", entry.skill_count).dimmed()
        );
        println!("    {} {}", "repo:".dimmed(), entry.repo);
        println!("    {} {}", "updated:".dimmed(), updated);
    }

    Ok(())
}

/// Update all taps (git pull)
pub fn update() -> Result<()> {
    let mut config = load_taps_config()?;

    if config.taps.is_empty() {
        println!("{}", "No taps configured.".yellow());
        return Ok(());
    }

    println!("{}", "Updating taps...".bold());
    println!();

    for (name, entry) in config.taps.iter_mut() {
        let tap_path = PathBuf::from(&entry.path);

        if !tap_path.exists() {
            println!(
                "  {} {} (path missing, re-add with 'fgp skill tap add {}')",
                "✗".red(),
                name,
                entry.repo
            );
            continue;
        }

        print!("  {} {}... ", "→".blue(), name);

        let output = Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&tap_path)
            .output()?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("Already up to date") {
                println!("{}", "up to date".dimmed());
            } else {
                // Recount skills
                let skill_count = count_skills(&tap_path)?;
                entry.skill_count = skill_count;
                entry.updated_at = Some(chrono::Utc::now().to_rfc3339());
                println!("{} ({} skills)", "updated".green(), skill_count);
            }
        } else {
            println!("{}", "failed".red());
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.is_empty() {
                println!("    {}", stderr.trim().dimmed());
            }
        }
    }

    save_taps_config(&config)?;

    Ok(())
}

/// Show skills in a specific tap
pub fn show(name: &str) -> Result<()> {
    let config = load_taps_config()?;
    let tap_name = find_tap_name(&config, name)?;
    let entry = config.taps.get(&tap_name).unwrap();
    let tap_path = PathBuf::from(&entry.path);

    if !tap_path.exists() {
        bail!("Tap directory not found. Re-add with 'fgp skill tap add {}'", entry.repo);
    }

    println!("{} {}", "Tap:".bold(), tap_name.cyan());
    println!("  {} {}", "repo:".dimmed(), entry.repo);
    println!("  {} {}", "path:".dimmed(), entry.path);
    println!();
    println!("{}:", "Skills".bold());

    list_tap_skills(&tap_path, usize::MAX)?;

    Ok(())
}

// ============================================================================
// Helper functions
// ============================================================================

/// Parse repo input (owner/repo or full URL)
fn parse_repo_input(input: &str) -> Result<(String, String, String)> {
    let input = input.trim();

    // Handle full GitHub URL
    if input.starts_with("https://") || input.starts_with("git@") {
        let cleaned = input
            .trim_end_matches('/')
            .trim_end_matches(".git");

        // Extract owner/repo from URL
        let parts: Vec<&str> = if cleaned.contains("github.com/") {
            cleaned.split("github.com/").last().unwrap_or("").split('/').collect()
        } else if cleaned.contains("github.com:") {
            cleaned.split("github.com:").last().unwrap_or("").split('/').collect()
        } else {
            bail!("Could not parse GitHub URL: {}", input);
        };

        if parts.len() < 2 {
            bail!("Invalid GitHub URL format: {}", input);
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let url = format!("https://github.com/{}/{}.git", owner, repo);

        return Ok((owner, repo, url));
    }

    // Handle owner/repo format
    let parts: Vec<&str> = input.split('/').collect();
    if parts.len() != 2 {
        bail!(
            "Invalid tap format '{}'. Use 'owner/repo' format (e.g., 'fast-gateway-protocol/official-skills')",
            input
        );
    }

    let owner = parts[0].to_string();
    let repo = parts[1].to_string();
    let url = format!("https://github.com/{}/{}.git", owner, repo);

    Ok((owner, repo, url))
}

/// Find tap name with partial matching
fn find_tap_name(config: &TapsConfig, partial: &str) -> Result<String> {
    // Exact match first
    if config.taps.contains_key(partial) {
        return Ok(partial.to_string());
    }

    // Try with repo format conversion
    let normalized = repo_to_tap_name(partial);
    if config.taps.contains_key(&normalized) {
        return Ok(normalized);
    }

    // Partial match
    let matches: Vec<&String> = config
        .taps
        .keys()
        .filter(|k| k.contains(partial))
        .collect();

    match matches.len() {
        0 => bail!("Tap '{}' not found. Use 'fgp skill tap list' to see configured taps.", partial),
        1 => Ok(matches[0].clone()),
        _ => bail!(
            "Ambiguous tap name '{}'. Matches: {}",
            partial,
            matches.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Count skills in a tap directory
fn count_skills(tap_path: &Path) -> Result<usize> {
    let skills_dir = tap_path.join("skills");
    if !skills_dir.exists() {
        // Try root level skill.yaml files
        return count_skills_in_dir(tap_path);
    }
    count_skills_in_dir(&skills_dir)
}

/// Count skill.yaml files in a directory
fn count_skills_in_dir(dir: &Path) -> Result<usize> {
    let mut count = 0;

    if !dir.exists() {
        return Ok(0);
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let skill_yaml = path.join("skill.yaml");
            let skill_yml = path.join("skill.yml");
            if skill_yaml.exists() || skill_yml.exists() {
                count += 1;
            }
        }
    }

    Ok(count)
}

/// List skills in a tap directory
fn list_tap_skills(tap_path: &Path, limit: usize) -> Result<()> {
    let skills_dir = tap_path.join("skills");
    let search_dir = if skills_dir.exists() {
        skills_dir
    } else {
        tap_path.to_path_buf()
    };

    let mut skills = Vec::new();

    for entry in fs::read_dir(&search_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let skill_yaml = path.join("skill.yaml");
            let skill_yml = path.join("skill.yml");
            let manifest_path = if skill_yaml.exists() {
                skill_yaml
            } else if skill_yml.exists() {
                skill_yml
            } else {
                continue;
            };

            // Try to read the manifest
            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_yaml::from_str::<SkillManifest>(&content) {
                    skills.push((manifest.name.clone(), manifest.version.clone(), manifest.description.clone()));
                }
            }
        }
    }

    if skills.is_empty() {
        println!("  {}", "No skills found".dimmed());
        return Ok(());
    }

    for (i, (name, version, description)) in skills.iter().enumerate() {
        if i >= limit {
            let remaining = skills.len() - limit;
            println!("  {} more skill(s)...", format!("... and {}", remaining).dimmed());
            break;
        }
        println!(
            "  {} {}",
            name.cyan(),
            format!("v{}", version).dimmed()
        );
        println!("    {}", description.dimmed());
    }

    Ok(())
}

/// Format a timestamp as relative time
fn format_relative_time(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt);

        if duration.num_minutes() < 1 {
            "just now".to_string()
        } else if duration.num_hours() < 1 {
            format!("{} minutes ago", duration.num_minutes())
        } else if duration.num_days() < 1 {
            format!("{} hours ago", duration.num_hours())
        } else if duration.num_days() < 7 {
            format!("{} days ago", duration.num_days())
        } else {
            dt.format("%Y-%m-%d").to_string()
        }
    } else {
        timestamp.to_string()
    }
}

// ============================================================================
// Public functions for skill search/install integration
// ============================================================================

/// Search all taps for a skill by name
pub fn search_taps(query: &str) -> Result<Vec<(String, PathBuf, SkillManifest)>> {
    let config = load_taps_config()?;
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (tap_name, entry) in &config.taps {
        let tap_path = PathBuf::from(&entry.path);
        let skills_dir = tap_path.join("skills");
        let search_dir = if skills_dir.exists() {
            skills_dir
        } else {
            tap_path.clone()
        };

        if !search_dir.exists() {
            continue;
        }

        for dir_entry in fs::read_dir(&search_dir)? {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();

            if !path.is_dir() {
                continue;
            }

            let skill_yaml = path.join("skill.yaml");
            let skill_yml = path.join("skill.yml");
            let manifest_path = if skill_yaml.exists() {
                skill_yaml
            } else if skill_yml.exists() {
                skill_yml
            } else {
                continue;
            };

            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_yaml::from_str::<SkillManifest>(&content) {
                    // Match against name, description, or keywords
                    let matches = manifest.name.to_lowercase().contains(&query_lower)
                        || manifest.description.to_lowercase().contains(&query_lower)
                        || manifest.keywords.iter().any(|k| k.to_lowercase().contains(&query_lower));

                    if matches {
                        results.push((tap_name.clone(), path, manifest));
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Find a skill by exact name across all taps
pub fn find_skill(name: &str) -> Result<Option<(String, PathBuf, SkillManifest)>> {
    let config = load_taps_config()?;

    for (tap_name, entry) in &config.taps {
        let tap_path = PathBuf::from(&entry.path);
        let skills_dir = tap_path.join("skills");
        let search_dir = if skills_dir.exists() {
            skills_dir
        } else {
            tap_path.clone()
        };

        if !search_dir.exists() {
            continue;
        }

        // Direct path match
        let skill_path = search_dir.join(name);
        if skill_path.exists() {
            let skill_yaml = skill_path.join("skill.yaml");
            let skill_yml = skill_path.join("skill.yml");
            let manifest_path = if skill_yaml.exists() {
                skill_yaml
            } else if skill_yml.exists() {
                skill_yml
            } else {
                continue;
            };

            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_yaml::from_str::<SkillManifest>(&content) {
                    return Ok(Some((tap_name.clone(), skill_path, manifest)));
                }
            }
        }

        // Search all skills for name match
        for dir_entry in fs::read_dir(&search_dir)? {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();

            if !path.is_dir() {
                continue;
            }

            let skill_yaml = path.join("skill.yaml");
            let skill_yml = path.join("skill.yml");
            let manifest_path = if skill_yaml.exists() {
                skill_yaml
            } else if skill_yml.exists() {
                skill_yml
            } else {
                continue;
            };

            if let Ok(content) = fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = serde_yaml::from_str::<SkillManifest>(&content) {
                    if manifest.name == name {
                        return Ok(Some((tap_name.clone(), path, manifest)));
                    }
                }
            }
        }
    }

    Ok(None)
}
