//! FGP CLI - Command-line interface for Fast Gateway Protocol daemons.
//!
//! # Usage
//!
//! ```bash
//! fgp agents              # Detect installed AI agents
//! fgp generate <service>  # Generate a new daemon from template
//! fgp new <name>          # Create a new FGP package from template
//! fgp start <service>     # Start a daemon
//! fgp stop <service>      # Stop a daemon
//! fgp status              # Show running daemons
//! fgp call <method>       # Call a method
//! fgp install <package>   # Install from local path
//! ```

mod commands;
mod tui;

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

    /// Generate a new daemon from template (67 service presets available)
    Generate {
        #[command(subcommand)]
        action: GenerateAction,
    },

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

    /// Interactive terminal dashboard
    Tui {
        /// Service polling interval in milliseconds
        #[arg(short, long, default_value = "2000")]
        poll: u64,
    },

    /// Run or validate a workflow
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },

    /// Manage FGP skills (install, update, search)
    Skill {
        #[command(subcommand)]
        action: SkillAction,
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

#[derive(Subcommand)]
enum SkillAction {
    /// List installed skills
    List,

    /// Search for skills in marketplaces
    Search {
        /// Search query
        query: String,
    },

    /// Install a skill from marketplace
    Install {
        /// Skill name (e.g., "browser-gateway")
        name: String,

        /// Specific marketplace to install from
        #[arg(short, long)]
        from: Option<String>,
    },

    /// Check for skill updates
    Update,

    /// Upgrade installed skills
    Upgrade {
        /// Specific skill to upgrade (all if not specified)
        skill: Option<String>,
    },

    /// Remove an installed skill
    Remove {
        /// Skill name to remove
        name: String,
    },

    /// Show detailed info about a skill
    Info {
        /// Skill name
        name: String,
    },

    /// Validate a skill manifest (skill.yaml)
    Validate {
        /// Path to skill directory or skill.yaml file
        path: String,
    },

    /// Export skill for a specific agent (claude-code, cursor, mcp, windsurf)
    Export {
        /// Target agent: claude-code, cursor, codex, mcp, windsurf
        target: String,

        /// Skill name or path to skill directory
        skill: String,

        /// Output directory (default: current directory)
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Manage skill taps (GitHub-based skill repositories)
    Tap {
        #[command(subcommand)]
        action: TapAction,
    },

    /// Manage skill marketplaces (legacy)
    Marketplace {
        #[command(subcommand)]
        action: MarketplaceAction,
    },

    /// Manage MCP bridge registration
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
}

#[derive(Subcommand)]
enum TapAction {
    /// Add a GitHub tap (e.g., fast-gateway-protocol/official-skills)
    Add {
        /// GitHub owner/repo (e.g., "fast-gateway-protocol/official-skills")
        repo: String,
    },

    /// Remove a tap
    Remove {
        /// Tap name to remove
        name: String,
    },

    /// List all configured taps
    List,

    /// Update all taps (git pull)
    Update,

    /// Show skills available in a specific tap
    Show {
        /// Tap name
        name: String,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// Register an installed skill with MCP server (and optionally other ecosystems)
    Register {
        /// Skill name to register
        name: String,

        /// Target ecosystems (comma-separated): mcp, claude, cursor, continue, windsurf, all
        #[arg(short, long, default_value = "mcp")]
        target: String,
    },

    /// Register all installed skills with MCP server
    RegisterAll,

    /// List MCP-registered skills
    List,

    /// Show registration status for a skill across all ecosystems
    Status {
        /// Skill name to check
        name: String,
    },
}

#[derive(Subcommand)]
enum MarketplaceAction {
    /// List configured marketplaces
    List,

    /// Add a new marketplace
    Add {
        /// GitHub URL or marketplace name
        url: String,
    },

    /// Update all marketplaces (git pull)
    Update,
}

#[derive(Subcommand)]
enum GenerateAction {
    /// List all available service presets
    List,

    /// Create a new daemon from a service preset
    #[command(name = "new")]
    NewDaemon {
        /// Service name (e.g., "slack", "linear", "notion")
        service: String,

        /// Use preset configuration for known services
        #[arg(short, long)]
        preset: bool,

        /// Human-readable display name
        #[arg(long)]
        display_name: Option<String>,

        /// Base URL for the API
        #[arg(long)]
        api_url: Option<String>,

        /// Environment variable name for API token
        #[arg(long)]
        env_token: Option<String>,

        /// Output directory (default: current directory)
        #[arg(short, long)]
        output: Option<String>,

        /// Author name for changelog entries
        #[arg(long, default_value = "Claude")]
        author: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Agents => commands::agents::run(),
        Commands::Generate { action } => match action {
            GenerateAction::List => commands::generate::list(),
            GenerateAction::NewDaemon {
                service,
                preset,
                display_name,
                api_url,
                env_token,
                output,
                author,
            } => commands::generate::new_daemon(
                &service,
                preset,
                display_name.as_deref(),
                api_url.as_deref(),
                env_token.as_deref(),
                output.as_deref(),
                &author,
            ),
        },
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
        Commands::Tui { poll } => commands::tui::run(poll),
        Commands::Workflow { action } => match action {
            WorkflowAction::Run { file, verbose } => commands::workflow::run(&file, verbose),
            WorkflowAction::Validate { file } => commands::workflow::validate(&file),
        },
        Commands::Skill { action } => match action {
            SkillAction::List => commands::skill::list(),
            SkillAction::Search { query } => commands::skill::search(&query),
            SkillAction::Install { name, from } => {
                commands::skill::install(&name, from.as_deref())
            }
            SkillAction::Update => commands::skill::check_updates(),
            SkillAction::Upgrade { skill } => commands::skill::upgrade(skill.as_deref()),
            SkillAction::Remove { name } => commands::skill::remove(&name),
            SkillAction::Info { name } => commands::skill::info(&name),
            SkillAction::Validate { path } => commands::skill_validate::validate(&path),
            SkillAction::Export { target, skill, output } => {
                commands::skill_export::export(&target, &skill, output.as_deref())
            }
            SkillAction::Tap { action } => match action {
                TapAction::Add { repo } => commands::skill_tap::add(&repo),
                TapAction::Remove { name } => commands::skill_tap::remove(&name),
                TapAction::List => commands::skill_tap::list(),
                TapAction::Update => commands::skill_tap::update(),
                TapAction::Show { name } => commands::skill_tap::show(&name),
            },
            SkillAction::Marketplace { action } => match action {
                MarketplaceAction::List => commands::skill::marketplace_list(),
                MarketplaceAction::Add { url } => commands::skill::marketplace_add(&url),
                MarketplaceAction::Update => commands::skill::marketplace_update(),
            },
            SkillAction::Mcp { action } => match action {
                McpAction::Register { name, target } => {
                    if target == "mcp" {
                        commands::skill::mcp_register(&name)
                    } else {
                        commands::skill::register_with_targets(&name, &target)
                    }
                }
                McpAction::RegisterAll => commands::skill::mcp_register_all(),
                McpAction::List => commands::skill::mcp_list(),
                McpAction::Status { name } => commands::skill::registration_status(&name),
            },
        },
    }
}
