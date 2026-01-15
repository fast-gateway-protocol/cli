//! Export FGP skills to agent-specific formats.
//!
//! Supported targets:
//! - claude-code: Generates SKILL.md for ~/.claude/skills/
//! - cursor: Generates .cursorrules and commands
//! - codex: Generates tool spec and prompts
//! - mcp: Generates MCP tool schema
//! - windsurf: Generates cascade rules

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use super::skill_validate::SkillManifest;

/// Export a skill for a specific agent.
pub fn export(target: &str, skill: &str, output: Option<&str>) -> Result<()> {
    println!(
        "{} Exporting skill for {}...",
        "→".blue().bold(),
        target.cyan()
    );

    // Load the skill manifest
    let skill_path = Path::new(skill);
    let skill_dir = if skill_path.is_dir() {
        skill_path.to_path_buf()
    } else {
        skill_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    let manifest_path = if skill_path.is_dir() {
        skill_path.join("skill.yaml")
    } else if skill_path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
        skill_path.to_path_buf()
    } else {
        // Assume it's a skill name, look in installed skills
        let installed_path = shellexpand::tilde("~/.fgp/skills").to_string();
        Path::new(&installed_path).join(skill).join("skill.yaml")
    };

    if !manifest_path.exists() {
        bail!(
            "Skill manifest not found: {}\n\
             Provide a path to a skill directory or skill.yaml file.",
            manifest_path.display()
        );
    }

    let content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;

    let manifest: SkillManifest = serde_yaml::from_str(&content)
        .with_context(|| "Invalid skill.yaml")?;

    // Determine output directory
    let output_dir = match output {
        Some(dir) => Path::new(dir).to_path_buf(),
        None => std::env::current_dir()?,
    };

    // Export based on target
    match target {
        "claude-code" | "claude" => export_claude_code(&manifest, &skill_dir, &output_dir),
        "cursor" => export_cursor(&manifest, &skill_dir, &output_dir),
        "codex" => export_codex(&manifest, &skill_dir, &output_dir),
        "mcp" => export_mcp(&manifest, &skill_dir, &output_dir),
        "windsurf" => export_windsurf(&manifest, &skill_dir, &output_dir),
        _ => bail!(
            "Unknown export target: {}\n\
             Valid targets: claude-code, cursor, codex, mcp, windsurf",
            target
        ),
    }
}

/// Export for Claude Code (generates SKILL.md).
fn export_claude_code(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    // Create output directory
    let skill_output_dir = output_dir.join(&manifest.name);
    fs::create_dir_all(&skill_output_dir)?;

    // Build SKILL.md content
    let mut skill_md = String::new();

    // YAML front matter
    skill_md.push_str("---\n");
    skill_md.push_str(&format!("name: {}\n", manifest.name));
    skill_md.push_str(&format!("description: {}\n", manifest.description));
    skill_md.push_str(&format!("version: {}\n", manifest.version));

    // Add triggers
    if let Some(ref triggers) = manifest.triggers {
        if !triggers.keywords.is_empty() {
            skill_md.push_str("triggers:\n");
            for keyword in &triggers.keywords {
                skill_md.push_str(&format!("  - \"{}\"\n", keyword));
            }
        }
    }

    skill_md.push_str("---\n\n");

    // Read core/claude-code instructions if they exist
    let claude_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.claude_code.as_ref())
        .or_else(|| manifest.instructions.as_ref().and_then(|i| i.core.as_ref()));

    if let Some(instruction_path) = claude_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            skill_md.push_str(&instructions);
        }
    } else {
        // Generate default instructions
        skill_md.push_str(&format!("# {}\n\n", manifest.name));
        skill_md.push_str(&format!("{}\n\n", manifest.description));

        // Add daemon usage
        if !manifest.daemons.is_empty() {
            skill_md.push_str("## Dependencies\n\n");
            skill_md.push_str("This skill requires the following FGP daemons:\n\n");
            for daemon in &manifest.daemons {
                let optional = if daemon.optional { " (optional)" } else { "" };
                skill_md.push_str(&format!("- **{}**{}\n", daemon.name, optional));
            }
            skill_md.push_str("\n");
        }

        // Add usage examples
        if let Some(ref triggers) = manifest.triggers {
            if !triggers.patterns.is_empty() {
                skill_md.push_str("## Usage\n\n");
                for pattern in &triggers.patterns {
                    skill_md.push_str(&format!("- `{}`\n", pattern));
                }
                skill_md.push_str("\n");
            }
        }

        // Add workflow info
        if !manifest.workflows.is_empty() {
            skill_md.push_str("## Workflows\n\n");
            for (name, workflow) in &manifest.workflows {
                let default = if workflow.default { " (default)" } else { "" };
                let desc = workflow.description.as_deref().unwrap_or("");
                skill_md.push_str(&format!("- **{}**{}: {}\n", name, default, desc));
            }
            skill_md.push_str("\n");
        }
    }

    // Write SKILL.md
    let skill_md_path = skill_output_dir.join("SKILL.md");
    fs::write(&skill_md_path, &skill_md)?;

    println!(
        "{} Exported Claude Code skill to: {}",
        "✓".green().bold(),
        skill_md_path.display()
    );

    // Provide install hint
    println!();
    println!("{}:", "Install".cyan().bold());
    println!(
        "  cp -r {} ~/.claude/skills/",
        skill_output_dir.display()
    );

    Ok(())
}

