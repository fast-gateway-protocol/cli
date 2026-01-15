//! MCP Bridge commands for FGP.
//!
//! Expose FGP daemons as MCP servers for compatibility with Claude Desktop,
//! Cline, Continue, and other MCP-compatible tools.

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::io::{self, BufRead, Write};

// Use shared helpers from parent module
use super::{fgp_services_dir, service_socket_path};

/// Maximum retries when waiting for daemon to start.
const MAX_START_RETRIES: u32 = 10;
/// Delay between health check retries (ms).
const RETRY_DELAY_MS: u64 = 100;

/// Validate that a daemon name contains only safe characters.
/// Prevents path traversal and shell injection attacks.
fn is_valid_daemon_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Start the MCP bridge in stdio mode.
///
/// This runs an MCP server that translates MCP tool calls to FGP daemon calls.
pub fn serve() -> Result<()> {
    // MCP uses JSON-RPC 2.0 over stdio
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.context("Failed to read from stdin")?;

        if line.is_empty() {
            continue;
        }

        // Parse JSON-RPC request
        let request: serde_json::Value =
            serde_json::from_str(&line).context("Invalid JSON-RPC request")?;

        let id = request.get("id").cloned();
        let method = request["method"].as_str().unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(&request),
            "tools/list" => handle_tools_list(id),
            "tools/call" => handle_tools_call(&request),
            _ => {
                // Unknown method - return error
                json_rpc_error(id, -32601, "Method not found")
            }
        };

        // Send response
        writeln!(stdout, "{}", response)?;
        stdout.flush()?;
    }

    Ok(())
}

/// Handle MCP initialize request.
fn handle_initialize(request: &serde_json::Value) -> String {
    let id = request.get("id").cloned();

    let result = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "serverInfo": {
            "name": "fgp-mcp-bridge",
            "version": env!("CARGO_PKG_VERSION")
        },
        "capabilities": {
            "tools": {}
        }
    });

    json_rpc_response(id, result)
}

/// Encode daemon and method into MCP tool name.
/// Uses double underscore (__) as separator since daemon names can contain single underscores.
fn encode_tool_name(daemon: &str, method: &str) -> String {
    format!("fgp__{}__{}", daemon, method.replace('.', "_"))
}

/// Decode MCP tool name into daemon and method.
/// Returns None if the format is invalid.
fn decode_tool_name(tool_name: &str) -> Option<(String, String)> {
    let stripped = tool_name.strip_prefix("fgp__")?;
    let parts: Vec<&str> = stripped.splitn(2, "__").collect();
    if parts.len() != 2 {
        return None;
    }
    let daemon = parts[0].to_string();
    let method = parts[1].replace('_', ".");
    Some((daemon, method))
}

