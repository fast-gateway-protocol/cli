//! FGP CLI - Command-line interface for Fast Gateway Protocol daemons.
//!
//! # Usage
//!
//! ```bash
//! fgp agents              # Detect installed AI agents
//! fgp new <name>          # Create a new FGP package from template
//! fgp start <service>     # Start a daemon
//! fgp stop <service>      # Stop a daemon
//! fgp status              # Show running daemons
//! fgp call <method>       # Call a method
//! fgp install <package>   # Install from local path
//! ```

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Fast Gateway Protocol CLI
///
/// Manage FGP daemons - the fast backend for AI agent capabilities.
#[derive(Parser)]
#[command(name = "fgp")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Detect installed AI agents on this machine
    Agents,

    /// Create a new FGP package from template
    New {
        /// Package name (e.g., "my-service")
        name: String,

        /// Service description
        #[arg(short, long)]
        description: Option<String>,

        /// Implementation language (rust, python)
        #[arg(short, long, default_value = "rust")]
        language: String,

        /// Skip git initialization
        #[arg(long)]
        no_git: bool,
    },

    /// Start a daemon service
    Start {
        /// Service name (e.g., "gmail", "imessage")
        service: String,

        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stop a running daemon
    Stop {
        /// Service name to stop
        service: String,
    },

    /// Show status of all running daemons
    Status {
        /// Show detailed health information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Call a method on a daemon
    Call {
        /// Method name (e.g., "gmail.list", "imessage.send")
        method: String,

        /// JSON parameters (e.g., '{"limit": 10}')
        #[arg(short, long, default_value = "{}")]
        params: String,

        /// Service name (inferred from method if not provided)
        #[arg(short, long)]
        service: Option<String>,

        /// Disable auto-start (fail if daemon is not running)
        #[arg(long)]
        no_auto_start: bool,
    },

    /// Install a package from local path
    Install {
        /// Path to package directory or manifest
        path: String,
    },

    /// List available methods for a service
    Methods {
        /// Service name
        service: String,
    },

    /// Check health of a specific service
    Health {
        /// Service name
        service: String,
    },

    /// Open the web dashboard
    Dashboard {
        /// Port to listen on
        #[arg(short, long, default_value = "8765")]
        port: u16,

        /// Open browser automatically
        #[arg(short, long)]
        open: bool,
    },

    /// Run or validate a workflow
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
}

#[derive(Subcommand)]
enum WorkflowAction {
    /// Run a workflow from YAML file
    Run {
        /// Path to workflow YAML file
        file: String,

        /// Show verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Validate a workflow file without running it
    Validate {
        /// Path to workflow YAML file
        file: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Agents => commands::agents::run(),
        Commands::New {
            name,
            description,
            language,
            no_git,
        } => commands::new::run(&name, description.as_deref(), &language, no_git),
        Commands::Start {
            service,
            foreground,
        } => commands::start::run(&service, foreground),
        Commands::Stop { service } => commands::stop::run(&service),
        Commands::Status { verbose } => commands::status::run(verbose),
        Commands::Call {
            method,
            params,
            service,
            no_auto_start,
        } => commands::call::run(&method, &params, service.as_deref(), no_auto_start),
        Commands::Install { path } => commands::install::run(&path),
        Commands::Methods { service } => commands::methods::run(&service),
        Commands::Health { service } => commands::health::run(&service),
        Commands::Dashboard { port, open } => commands::dashboard::run(port, open),
        Commands::Workflow { action } => match action {
            WorkflowAction::Run { file, verbose } => commands::workflow::run(&file, verbose),
            WorkflowAction::Validate { file } => commands::workflow::validate(&file),
        },
    }
}
