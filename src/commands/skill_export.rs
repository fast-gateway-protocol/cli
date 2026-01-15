//! Export FGP skills to agent-specific formats.
//!
//! Supported targets:
//! - claude-code: Generates SKILL.md for ~/.claude/skills/
//! - cursor: Generates .cursorrules and commands
//! - codex: Generates tool spec and prompts
//! - mcp: Generates MCP tool schema
//! - windsurf: Generates cascade rules
//! - zed: Generates .rules file for Zed's AI assistant

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
    let (skill_dir, manifest_path) = if skill_path.is_dir() {
        (skill_path.to_path_buf(), skill_path.join("skill.yaml"))
    } else if skill_path.extension().map(|e| e == "yaml" || e == "yml").unwrap_or(false) {
        (
            skill_path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            skill_path.to_path_buf(),
        )
    } else {
        // Assume it's a skill name, look in installed skills
        let installed_path = shellexpand::tilde("~/.fgp/skills").to_string();
        let installed_skill_dir = Path::new(&installed_path).join(skill);
        (
            installed_skill_dir.clone(),
            installed_skill_dir.join("skill.yaml"),
        )
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
        "zed" => export_zed(&manifest, &skill_dir, &output_dir),
        "gemini" => export_gemini(&manifest, &skill_dir, &output_dir),
        "aider" => export_aider(&manifest, &skill_dir, &output_dir),
        _ => bail!(
            "Unknown export target: {}\n\
             Valid targets: claude-code, cursor, codex, mcp, windsurf, zed, gemini, aider",
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

/// Export for Zed (generates .rules file for Zed's AI assistant).
fn export_zed(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut rules = String::new();

    // Zed rules format - plain text instructions for the AI assistant
    rules.push_str(&format!("# {}\n\n", manifest.name));
    rules.push_str(&format!("{}\n\n", manifest.description));

    // Read zed-specific instructions if they exist
    let zed_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.zed.as_ref())
        .or_else(|| manifest.instructions.as_ref().and_then(|i| i.core.as_ref()));

    if let Some(instruction_path) = zed_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            rules.push_str(&instructions);
        }
    } else {
        // Generate default rules
        rules.push_str("## When to Activate\n\n");
        if let Some(ref triggers) = manifest.triggers {
            rules.push_str("Activate this skill when the user:\n");
            for keyword in &triggers.keywords {
                rules.push_str(&format!("- Mentions \"{}\"\n", keyword));
            }
            for pattern in &triggers.patterns {
                rules.push_str(&format!("- Asks to \"{}\"\n", pattern));
            }
            rules.push_str("\n");
        }

        // Add FGP daemon usage
        if !manifest.daemons.is_empty() {
            rules.push_str("## FGP Daemons\n\n");
            rules.push_str("Use these Fast Gateway Protocol commands for high-performance execution:\n\n");
            rules.push_str("```bash\n");
            for daemon in &manifest.daemons {
                for method in &daemon.methods {
                    rules.push_str(&format!(
                        "fgp call {}.{} -p '{{\"param\": \"value\"}}'\n",
                        daemon.name, method
                    ));
                }
            }
            rules.push_str("```\n\n");

            rules.push_str("### Available Methods\n\n");
            for daemon in &manifest.daemons {
                let optional = if daemon.optional { " (optional)" } else { "" };
                rules.push_str(&format!("**{}**{}:\n", daemon.name, optional));
                for method in &daemon.methods {
                    rules.push_str(&format!("- `{}.{}`\n", daemon.name, method));
                }
                rules.push_str("\n");
            }
        }

        // Add workflow info
        if !manifest.workflows.is_empty() {
            rules.push_str("## Workflows\n\n");
            for (name, workflow) in &manifest.workflows {
                let default = if workflow.default { " (default)" } else { "" };
                let desc = workflow.description.as_deref().unwrap_or("");
                rules.push_str(&format!("- **{}**{}: {}\n", name, default, desc));
            }
            rules.push_str("\n");
        }
    }

    // Write .rules file (Zed's native format)
    let rules_path = output_dir.join(format!("{}.rules", manifest.name));
    fs::write(&rules_path, &rules)?;

    println!(
        "{} Exported Zed rules to: {}",
        "✓".green().bold(),
        rules_path.display()
    );

    // Provide usage hints
    println!();
    println!("{}:", "Usage".cyan().bold());
    println!("  1. Copy to project root as .rules");
    println!("  2. Or add to Zed's Rules Library (Cmd+Alt+L)");

    Ok(())
}