/// Handle MCP tools/list request.
fn handle_tools_list(id: Option<serde_json::Value>) -> String {
    let mut tools = Vec::new();

    // Scan installed daemons and collect their methods
    let services_dir = fgp_services_dir();
    if services_dir.exists() {
        if let Ok(entries) = fs::read_dir(&services_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let socket = service_socket_path(&name);

                if socket.exists() {
                    // Try to get methods from this daemon
                    if let Ok(client) = fgp_daemon::FgpClient::new(&socket) {
                        if let Ok(response) = client.methods() {
                            if response.ok {
                                if let Some(result) = response.result {
                                    if let Some(methods) = result["methods"].as_array() {
                                        for method in methods {
                                            let method_name =
                                                method["name"].as_str().unwrap_or("unknown");
                                            let description = method["description"]
                                                .as_str()
                                                .unwrap_or("No description");

                                            // Skip internal methods
                                            if method_name == "health"
                                                || method_name == "stop"
                                                || method_name == "methods"
                                            {
                                                continue;
                                            }

                                            // Build input schema from method params
                                            let input_schema = method
                                                .get("params")
                                                .cloned()
                                                .unwrap_or(serde_json::json!({
                                                    "type": "object",
                                                    "properties": {}
                                                }));

                                            tools.push(serde_json::json!({
                                                "name": encode_tool_name(&name, method_name),
                                                "description": format!("[FGP:{}] {}", name, description),
                                                "inputSchema": input_schema
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Add meta-tools
    tools.push(serde_json::json!({
        "name": "fgp_list_daemons",
        "description": "List all FGP daemons with their status",
        "inputSchema": {
            "type": "object",
            "properties": {}
        }
    }));

    tools.push(serde_json::json!({
        "name": "fgp_start_daemon",
        "description": "Start an FGP daemon",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the daemon to start"
                }
            },
            "required": ["name"]
        }
    }));

    tools.push(serde_json::json!({
        "name": "fgp_stop_daemon",
        "description": "Stop an FGP daemon",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the daemon to stop"
                }
            },
            "required": ["name"]
        }
    }));

    let result = serde_json::json!({
        "tools": tools
    });

    json_rpc_response(id, result)
}

/// Handle MCP tools/call request.
fn handle_tools_call(request: &serde_json::Value) -> String {
    let id = request.get("id").cloned();
    let params = &request["params"];
    let tool_name = params["name"].as_str().unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    // Handle meta-tools
    if tool_name == "fgp_list_daemons" {
        return handle_list_daemons(id);
    } else if tool_name == "fgp_start_daemon" {
        let daemon_name = arguments["name"].as_str().unwrap_or("");
        return handle_start_daemon(id, daemon_name);
    } else if tool_name == "fgp_stop_daemon" {
        let daemon_name = arguments["name"].as_str().unwrap_or("");
        return handle_stop_daemon(id, daemon_name);
    }

    // Parse tool name to extract daemon and method
    let (daemon, method) = match decode_tool_name(tool_name) {
        Some((d, m)) => (d, m),
        None => {
            return json_rpc_error(
                id,
                -32602,
                "Invalid tool name format. Expected fgp__<daemon>__<method>",
            )
        }
    };

    // Validate daemon name to prevent path traversal
    if !is_valid_daemon_name(&daemon) {
        return json_rpc_error(id, -32602, "Invalid daemon name");
    }

    // Call the daemon
    let socket = service_socket_path(&daemon);

    // Auto-start if needed with health polling
    if !socket.exists() {
        if let Err(e) = fgp_daemon::lifecycle::start_service(&daemon) {
            return json_rpc_error(id, -32603, &format!("Failed to start daemon: {}", e));
        }
        // Poll for daemon readiness instead of fixed sleep
        if !wait_for_daemon_ready(&daemon) {
            return json_rpc_error(id, -32603, "Daemon started but not responding");
        }
    }

    match fgp_daemon::FgpClient::new(&socket) {
        Ok(client) => match client.call(&method, arguments) {
            Ok(response) if response.ok => {
                let result = serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": serde_json::to_string_pretty(&response.result).unwrap_or_default()
                    }]
                });
                json_rpc_response(id, result)
            }
            Ok(response) => {
                let error_msg = response
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                json_rpc_error(id, -32603, &error_msg)
            }
            Err(e) => json_rpc_error(id, -32603, &format!("Call failed: {}", e)),
        },
        Err(e) => json_rpc_error(id, -32603, &format!("Failed to connect to daemon: {}", e)),
    }
}

/// Wait for a daemon to become ready by polling its health endpoint.
fn wait_for_daemon_ready(daemon: &str) -> bool {
    let socket = service_socket_path(daemon);
    for _ in 0..MAX_START_RETRIES {
        std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
        if socket.exists() {
            if let Ok(client) = fgp_daemon::FgpClient::new(&socket) {
                if client.health().is_ok() {
                    return true;
                }
            }
        }
    }
    false
}

/// Handle fgp_list_daemons meta-tool.
fn handle_list_daemons(id: Option<serde_json::Value>) -> String {
    let services_dir = fgp_services_dir();
    let mut daemons = Vec::new();

    if services_dir.exists() {
        if let Ok(entries) = fs::read_dir(&services_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let socket = service_socket_path(&name);

                let status = if socket.exists() {
                    if let Ok(client) = fgp_daemon::FgpClient::new(&socket) {
                        if client.health().is_ok() {
                            "running"
                        } else {
                            "error"
                        }
                    } else {
                        "error"
                    }
                } else {
                    "stopped"
                };

                daemons.push(serde_json::json!({
                    "name": name,
                    "status": status
                }));
            }
        }
    }

    let result = serde_json::json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&daemons).unwrap_or_default()
        }]
    });

    json_rpc_response(id, result)
}

