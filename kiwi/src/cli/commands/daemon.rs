use crate::cli::error::{CliError, CliResult};
use crate::cli::{DaemonArgs, DaemonCommand};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SERVICE_LABEL: &str = "com.example.kiwi";

pub fn run(args: DaemonArgs) -> CliResult<()> {
    let uid = current_uid()?;
    let domain = format!("gui/{uid}");
    let plist = args.plist.unwrap_or(default_plist_path()?);

    match args.command {
        DaemonCommand::Start => start(&domain, &plist),
        DaemonCommand::Stop => stop(&domain, &plist),
        DaemonCommand::Restart => {
            stop(&domain, &plist)?;
            start(&domain, &plist)
        }
        DaemonCommand::Status => status(&domain),
    }
}

fn start(domain: &str, plist_path: &Path) -> CliResult<()> {
    if !plist_path.exists() {
        return Err(CliError::new(format!(
            "plist not found at {} (run `kiwi install` first or pass --plist)",
            plist_path.display()
        )));
    }

    let bootstrap = launchctl(["bootstrap", domain, path_to_str(plist_path)?])?;
    if bootstrap.status.success() {
        println!("started");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&bootstrap.stderr).to_lowercase();
    if stderr.contains("in use") || stderr.contains("already") {
        let service = format!("{domain}/{SERVICE_LABEL}");
        let kickstart = launchctl(["kickstart", "-k", &service])?;
        if kickstart.status.success() {
            println!("started");
            return Ok(());
        }
    }

    Err(CliError::new(format!(
        "failed to start service: {}",
        String::from_utf8_lossy(&bootstrap.stderr).trim()
    )))
}

fn stop(domain: &str, plist_path: &Path) -> CliResult<()> {
    let plist_str = path_to_str(plist_path)?;
    let first = launchctl(["bootout", domain, plist_str])?;
    if first.status.success() {
        println!("stopped");
        return Ok(());
    }

    let service = format!("{domain}/{SERVICE_LABEL}");
    let second = launchctl(["bootout", &service])?;
    if second.status.success() {
        println!("stopped");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&first.stderr).trim().to_string();
    if stderr.contains("No such process") || stderr.contains("Could not find service") {
        println!("inactive");
        return Ok(());
    }

    Err(CliError::new(format!("failed to stop service: {stderr}")))
}

fn status(domain: &str) -> CliResult<()> {
    let service = format!("{domain}/{SERVICE_LABEL}");
    let output = launchctl(["print", &service])?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let active = stdout.contains("state = active");
        let pid = extract_pid(&stdout);
        if active {
            if let Some(pid) = pid {
                println!("active pid={pid}");
            } else {
                println!("active");
            }
        } else {
            println!("inactive");
        }
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Could not find service") {
        println!("inactive");
        return Ok(());
    }

    Err(CliError::new(format!(
        "failed to query service status: {}",
        stderr.trim()
    )))
}

fn extract_pid(launchctl_stdout: &str) -> Option<String> {
    launchctl_stdout
        .lines()
        .find(|line| line.trim_start().starts_with("pid ="))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim().to_string())
}

fn launchctl<I, S>(args: I) -> CliResult<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("launchctl")
        .args(args)
        .output()
        .map_err(|e| CliError::new(format!("failed to execute launchctl: {e}")))
}

fn current_uid() -> CliResult<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| CliError::new(format!("failed to execute id -u: {e}")))?;

    if !output.status.success() {
        return Err(CliError::new("failed to resolve current uid"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn default_plist_path() -> CliResult<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| CliError::new("HOME not set; pass --plist explicitly"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist")))
}

fn path_to_str(path: &Path) -> CliResult<&str> {
    path.to_str()
        .ok_or_else(|| CliError::new("path contains invalid UTF-8"))
}
