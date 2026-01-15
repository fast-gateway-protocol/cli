//! CLI command implementations.

pub mod agents;
pub mod call;
pub mod dashboard;
pub mod generate;
pub mod health;
pub mod install;
pub mod logs;
pub mod mcp_bridge;
pub mod methods;
pub mod monitor;
pub mod new;
pub mod skill;
pub mod skill_export;
pub mod skill_tap;
pub mod skill_validate;
pub mod start;
pub mod status;
pub mod stop;
pub mod tui;
pub mod workflow;

use std::path::PathBuf;

/// Get the FGP services directory.
pub fn fgp_services_dir() -> PathBuf {
    let base = shellexpand::tilde("~/.fgp/services");
    PathBuf::from(base.as_ref())
}

/// Get the socket path for a service.
pub fn service_socket_path(service: &str) -> PathBuf {
    fgp_services_dir().join(service).join("daemon.sock")
}

/// Get the PID file path for a service.
#[allow(dead_code)]
pub fn service_pid_path(service: &str) -> PathBuf {
    fgp_services_dir().join(service).join("daemon.pid")
}