/// Handle fgp_start_daemon meta-tool.
fn handle_start_daemon(id: Option<serde_json::Value>, name: &str) -> String {
    if !is_valid_daemon_name(name) {
        return json_rpc_error(id, -32602, "Invalid daemon name");
    }

    match fgp_daemon::lifecycle::start_service(name) {
        Ok(()) => {
            let result = serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": format!("Started daemon: {}", name)
                }]
            });
            json_rpc_response(id, result)
        }
        Err(e) => json_rpc_error(id, -32603, &format!("Failed to start daemon: {}", e)),
    }
}

/// Handle fgp_stop_daemon meta-tool.
fn handle_stop_daemon(id: Option<serde_json::Value>, name: &str) -> String {
    if !is_valid_daemon_name(name) {
        return json_rpc_error(id, -32602, "Invalid daemon name");
    }

    match fgp_daemon::lifecycle::stop_service(name) {
        Ok(()) => {
            let result = serde_json::json!({
                "content": [{
                    "type": "text",
                    "text": format!("Stopped daemon: {}", name)
                }]
            });
            json_rpc_response(id, result)
        }
        Err(e) => json_rpc_error(id, -32603, &format!("Failed to stop daemon: {}", e)),
    }
}

/// Create a JSON-RPC response.
fn json_rpc_response(id: Option<serde_json::Value>, result: serde_json::Value) -> String {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    });
    serde_json::to_string(&response).unwrap_or_default()
}

/// Create a JSON-RPC error response.
fn json_rpc_error(id: Option<serde_json::Value>, code: i32, message: &str) -> String {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    });
    serde_json::to_string(&response).unwrap_or_default()
}

/// Register FGP with Claude Code.
pub fn install() -> Result<()> {
    println!("{} Registering FGP with Claude Code...", "→".blue().bold());

    // Run: claude mcp add fgp -- fgp mcp serve
    let status = std::process::Command::new("claude")
        .args(["mcp", "add", "fgp", "--", "fgp", "mcp", "serve"])
        .status()
        .context("Failed to run 'claude mcp add'. Is Claude Code installed?")?;

    if status.success() {
        println!("{} FGP registered with Claude Code!", "✓".green().bold());
        println!();
        println!("Verify with: {}", "claude mcp list".cyan());
        println!("Usage: Ask Claude to use FGP tools (e.g., \"List my unread emails with FGP\")");
    } else {
        println!("{} Failed to register with Claude Code", "✗".red().bold());
    }

    Ok(())
}

/// List available MCP tools from daemons.
pub fn tools() -> Result<()> {
    println!("{}", "FGP MCP Tools".bold());
    println!("{}", "=".repeat(50));
    println!();

    let services_dir = fgp_services_dir();
    if !services_dir.exists() {
        println!("{} No FGP services installed", "!".yellow().bold());
        return Ok(());
    }

    let entries = fs::read_dir(&services_dir)?;
    let mut total_tools = 0;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let socket = service_socket_path(&name);

        println!("{}", name.cyan().bold());

        if !socket.exists() {
            println!("  {} (not running)", "○".dimmed());
            continue;
        }

        match fgp_daemon::FgpClient::new(&socket) {
            Ok(client) => match client.methods() {
                Ok(response) if response.ok => {
                    if let Some(result) = response.result {
                        if let Some(methods) = result["methods"].as_array() {
                            for method in methods {
                                let method_name = method["name"].as_str().unwrap_or("unknown");
                                let description =
                                    method["description"].as_str().unwrap_or("No description");

                                // Skip internal methods
                                if method_name == "health"
                                    || method_name == "stop"
                                    || method_name == "methods"
                                {
                                    continue;
                                }

                                println!(
                                    "  {} - {}",
                                    encode_tool_name(&name, method_name).green(),
                                    description.dimmed()
                                );
                                total_tools += 1;
                            }
                        }
                    }
                }
                _ => {
                    println!("  {} Error fetching methods", "✗".red());
                }
            },
            Err(_) => {
                println!("  {} Connection error", "✗".red());
            }
        }

        println!();
    }

    // Meta-tools
    println!("{}", "Meta-Tools".cyan().bold());
    println!(
        "  {} - List all FGP daemons with their status",
        "fgp_list_daemons".green()
    );
    println!("  {} - Start an FGP daemon", "fgp_start_daemon".green());
    println!("  {} - Stop an FGP daemon", "fgp_stop_daemon".green());

    println!();
    println!("Total: {} tools available", total_tools + 3);

    Ok(())
}
