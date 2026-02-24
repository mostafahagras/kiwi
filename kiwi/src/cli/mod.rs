pub mod commands;
pub mod error;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

use crate::cli::commands::{check, config, daemon, install, logs, update};
use crate::cli::error::CliResult;

#[derive(Debug, Parser)]
#[command(name = "kiwi", version, about = "Keyboard shortcut daemon for macOS")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Install Kiwi app bundle and launch agent
    #[command(alias = "i")]
    Install,
    /// Update Kiwi using cargo install --path kiwi --force
    Update(UpdateArgs),
    /// Control Kiwi launch agent
    Daemon(DaemonArgs),
    /// Validate configuration and exit
    Check(CheckArgs),
    /// Config helpers
    Config(ConfigArgs),
    /// Log helpers
    Logs(LogsArgs),
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Repository path containing Kiwi workspace root
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Config file path override
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub command: DaemonCommand,
    /// LaunchAgent plist path override
    #[arg(long, global = true)]
    pub plist: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum DaemonCommand {
    Start,
    Stop,
    Restart,
    Status,
}

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print resolved config path
    Path,
    /// Initialize ~/.kiwi/config.toml
    Init(ConfigInitArgs),
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    /// Overwrite file if it exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    #[command(subcommand)]
    pub command: LogsCommand,
}

#[derive(Debug, Subcommand)]
pub enum LogsCommand {
    /// Print default log directory path
    Path,
    /// Tail Kiwi logs
    Tail(LogsTailArgs),
}

#[derive(Debug, Args)]
pub struct LogsTailArgs {
    /// Number of lines to print before following
    #[arg(long, default_value_t = 100)]
    pub lines: u32,
    /// Which stream(s) to tail
    #[arg(long, value_enum, default_value_t = LogStream::Both)]
    pub stream: LogStream,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
    Both,
}

pub fn run(command: Commands) -> CliResult<()> {
    match command {
        Commands::Install => install::run(),
        Commands::Update(args) => update::run(args),
        Commands::Daemon(args) => daemon::run(args),
        Commands::Check(args) => check::run(args),
        Commands::Config(args) => config::run(args),
        Commands::Logs(args) => logs::run(args),
    }
}
