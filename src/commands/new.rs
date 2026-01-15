//! Create a new FGP package from template.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::process::Command;

// Template file contents embedded at compile time
const TEMPLATE_MANIFEST: &str = include_str!("../templates/manifest.json.tmpl");
const TEMPLATE_CARGO: &str = include_str!("../templates/Cargo.toml.tmpl");
const TEMPLATE_MAIN: &str = include_str!("../templates/main.rs.tmpl");
const TEMPLATE_GITIGNORE: &str = include_str!("../templates/gitignore.tmpl");
const TEMPLATE_README: &str = include_str!("../templates/README.md.tmpl");
const TEMPLATE_SKILL: &str = include_str!("../templates/skill.md.tmpl");
const TEMPLATE_CURSOR: &str = include_str!("../templates/cursor.mdc.tmpl");
const TEMPLATE_WINDSURF: &str = include_str!("../templates/windsurf.md.tmpl");
const TEMPLATE_CONTINUE: &str = include_str!("../templates/continue.yaml.tmpl");

/// Known AI agent configurations for skill distribution.
const AGENT_CONFIGS: &[(&str, &str, &str)] = &[
    ("claude-code", "~/.claude/skills", "Claude Code"),
    ("cursor", "~/.cursor/rules", "Cursor"),
    ("windsurf", "~/.windsurf/workflows", "Windsurf"),
    ("continue", "~/.continue/rules", "Continue"),
];

pub fn run(name: &str, description: Option<&str>, language: &str, no_git: bool) -> Result<()> {
    // Validate name
    if !is_valid_name(name) {
        bail!(
            "Invalid package name '{}'. Use lowercase letters, numbers, and hyphens only.",
            name
        );
    }

    // Default description
    let default_desc = format!("{} service", to_title_case(name));
    let description = description.unwrap_or(&default_desc);

    // Only Rust is supported for now
    if language != "rust" {
        bail!("Only 'rust' language is currently supported");
    }

    println!();
    println!(
        "{} Creating new FGP package: {}",
        "→".blue().bold(),
        name.bold()
    );

    // Create package directory
    let package_dir = Path::new(name);
    if package_dir.exists() {
        bail!("Directory '{}' already exists", name);
    }

    fs::create_dir_all(package_dir).context("Failed to create package directory")?;
    println!("  {} Created ./{}/", "✓".green(), name);

    // Create directory structure
    fs::create_dir_all(package_dir.join("src"))?;
    fs::create_dir_all(package_dir.join("skills/claude-code"))?;
    fs::create_dir_all(package_dir.join("skills/cursor"))?;
    fs::create_dir_all(package_dir.join("skills/windsurf"))?;
    fs::create_dir_all(package_dir.join("skills/continue"))?;

    // Generate files from templates
    let name_pascal = to_pascal_case(name);
    let name_title = to_title_case(name);
    let description_lower = description.to_lowercase();

    // manifest.json
    let manifest = substitute_template(
        TEMPLATE_MANIFEST,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(package_dir.join("manifest.json"), manifest)?;
    println!("  {} Generated manifest.json", "✓".green());

    // Cargo.toml
    let cargo = substitute_template(
        TEMPLATE_CARGO,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(package_dir.join("Cargo.toml"), cargo)?;
    println!("  {} Generated Cargo.toml", "✓".green());

    // src/main.rs
    let main_rs = substitute_template(
        TEMPLATE_MAIN,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(package_dir.join("src/main.rs"), main_rs)?;
    println!(
        "  {} Generated src/main.rs (Rust daemon skeleton)",
        "✓".green()
    );

    // .gitignore
    fs::write(package_dir.join(".gitignore"), TEMPLATE_GITIGNORE)?;

    // README.md
    let readme = substitute_template(
        TEMPLATE_README,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(package_dir.join("README.md"), readme)?;
    println!("  {} Generated README.md", "✓".green());

    // Detect agents and generate skill files
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

    // Claude Code skill
    let skill = substitute_template(
        TEMPLATE_SKILL,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(package_dir.join("skills/claude-code/SKILL.md"), skill)?;
    println!("  {} Generated skills/claude-code/SKILL.md", "✓".green());

    // Cursor skill
    let cursor = substitute_template(
        TEMPLATE_CURSOR,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(
        package_dir.join(format!("skills/cursor/{}.mdc", name)),
        cursor,
    )?;
    println!("  {} Generated skills/cursor/{}.mdc", "✓".green(), name);

    // Windsurf skill
    let windsurf = substitute_template(
        TEMPLATE_WINDSURF,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(
        package_dir.join(format!("skills/windsurf/{}.md", name)),
        windsurf,
    )?;
    println!("  {} Generated skills/windsurf/{}.md", "✓".green(), name);

    // Continue skill
    let continue_skill = substitute_template(
        TEMPLATE_CONTINUE,
        name,
        description,
        &name_pascal,
        &name_title,
        &description_lower,
    );
    fs::write(
        package_dir.join(format!("skills/continue/{}.yaml", name)),
        continue_skill,
    )?;
    println!("  {} Generated skills/continue/{}.yaml", "✓".green(), name);

    // Git initialization
    if !no_git {
        let git_init = Command::new("git")
            .arg("init")
            .current_dir(package_dir)
            .output();

        if git_init.is_ok() {
            println!("  {} Initialized git repository", "✓".green());
        }
    }

    // Summary
    println!();
    println!(
        "{} Package {} created successfully!",
        "✓".green().bold(),
        name.bold()
    );
    println!();
    println!("{}", "Next steps:".bold());
    println!("  1. cd {}", name.cyan());
    println!("  2. Edit manifest.json to add your methods");
    println!("  3. Implement methods in src/main.rs");
    println!("  4. {}", "cargo build --release".cyan());
    println!("  5. {}", "fgp install .".cyan());
    println!();

    Ok(())
}

/// Check if a package name is valid.
fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }

    // Must start with a letter
    if !name
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false)
    {
        return false;
    }

    // Only lowercase letters, numbers, and hyphens
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Convert to PascalCase.
fn to_pascal_case(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Convert to Title Case.
fn to_title_case(name: &str) -> String {
    name.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Substitute template variables.
fn substitute_template(
    template: &str,
    name: &str,
    description: &str,
    name_pascal: &str,
    name_title: &str,
    description_lower: &str,
) -> String {
    template
        .replace("{{NAME}}", name)
        .replace("{{DESCRIPTION}}", description)
        .replace("{{NAME_PASCAL}}", name_pascal)
        .replace("{{NAME_TITLE}}", name_title)
        .replace("{{DESCRIPTION_LOWER}}", description_lower)
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