/// Export for Cursor (generates .cursorrules).
fn export_cursor(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut rules = String::new();

    rules.push_str(&format!("# {} - FGP Skill\n\n", manifest.name));
    rules.push_str(&format!("{}\n\n", manifest.description));

    // Add trigger detection
    if let Some(ref triggers) = manifest.triggers {
        rules.push_str("## Trigger Detection\n\n");
        rules.push_str("When user mentions:\n");
        for keyword in &triggers.keywords {
            rules.push_str(&format!("- \"{}\"\n", keyword));
        }
        rules.push_str("\n");
    }

    // Read cursor-specific instructions
    let cursor_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.cursor.as_ref());

    if let Some(instruction_path) = cursor_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            rules.push_str(&instructions);
        }
    } else {
        // Generate default
        rules.push_str("## Execution\n\n");
        rules.push_str("Use FGP daemons for fast execution:\n\n");
        rules.push_str("```bash\n");
        for daemon in &manifest.daemons {
            for method in &daemon.methods {
                rules.push_str(&format!(
                    "fgp call {}.{} -p '{{\"param\": \"value\"}}'\n",
                    daemon.name, method
                ));
            }
        }
        rules.push_str("```\n");
    }

    // Write file
    let rules_path = output_dir.join(format!("{}.cursorrules", manifest.name));
    fs::write(&rules_path, &rules)?;

    println!(
        "{} Exported Cursor rules to: {}",
        "✓".green().bold(),
        rules_path.display()
    );

    Ok(())
}

/// Export for Codex (generates tool spec).
fn export_codex(manifest: &SkillManifest, _skill_dir: &Path, output_dir: &Path) -> Result<()> {
    // Generate a simple tool specification for Codex
    let mut spec = serde_json::json!({
        "name": manifest.name,
        "description": manifest.description,
        "version": manifest.version,
        "tools": []
    });

    // Add tools from daemon methods
    let tools = spec["tools"].as_array_mut().unwrap();
    for daemon in &manifest.daemons {
        for method in &daemon.methods {
            tools.push(serde_json::json!({
                "name": format!("{}.{}", daemon.name, method),
                "description": format!("{} {} operation", daemon.name, method),
                "invocation": format!("fgp call {}.{} -p '{{...}}'", daemon.name, method)
            }));
        }
    }

    // Write file
    let spec_path = output_dir.join(format!("{}.codex.json", manifest.name));
    fs::write(&spec_path, serde_json::to_string_pretty(&spec)?)?;

    println!(
        "{} Exported Codex spec to: {}",
        "✓".green().bold(),
        spec_path.display()
    );

    Ok(())
}

/// Export for MCP (generates tool schema).
fn export_mcp(manifest: &SkillManifest, _skill_dir: &Path, output_dir: &Path) -> Result<()> {
    let prefix = manifest
        .exports
        .as_ref()
        .and_then(|e| e.mcp.as_ref())
        .and_then(|m| m.tools_prefix.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(&manifest.name);

    let mut mcp_tools = Vec::new();

    for daemon in &manifest.daemons {
        for method in &daemon.methods {
            mcp_tools.push(serde_json::json!({
                "name": format!("{}_{}", prefix, method),
                "description": format!("{} via FGP {} daemon", method, daemon.name),
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }));
        }
    }

    let mcp_spec = serde_json::json!({
        "name": manifest.name,
        "version": manifest.version,
        "description": manifest.description,
        "tools": mcp_tools
    });

    // Write file
    let mcp_path = output_dir.join(format!("{}.mcp.json", manifest.name));
    fs::write(&mcp_path, serde_json::to_string_pretty(&mcp_spec)?)?;

    println!(
        "{} Exported MCP schema to: {}",
        "✓".green().bold(),
        mcp_path.display()
    );

    Ok(())
}

/// Export for Windsurf (generates cascade rules).
fn export_windsurf(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut rules = String::new();

    rules.push_str(&format!("# {} - FGP Skill for Windsurf\n\n", manifest.name));
    rules.push_str(&format!("{}\n\n", manifest.description));

    // Read windsurf-specific instructions
    let windsurf_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.windsurf.as_ref());

    if let Some(instruction_path) = windsurf_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            rules.push_str(&instructions);
        }
    } else {
        // Generate default
        rules.push_str("## When to Use\n\n");
        if let Some(ref triggers) = manifest.triggers {
            for keyword in &triggers.keywords {
                rules.push_str(&format!("- User mentions \"{}\"\n", keyword));
            }
        }
        rules.push_str("\n## Commands\n\n");
        for daemon in &manifest.daemons {
            for method in &daemon.methods {
                rules.push_str(&format!(
                    "- `fgp call {}.{}`\n",
                    daemon.name, method
                ));
            }
        }
    }

    // Write file
    let rules_path = output_dir.join(format!("{}.windsurf.md", manifest.name));
    fs::write(&rules_path, &rules)?;

    println!(
        "{} Exported Windsurf rules to: {}",
        "✓".green().bold(),
        rules_path.display()
    );

    Ok(())
}
