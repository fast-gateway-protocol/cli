//! Validate FGP skill manifests (skill.yaml).
//!
//! This module validates the composed skill package format,
//! which bundles daemon dependencies, instructions, and triggers.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Skill manifest (skill.yaml) - the composed skill format.
#[derive(Debug, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Skill name (lowercase, alphanumeric with hyphens)
    pub name: String,
    /// Semantic version
    pub version: String,
    /// Brief description
    pub description: String,
    /// Author information
    pub author: Author,
    /// SPDX license identifier
    #[serde(default)]
    pub license: Option<String>,
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,
    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Daemon dependencies
    #[serde(default)]
    pub daemons: Vec<DaemonDependency>,
    /// Agent-specific instruction files
    #[serde(default)]
    pub instructions: Option<Instructions>,
    /// Trigger conditions
    #[serde(default)]
    pub triggers: Option<Triggers>,
    /// Named workflows
    #[serde(default)]
    pub workflows: HashMap<String, WorkflowRef>,
    /// User configuration options
    #[serde(default)]
    pub config: HashMap<String, ConfigOption>,
    /// Authentication requirements
    #[serde(default)]
    pub auth: Option<AuthConfig>,
    /// Permission declarations
    #[serde(default)]
    pub permissions: Option<Permissions>,
    /// Export configuration
    #[serde(default)]
    pub exports: Option<Exports>,
}

/// Author information - can be string or object.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Author {
    String(String),
    Object {
        name: String,
        #[serde(default)]
        email: Option<String>,
        #[serde(default)]
        url: Option<String>,
    },
}

/// Daemon dependency.
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonDependency {
    /// Daemon name (e.g., "browser", "gmail")
    pub name: String,
    /// Version requirement (e.g., ">=1.0.0")
    #[serde(default)]
    pub version: Option<String>,
    /// Whether this daemon is optional
    #[serde(default)]
    pub optional: bool,
    /// Specific methods used (for permissions)
    #[serde(default)]
    pub methods: Vec<String>,
}

/// Agent-specific instruction files.
#[derive(Debug, Serialize, Deserialize)]
pub struct Instructions {
    #[serde(default)]
    pub core: Option<String>,
    #[serde(rename = "claude-code", default)]
    pub claude_code: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub codex: Option<String>,
    #[serde(default)]
    pub windsurf: Option<String>,
    #[serde(default)]
    pub mcp: Option<String>,
}

/// Trigger conditions.
#[derive(Debug, Serialize, Deserialize)]
pub struct Triggers {
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
}

/// Workflow reference.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowRef {
    pub file: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: bool,
}

/// Configuration option.
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigOption {
    #[serde(rename = "type")]
    pub config_type: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub options: Vec<serde_yaml::Value>,
}

/// Authentication configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub daemons: HashMap<String, String>,
    #[serde(default)]
    pub secrets: Vec<SecretConfig>,
}

/// Secret configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
}

/// Permission declarations.
#[derive(Debug, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub daemons: HashMap<String, DaemonPermission>,
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub subprocess: bool,
    #[serde(default)]
    pub env_vars: Vec<String>,
}

/// Daemon permission - can be "all", "deny", or list of methods.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DaemonPermission {
    All(String),
    Methods(Vec<String>),
}