/// Export for Gemini CLI (generates extension directory with gemini-extension.json + GEMINI.md).
fn export_gemini(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    // Create extension directory
    let ext_dir = output_dir.join(&manifest.name);
    fs::create_dir_all(&ext_dir)?;

    // Generate gemini-extension.json manifest
    let extension_json = serde_json::json!({
        "name": manifest.name,
        "version": manifest.version,
        "contextFileName": "GEMINI.md"
    });
    let manifest_path = ext_dir.join("gemini-extension.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&extension_json)?)?;

    // Generate GEMINI.md context file
    let mut gemini_md = String::new();
    gemini_md.push_str(&format!("# {}\n\n", manifest.name));
    gemini_md.push_str(&format!("{}\n\n", manifest.description));

    // Read gemini-specific or core instructions
    let gemini_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.core.as_ref());

    if let Some(instruction_path) = gemini_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            gemini_md.push_str(&instructions);
        }
    } else {
        // Generate default context
        if let Some(ref triggers) = manifest.triggers {
            gemini_md.push_str("## When to Use\n\n");
            gemini_md.push_str("Activate this skill when the user:\n");
            for keyword in &triggers.keywords {
                gemini_md.push_str(&format!("- Mentions \"{}\"\n", keyword));
            }
            for pattern in &triggers.patterns {
                gemini_md.push_str(&format!("- Asks to \"{}\"\n", pattern));
            }
            gemini_md.push_str("\n");
        }

        // Add FGP daemon usage
        if !manifest.daemons.is_empty() {
            gemini_md.push_str("## FGP Commands\n\n");
            gemini_md.push_str("Use Fast Gateway Protocol for high-performance execution:\n\n");
            gemini_md.push_str("```bash\n");
            for daemon in &manifest.daemons {
                for method in &daemon.methods {
                    gemini_md.push_str(&format!(
                        "fgp call {}.{} -p '{{}}'\n",
                        daemon.name, method
                    ));
                }
            }
            gemini_md.push_str("```\n");
        }
    }

    let gemini_md_path = ext_dir.join("GEMINI.md");
    fs::write(&gemini_md_path, &gemini_md)?;

    println!(
        "{} Exported Gemini extension to: {}",
        "✓".green().bold(),
        ext_dir.display()
    );

    // Provide usage hints
    println!();
    println!("{}:", "Usage".cyan().bold());
    println!("  1. Copy directory to ~/.gemini/extensions/");
    println!("  2. Or run: gemini extensions install {}", ext_dir.display());

    Ok(())
}

/// Export for Aider (generates CONVENTIONS.md).
fn export_aider(manifest: &SkillManifest, skill_dir: &Path, output_dir: &Path) -> Result<()> {
    let mut conventions = String::new();

    conventions.push_str(&format!("# {} Conventions\n\n", manifest.name));
    conventions.push_str(&format!("{}\n\n", manifest.description));

    // Read aider-specific or core instructions
    let aider_instructions = manifest
        .instructions
        .as_ref()
        .and_then(|i| i.core.as_ref());

    if let Some(instruction_path) = aider_instructions {
        let full_path = skill_dir.join(instruction_path);
        if full_path.exists() {
            let instructions = fs::read_to_string(&full_path)?;
            conventions.push_str(&instructions);
        }
    } else {
        // Generate default conventions
        conventions.push_str("## Guidelines\n\n");

        if let Some(ref triggers) = manifest.triggers {
            conventions.push_str("When working with this skill:\n");
            for keyword in &triggers.keywords {
                conventions.push_str(&format!("- Use when dealing with \"{}\"\n", keyword));
            }
            conventions.push_str("\n");
        }

        // Add FGP daemon usage
        if !manifest.daemons.is_empty() {
            conventions.push_str("## FGP Integration\n\n");
            conventions.push_str("This project uses Fast Gateway Protocol daemons for performance.\n\n");
            conventions.push_str("### Available Commands\n\n");
            for daemon in &manifest.daemons {
                let optional = if daemon.optional { " (optional)" } else { "" };
                conventions.push_str(&format!("**{}**{}:\n", daemon.name, optional));
                for method in &daemon.methods {
                    conventions.push_str(&format!(
                        "- `fgp call {}.{} -p '{{\"param\": \"value\"}}'`\n",
                        daemon.name, method
                    ));
                }
                conventions.push_str("\n");
            }
        }

        // Add workflow info
        if !manifest.workflows.is_empty() {
            conventions.push_str("## Workflows\n\n");
            for (name, workflow) in &manifest.workflows {
                let desc = workflow.description.as_deref().unwrap_or("");
                conventions.push_str(&format!("- **{}**: {}\n", name, desc));
            }
            conventions.push_str("\n");
        }
    }

    // Write CONVENTIONS.md
    let conventions_path = output_dir.join(format!("{}.CONVENTIONS.md", manifest.name));
    fs::write(&conventions_path, &conventions)?;

    println!(
        "{} Exported Aider conventions to: {}",
        "✓".green().bold(),
        conventions_path.display()
    );

    // Provide usage hints
    println!();
    println!("{}:", "Usage".cyan().bold());
    println!("  1. Rename to CONVENTIONS.md in project root");
    println!("  2. Run: aider --read CONVENTIONS.md");
    println!("  3. Or add to .aider.conf.yml: read: CONVENTIONS.md");

    Ok(())
}
