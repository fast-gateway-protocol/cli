//! Detect installed AI agents on the system.

use anyhow::Result;
use colored::Colorize;
use std::path::Path;

/// Known AI agent configurations.
const AGENT_PATHS: &[(&str, &str, &str)] = &[
    ("Claude Code", "~/.claude/skills", "SKILL.md files"),
    ("Cursor", "~/.cursor", ".mdc rules"),
    ("Windsurf", "~/.windsurf", "Workflow files"),
    (
        "Cline",
        "~/.config/Code/User/globalStorage/saoudrizwan.claude-dev",
        "MCP config",
    ),
    ("Continue", "~/.continue", "YAML config"),
];

pub fn run() -> Result<()> {
    println!("{}", "Detecting installed AI agents...".bold());
    println!();

    let mut found_any = false;

    for (name, path, format) in AGENT_PATHS {
        let expanded = shellexpand::tilde(path);
        let path = Path::new(expanded.as_ref());

        if path.exists() {
            found_any = true;
            println!("  {} {}", "✓".green().bold(), name.bold());
            println!("    Path: {}", path.display().to_string().dimmed());
            println!("    Format: {}", format.dimmed());
            println!();
        }
    }

    if !found_any {
        println!("  {} No supported AI agents detected.", "!".yellow().bold());
        println!();
        println!("  Supported agents:");
        for (name, _, _) in AGENT_PATHS {
            println!("    - {}", name);
        }
    } else {
        println!(
            "{}",
            "FGP packages will automatically install skill files for detected agents.".dimmed()
        );
    }

    Ok(())
}
