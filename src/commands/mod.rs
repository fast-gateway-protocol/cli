//! CLI command implementations.

pub mod agents;
pub mod call;
pub mod dashboard;
pub mod health;
pub mod install;
pub mod methods;
pub mod new;
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
