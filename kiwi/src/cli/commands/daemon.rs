use crate::cli::error::{CliError, CliResult};
use crate::cli::{DaemonArgs, DaemonCommand, LogArgs};
use std::fs;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SERVICE_LABEL: &str = "com.example.kiwi";

pub fn run(args: DaemonArgs) -> CliResult<()> {
    let uid = current_uid()?;
    let domain = format!("gui/{uid}");
    let plist = args.plist.unwrap_or(default_plist_path()?);
    let log_flags = to_log_flags(args.log);

    match args.command {
        DaemonCommand::Start => start(&domain, &plist, &log_flags),
        DaemonCommand::Stop => stop(&domain, &plist),
        DaemonCommand::Restart => {
            stop(&domain, &plist)?;
            start(&domain, &plist, &log_flags)
        }
        DaemonCommand::Status => status(&domain),
    }
}

fn start(domain: &str, plist_path: &Path, log_flags: &[&str]) -> CliResult<()> {
    if !plist_path.exists() {
        return Err(CliError::new(format!(
            "plist not found at {} (run `kiwi install` first or pass --plist)",
            plist_path.display()
        )));
    }

    if !log_flags.is_empty() {
        update_plist_program_arguments(plist_path, log_flags)?;
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

fn to_log_flags(log: LogArgs) -> Vec<&'static str> {
    if log.quiet {
        return vec!["--quiet"];
    }
    if log.trace {
        return vec!["--trace"];
    }
    if log.debug {
        return vec!["--debug"];
    }
    Vec::new()
}

fn update_plist_program_arguments(plist_path: &Path, extra_args: &[&str]) -> CliResult<()> {
    let contents = fs::read_to_string(plist_path).map_err(|e| {
        CliError::new(format!(
            "failed to read plist {}: {e}",
            plist_path.display()
        ))
    })?;

    let key_idx = contents
        .find("<key>ProgramArguments</key>")
        .ok_or_else(|| CliError::new("plist is missing ProgramArguments key"))?;
    let array_start_rel = contents[key_idx..]
        .find("<array>")
        .ok_or_else(|| CliError::new("plist ProgramArguments is missing <array>"))?;
    let array_start = key_idx + array_start_rel;
    let array_end_rel = contents[array_start..]
        .find("</array>")
        .ok_or_else(|| CliError::new("plist ProgramArguments is missing </array>"))?;
    let array_end = array_start + array_end_rel + "</array>".len();

    let array_section = &contents[array_start..array_end];
    let first_string_start_rel = array_section
        .find("<string>")
        .ok_or_else(|| CliError::new("plist ProgramArguments has no executable entry"))?;
    let first_string_end_rel = array_section[first_string_start_rel + "<string>".len()..]
        .find("</string>")
        .ok_or_else(|| CliError::new("plist ProgramArguments executable entry is malformed"))?;
    let first_string_value_start = first_string_start_rel + "<string>".len();
    let exec = &array_section
        [first_string_value_start..first_string_value_start + first_string_end_rel];

    let mut new_array = String::from("<array><string>");
    new_array.push_str(exec);
    new_array.push_str("</string>");
    for arg in extra_args {
        new_array.push_str("<string>");
        new_array.push_str(arg);
        new_array.push_str("</string>");
    }
    new_array.push_str("</array>");

    let mut updated = contents;
    updated.replace_range(array_start..array_end, &new_array);
    fs::write(plist_path, updated).map_err(|e| {
        CliError::new(format!(
            "failed to update plist {}: {e}",
            plist_path.display()
        ))
    })?;

    Ok(())
}