/// Export configuration.
#[derive(Debug, Serialize, Deserialize)]
pub struct Exports {
    #[serde(rename = "claude-code", default)]
    pub claude_code: Option<ClaudeExport>,
    #[serde(default)]
    pub cursor: Option<CursorExport>,
    #[serde(default)]
    pub mcp: Option<McpExport>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClaudeExport {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub skill_name: Option<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CursorExport {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub rules_file: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpExport {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub tools_prefix: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Validate a skill manifest.
pub fn validate(path: &str) -> Result<()> {
    println!("{} Validating skill manifest...", "→".blue().bold());

    let skill_path = Path::new(path);

    // Check if path exists
    if !skill_path.exists() {
        bail!("Path not found: {}", path);
    }

    // Find skill.yaml
    let manifest_path = if skill_path.is_dir() {
        skill_path.join("skill.yaml")
    } else {
        skill_path.to_path_buf()
    };

    if !manifest_path.exists() {
        // Also check for skill.yml
        let alt_path = if skill_path.is_dir() {
            skill_path.join("skill.yml")
        } else {
            skill_path.with_extension("yml")
        };

        if alt_path.exists() {
            return validate_manifest(&alt_path, skill_path);
        }

        bail!(
            "Skill manifest not found. Expected: {}\n\
             Create a skill.yaml file with name, version, description, and author.",
            manifest_path.display()
        );
    }

    validate_manifest(&manifest_path, skill_path)
}

fn validate_manifest(manifest_path: &Path, skill_dir: &Path) -> Result<()> {
    // Read and parse
    let content = fs::read_to_string(manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let skill: SkillManifest = serde_yaml::from_str(&content)
        .with_context(|| "Invalid YAML or schema mismatch")?;

    // Validation checks
    validate_name(&skill.name)?;
    validate_version(&skill.version)?;
    validate_description(&skill.description)?;

    let mut warnings = Vec::new();

    // Validate daemon dependencies
    validate_daemons(&skill.daemons)?;

    // Validate instruction files exist
    if let Some(ref instructions) = skill.instructions {
        validate_instructions(instructions, skill_dir, &mut warnings)?;
    }

    // Validate workflow files exist
    validate_workflows(&skill.workflows, skill_dir, &mut warnings)?;

    // Validate config options
    validate_config(&skill.config)?;

    // Validate auth config
    if let Some(ref auth) = skill.auth {
        validate_auth(auth)?;
    }

    // Success output
    println!("{} Skill manifest is valid!", "✓".green().bold());
    println!();

    // Print summary
    println!("{}:", "Skill Info".cyan().bold());
    println!("  Name:        {}", skill.name.white().bold());
    println!("  Version:     {}", skill.version);
    println!("  Description: {}", skill.description);
    println!(
        "  Author:      {}",
        match &skill.author {
            Author::String(s) => s.clone(),
            Author::Object { name, .. } => name.clone(),
        }
    );

    if !skill.daemons.is_empty() {
        println!();
        println!("{}:", "Daemon Dependencies".cyan().bold());
        for daemon in &skill.daemons {
            let optional = if daemon.optional { " (optional)" } else { "" };
            let version = daemon.version.as_deref().unwrap_or("any");
            println!("  - {}{} [{}]", daemon.name, optional, version);
        }
    }

    if !skill.workflows.is_empty() {
        println!();
        println!("{}:", "Workflows".cyan().bold());
        for (name, workflow) in &skill.workflows {
            let default = if workflow.default { " (default)" } else { "" };
            println!("  - {}{}", name, default);
        }
    }

    if let Some(ref triggers) = skill.triggers {
        let total = triggers.keywords.len() + triggers.patterns.len() + triggers.commands.len();
        if total > 0 {
            println!();
            println!("{}:", "Triggers".cyan().bold());
            println!(
                "  {} keywords, {} patterns, {} commands",
                triggers.keywords.len(),
                triggers.patterns.len(),
                triggers.commands.len()
            );
        }
    }

    if let Some(ref exports) = skill.exports {
        println!();
        println!("{}:", "Export Targets".cyan().bold());
        if exports.claude_code.as_ref().map(|e| e.enabled).unwrap_or(false) {
            println!("  - Claude Code");
        }
        if exports.cursor.as_ref().map(|e| e.enabled).unwrap_or(false) {
            println!("  - Cursor");
        }
        if exports.mcp.as_ref().map(|e| e.enabled).unwrap_or(false) {
            println!("  - MCP");
        }
    }

    // Print warnings
    if !warnings.is_empty() {
        println!();
        println!("{}:", "Warnings".yellow().bold());
        for warning in warnings {
            println!("  {} {}", "⚠".yellow(), warning);
        }
    }

    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.len() < 2 {
        bail!("Skill name must be at least 2 characters");
    }
    if name.len() > 64 {
        bail!("Skill name must be at most 64 characters");
    }
    if !name.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        bail!("Skill name must start with a lowercase letter");
    }
    if !name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        bail!("Skill name must contain only lowercase letters, numbers, and hyphens");
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<()> {
    // Simple semver check
    let parts: Vec<&str> = version.split('-').next().unwrap_or(version).split('.').collect();
    if parts.len() != 3 {
        bail!("Version must be semver format (e.g., 1.0.0)");
    }
    for part in parts {
        if part.parse::<u32>().is_err() {
            bail!("Version components must be numbers");
        }
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<()> {
    if description.len() < 10 {
        bail!("Description must be at least 10 characters");
    }
    if description.len() > 500 {
        bail!("Description must be at most 500 characters");
    }
    Ok(())
}

fn validate_daemons(daemons: &[DaemonDependency]) -> Result<()> {
    for daemon in daemons {
        if daemon.name.is_empty() {
            bail!("Daemon name cannot be empty");
        }
        // Known daemons (could be expanded or loaded from registry)
        let known_daemons = [
            "browser", "gmail", "calendar", "github", "imessage", "fly", "neon", "vercel", "slack", "travel",
        ];
        if !known_daemons.contains(&daemon.name.as_str()) {
            eprintln!(
                "  {} Unknown daemon '{}' - may not be available",
                "⚠".yellow(),
                daemon.name
            );
        }
    }
    Ok(())
}

fn validate_instructions(
    instructions: &Instructions,
    skill_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let mut check_file = |path: &Option<String>, name: &str| {
        if let Some(ref p) = path {
            let full_path = skill_dir.join(p);
            if !full_path.exists() {
                warnings.push(format!("{} instruction file not found: {}", name, p));
            }
        }
    };

    check_file(&instructions.core, "Core");
    check_file(&instructions.claude_code, "Claude Code");
    check_file(&instructions.cursor, "Cursor");
    check_file(&instructions.codex, "Codex");
    check_file(&instructions.windsurf, "Windsurf");
    check_file(&instructions.mcp, "MCP");

    Ok(())
}

fn validate_workflows(
    workflows: &HashMap<String, WorkflowRef>,
    skill_dir: &Path,
    warnings: &mut Vec<String>,
) -> Result<()> {
    for (name, workflow) in workflows {
        let workflow_path = skill_dir.join(&workflow.file);
        if !workflow_path.exists() {
            warnings.push(format!("Workflow '{}' file not found: {}", name, workflow.file));
        }
    }
    Ok(())
}

fn validate_config(config: &HashMap<String, ConfigOption>) -> Result<()> {
    let valid_types = ["string", "number", "boolean", "enum", "array"];
    for (name, opt) in config {
        if !valid_types.contains(&opt.config_type.as_str()) {
            bail!(
                "Invalid config type '{}' for '{}'. Valid types: {:?}",
                opt.config_type,
                name,
                valid_types
            );
        }
        if opt.config_type == "enum" && opt.options.is_empty() {
            bail!("Enum config '{}' must have options", name);
        }
    }
    Ok(())
}

fn validate_auth(auth: &AuthConfig) -> Result<()> {
    let valid_auth_values = ["required", "optional"];
    for (daemon, value) in &auth.daemons {
        if !valid_auth_values.contains(&value.as_str()) {
            bail!(
                "Invalid auth value '{}' for daemon '{}'. Use 'required' or 'optional'",
                value,
                daemon
            );
        }
    }

    for secret in &auth.secrets {
        // Validate secret name format (UPPER_SNAKE_CASE)
        if !secret.name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
            bail!(
                "Secret name '{}' must be UPPER_SNAKE_CASE",
                secret.name
            );
        }
    }

    Ok(())
}
