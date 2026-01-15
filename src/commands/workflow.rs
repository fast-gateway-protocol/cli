//! Run and validate FGP workflows.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;

/// Built-in workflow templates.
static TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "email-summary",
        "Summarize unread emails",
        r#"name: email-summary
description: Summarize unread emails
steps:
  - service: gmail
    method: gmail.unread
    params:
      limit: 10
    output: emails
"#,
    ),
    (
        "calendar-today",
        "Get today's calendar events",
        r#"name: calendar-today
description: Get today's calendar events
steps:
  - service: calendar
    method: calendar.today
    output: events
"#,
    ),
    (
        "github-prs",
        "List open PRs needing review",
        r#"name: github-prs
description: Find PRs that need your review
steps:
  - service: github
    method: github.prs
    params:
      state: open
      review_requested: true
    output: prs
"#,
    ),
];

/// Run a workflow from a YAML file.
pub fn run(file: &str, verbose: bool) -> Result<()> {
    println!("{} Loading workflow from {}...", "→".blue().bold(), file);

    // Load and parse the workflow
    let workflow = fgp_workflow::yaml::load_file(file).context("Failed to load workflow")?;

    println!(
        "{} Running workflow: {}",
        "→".blue().bold(),
        workflow.name.bold()
    );

    if let Some(ref desc) = workflow.description {
        println!("  {}", desc.dimmed());
    }

    println!("  Steps: {}", workflow.steps.len());
    println!();

    // Execute the workflow
    let result = fgp_workflow::execute(&workflow)?;

    // Print results
    println!("{} Workflow completed!", "✓".green().bold());
    println!();

    if verbose {
        println!("Step Results:");
        for step_result in &result.step_results {
            println!(
                "  {}. {} ({:.1}ms)",
                step_result.index + 1,
                format!("{}.{}", step_result.step.service, step_result.step.method).bold(),
                step_result.duration_ms
            );

            // Print output variable if set
            if let Some(ref output) = step_result.step.output {
                println!("     → {}", output.cyan());
            }
        }
        println!();
    }

    println!("Total time: {:.1}ms", result.total_ms);

    // Print final result
    println!();
    println!("Result:");
    println!("{}", serde_json::to_string_pretty(&result.result)?);

    Ok(())
}

/// Validate a workflow file without running it.
pub fn validate(file: &str) -> Result<()> {
    println!("{} Validating workflow {}...", "→".blue().bold(), file);

    // Load and parse the workflow
    let workflow = fgp_workflow::yaml::load_file(file).context("Failed to load workflow")?;

    println!("{} Workflow is valid!", "✓".green().bold());
    println!();
    println!("Name: {}", workflow.name.bold());

    if let Some(ref desc) = workflow.description {
        println!("Description: {}", desc);
    }

    println!("Steps: {}", workflow.steps.len());

    for (i, step) in workflow.steps.iter().enumerate() {
        println!(
            "  {}. {} → {}",
            i + 1,
            format!("{}.{}", step.service, step.method).bold(),
            step.output.as_deref().unwrap_or("-").cyan()
        );
    }

    Ok(())
}

/// List available workflow templates.
pub fn list(builtin_only: bool) -> Result<()> {
    println!("{}", "Workflow Templates".bold());
    println!("{}", "=".repeat(50));
    println!();

    // Built-in templates
    println!("{}", "Built-in Templates:".cyan());
    for (name, desc, _) in TEMPLATES {
        println!("  {} - {}", name.green(), desc.dimmed());
    }

    if !builtin_only {
        // User templates from ~/.fgp/workflows/
        let workflows_dir = workflows_dir();
        if workflows_dir.exists() {
            println!();
            println!("{}", "User Workflows:".cyan());
            for entry in fs::read_dir(&workflows_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path
                    .extension()
                    .map(|e| e == "yaml" || e == "yml")
                    .unwrap_or(false)
                {
                    if let Some(name) = path.file_stem().and_then(|n| n.to_str()) {
                        println!("  {}", name.green());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Initialize a workflow from a template.
pub fn init(template: &str) -> Result<()> {
    // Find template
    let content = TEMPLATES
        .iter()
        .find(|(name, _, _)| *name == template)
        .map(|(_, _, content)| *content);

    let content = match content {
        Some(c) => c,
        None => {
            bail!(
                "Template '{}' not found. Use 'fgp workflow list --builtin' to see available templates.",
                template
            );
        }
    };

    // Create workflows directory if needed
    let workflows_dir = workflows_dir();
    fs::create_dir_all(&workflows_dir)?;

    // Write template
    let output_path = workflows_dir.join(format!("{}.yaml", template));
    if output_path.exists() {
        bail!(
            "Workflow '{}' already exists at {}",
            template,
            output_path.display()
        );
    }

    fs::write(&output_path, content)?;

    println!(
        "{} Created workflow: {}",
        "✓".green().bold(),
        output_path.display()
    );
    println!();
    println!(
        "Run with: {}",
        format!("fgp workflow run {}", output_path.display()).cyan()
    );

    Ok(())
}

/// Get the workflows directory.
fn workflows_dir() -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/workflows");
    PathBuf::from(base.as_ref())
}
