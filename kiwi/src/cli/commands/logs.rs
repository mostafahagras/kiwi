use crate::cli::error::{CliError, CliResult};
use crate::cli::{LogStream, LogsArgs, LogsCommand, LogsTailArgs};
use std::path::PathBuf;
use std::process::Command;

pub fn run(args: LogsArgs) -> CliResult<()> {
    match args.command {
        LogsCommand::Path => {
            println!("{}", default_log_dir()?.display());
            Ok(())
        }
        LogsCommand::Tail(tail_args) => tail_logs(tail_args),
    }
}

fn tail_logs(args: LogsTailArgs) -> CliResult<()> {
    let log_dir = default_log_dir()?;
    let stdout = log_dir.join("stdout.log");
    let stderr = log_dir.join("stderr.log");

    let mut files = Vec::new();
    match args.stream {
        LogStream::Stdout => files.push(stdout),
        LogStream::Stderr => files.push(stderr),
        LogStream::Both => {
            files.push(stdout);
            files.push(stderr);
        }
    }

    if let Some(missing) = files.iter().find(|path| !path.exists()) {
        return Err(CliError::new(format!(
            "log file not found at {} (run `kiwi install` first)",
            missing.display()
        )));
    }

    let mut cmd = Command::new("tail");
    cmd.arg("-n")
        .arg(args.lines.to_string())
        .arg("-f")
        .args(files);

    let status = cmd
        .status()
        .map_err(|e| CliError::new(format!("failed to execute tail: {e}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(CliError::new("tail exited with non-zero status"))
    }
}

fn default_log_dir() -> CliResult<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| CliError::new("HOME not set; cannot resolve log directory"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Logs")
        .join("Kiwi"))
}
