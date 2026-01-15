//! Run and validate FGP workflows.

use anyhow::{Context, Result};
use colored::Colorize;

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
