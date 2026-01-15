//! List available methods for a service.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use tabled::{Table, Tabled};

use super::service_socket_path;

#[derive(Tabled)]
struct MethodInfo {
    #[tabled(rename = "Method")]
    name: String,
    #[tabled(rename = "Description")]
    description: String,
}

pub fn run(service: &str) -> Result<()> {
    let socket_path = service_socket_path(service);

    if !socket_path.exists() {
        bail!(
            "Service '{}' is not running. Run 'fgp start {}' first.",
            service,
            service
        );
    }

    let client = fgp_daemon::FgpClient::new(&socket_path).context("Failed to connect to daemon")?;

    let response = client.methods().context("Failed to get methods")?;

    if !response.ok {
        let error = response.error.unwrap_or_default();
        bail!("Error ({}): {}", error.code, error.message);
    }

    let result = response.result.unwrap_or_default();
    let methods_array = result["methods"].as_array().cloned().unwrap_or_default();

    println!("{} methods:", service.bold());
    println!();

    let methods: Vec<MethodInfo> = methods_array
        .iter()
        .map(|m| MethodInfo {
            name: m["name"].as_str().unwrap_or("?").to_string(),
            description: m["description"].as_str().unwrap_or("").to_string(),
        })
        .collect();

    if methods.is_empty() {
        println!("  No methods available.");
    } else {
        let table = Table::new(&methods).to_string();
        println!("{}", table);
    }

    Ok(())
}
