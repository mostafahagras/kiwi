pub mod commands;
pub mod error;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use std::path::PathBuf;

use crate::cli::commands::{check, completions, config, ctl, daemon, install, logs, update};
use crate::cli::error::CliResult;

#[derive(Debug, Parser)]
#[command(name = "kiwi", version, about = "Keyboard shortcut daemon for macOS")]
pub struct Cli {
    #[command(flatten)]
    pub log: LogArgs,
    /// Config file path override (used when running the daemon directly)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
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
    /// Control a running Kiwi daemon via local IPC
    Ctl(CtlArgs),
    /// Validate configuration and exit
    Check(CheckArgs),
    /// Config helpers
    Config(ConfigArgs),
    /// Log helpers
    Logs(LogsArgs),
    /// Generate shell completion scripts
    Completions(CompletionArgs),
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
    #[command(flatten)]
    pub log: LogArgs,
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
pub struct CtlArgs {
    /// Control socket path override
    #[arg(long, global = true)]
    pub socket: Option<PathBuf>,
    /// Emit raw JSON response
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: CtlCommand,
}

#[derive(Debug, Subcommand)]
pub enum CtlCommand {
    Ping,
    Status,
    Reload,
    Quit,
    ConfigPath,
    Version,
    Layer(LayerArgs),
    Send(BindingsArgs),
    Press(BindingsArgs),
    Exec(ExecArgs),
}

#[derive(Debug, Args)]
pub struct LayerArgs {
    #[command(subcommand)]
    pub command: LayerCommand,
}

#[derive(Debug, Subcommand)]
pub enum LayerCommand {
    List,
    Active,
    Activate { layer: String },
}

#[derive(Debug, Args)]
pub struct BindingsArgs {
    pub bindings: Vec<String>,
}

#[derive(Debug, Args)]
pub struct ExecArgs {
    pub action: String,
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

#[derive(Debug, Args, Clone, Copy, Default)]
pub struct LogArgs {
    /// Disable logging except for errors; takes precedence over --debug/--trace
    #[arg(short = 'q', long = "quiet", global = true)]
    pub quiet: bool,
    /// Enable debug logging
    #[arg(short = 'd', long = "debug", global = true)]
    pub debug: bool,
    /// Enable trace logging (implies --debug)
    #[arg(short = 't', long = "trace", global = true)]
    pub trace: bool,
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    pub shell: Shell,
}

pub fn run(command: Commands) -> CliResult<()> {
    match command {
        Commands::Install => install::run(),
        Commands::Update(args) => update::run(args),
        Commands::Daemon(args) => daemon::run(args),
        Commands::Ctl(args) => ctl::run(args),
        Commands::Check(args) => check::run(args),
        Commands::Config(args) => config::run(args),
        Commands::Logs(args) => logs::run(args),
        Commands::Completions(args) => completions::run(args),
    }
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, CtlCommand, DaemonCommand, LayerCommand};
    use clap::Parser;

    #[test]
    fn parse_ctl_send_single_binding() {
        let cli = Cli::try_parse_from(["kiwi", "ctl", "send", "cmd+k"]).expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => match args.command {
                CtlCommand::Send(bindings) => assert_eq!(bindings.bindings, vec!["cmd+k"]),
                _ => panic!("expected ctl send"),
            },
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_ctl_send_multiple_bindings() {
        let cli =
            Cli::try_parse_from(["kiwi", "ctl", "send", "cmd+k", "shift+l"]).expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => match args.command {
                CtlCommand::Send(bindings) => {
                    assert_eq!(bindings.bindings, vec!["cmd+k", "shift+l"])
                }
                _ => panic!("expected ctl send"),
            },
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_ctl_press() {
        let cli = Cli::try_parse_from(["kiwi", "ctl", "press", "cmd+k"]).expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => match args.command {
                CtlCommand::Press(bindings) => assert_eq!(bindings.bindings, vec!["cmd+k"]),
                _ => panic!("expected ctl press"),
            },
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_ctl_exec() {
        let cli =
            Cli::try_parse_from(["kiwi", "ctl", "exec", "snap:LeftHalf"]).expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => match args.command {
                CtlCommand::Exec(exec) => assert_eq!(exec.action, "snap:LeftHalf"),
                _ => panic!("expected ctl exec"),
            },
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_ctl_layer_activate_root() {
        let cli = Cli::try_parse_from(["kiwi", "ctl", "layer", "activate", "root"])
            .expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => match args.command {
                CtlCommand::Layer(layer_args) => match layer_args.command {
                    LayerCommand::Activate { layer } => assert_eq!(layer, "root"),
                    _ => panic!("expected layer activate"),
                },
                _ => panic!("expected ctl layer"),
            },
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_ctl_json_after_subcommand() {
        let cli = Cli::try_parse_from(["kiwi", "ctl", "status", "--json"]).expect("should parse");
        match cli.command {
            Some(Commands::Ctl(args)) => {
                assert!(args.json);
                assert!(matches!(args.command, CtlCommand::Status));
            }
            _ => panic!("expected ctl command"),
        }
    }

    #[test]
    fn parse_completions_zsh() {
        let cli = Cli::try_parse_from(["kiwi", "completions", "zsh"]).expect("should parse");
        assert!(matches!(cli.command, Some(Commands::Completions(_))));
    }

    #[test]
    fn parse_daemon_restart_with_trace() {
        let cli =
            Cli::try_parse_from(["kiwi", "daemon", "restart", "--trace"]).expect("should parse");
        match cli.command {
            Some(Commands::Daemon(args)) => {
                assert!(args.log.trace);
                assert!(matches!(args.command, DaemonCommand::Restart));
            }
            _ => panic!("expected daemon command"),
        }
    }

    #[test]
    fn parse_root_config_override() {
        let cli = Cli::try_parse_from(["kiwi", "--config", "./config.toml"]).expect("should parse");
        assert!(cli.command.is_none());
        assert_eq!(
            cli.config.as_deref(),
            Some(PathBuf::from("./config.toml").as_path())
        );
    }
}
