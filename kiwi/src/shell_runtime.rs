use std::env;
use std::ffi::CStr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{OnceLock, RwLock};
use tracing::{debug, warn};

const DEFAULT_PATH: &str = "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

#[derive(Debug, Clone)]
pub struct ShellContext {
    pub shell_path: PathBuf,
    pub path_value: String,
}

static SHELL_CONTEXT: OnceLock<RwLock<ShellContext>> = OnceLock::new();

#[derive(Debug, PartialEq, Eq)]
enum CommandPlan {
    Direct { program: String, args: Vec<String> },
    ShellFallback,
}

fn context() -> &'static RwLock<ShellContext> {
    SHELL_CONTEXT.get_or_init(|| {
        let shell_path = detect_user_shell();
        let path_value = env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string());
        RwLock::new(ShellContext {
            shell_path,
            path_value,
        })
    })
}

pub fn init_shell_context() {
    let _ = context();
    refresh_path_cache();
}

pub fn refresh_path_cache() {
    let shell_path = match context().read() {
        Ok(guard) => guard.shell_path.clone(),
        Err(_) => {
            warn!("Shell context is poisoned; cannot refresh PATH cache");
            return;
        }
    };

    let next_path = fetch_shell_path(&shell_path);

    if let Ok(mut guard) = context().write() {
        if let Some(path) = next_path {
            guard.path_value = path;
        } else if guard.path_value.trim().is_empty() {
            guard.path_value = env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string());
        }
    } else {
        warn!("Shell context is poisoned; cannot update PATH cache");
    }
}

pub fn execute_user_command(cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return;
    }

    match plan_command(cmd) {
        CommandPlan::Direct { program, args } => {
            let path_value = cached_path();
            let status = Command::new(&program)
                .args(&args)
                .env("PATH", &path_value)
                .status();
            match status {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    debug!(
                        "Direct command not found in PATH cache, falling back to shell: {}",
                        program
                    );
                    run_in_shell(cmd);
                }
                Err(e) => {
                    warn!("Direct command execution failed for '{}': {}", cmd, e);
                }
            }
        }
        CommandPlan::ShellFallback => run_in_shell(cmd),
    }
}

fn cached_path() -> String {
    match context().read() {
        Ok(guard) => guard.path_value.clone(),
        Err(_) => env::var("PATH").unwrap_or_else(|_| DEFAULT_PATH.to_string()),
    }
}

fn shell_path() -> PathBuf {
    match context().read() {
        Ok(guard) => guard.shell_path.clone(),
        Err(_) => detect_user_shell(),
    }
}

fn run_in_shell(cmd: &str) {
    let shell = shell_path();
    let path_value = cached_path();
    let status = Command::new(&shell)
        .arg("-lc")
        .arg(cmd)
        .env("PATH", path_value)
        .status();
    if let Err(e) = status {
        warn!("Shell command execution failed for '{}': {}", cmd, e);
    }
}

fn plan_command(cmd: &str) -> CommandPlan {
    if command_requires_shell(cmd) {
        return CommandPlan::ShellFallback;
    }

    match shell_words::split(cmd) {
        Ok(parts) if !parts.is_empty() => CommandPlan::Direct {
            program: parts[0].clone(),
            args: parts[1..].to_vec(),
        },
        _ => CommandPlan::ShellFallback,
    }
}

fn command_requires_shell(cmd: &str) -> bool {
    let trimmed = cmd.trim();
    if trimmed.is_empty() {
        return false;
    }

    let shell_tokens = [
        "&&", "||", "|", ";", ">>", "<<", ">", "<", "$(", "`", "*", "?", "{", "}", "~",
    ];

    shell_tokens.iter().any(|token| trimmed.contains(token))
}

fn fetch_shell_path(shell_path: &PathBuf) -> Option<String> {
    let output = Command::new(shell_path)
        .arg("-lic")
        .arg("printf %s \"$PATH\"")
        .output();

    match output {
        Ok(output) => {
            if !output.status.success() {
                warn!(
                    "Failed to refresh PATH using shell {}: exit={}",
                    shell_path.display(),
                    output.status
                );
                return None;
            }

            normalize_path_output(&output.stdout)
        }
        Err(err) => {
            warn!(
                "Failed to execute shell {} to refresh PATH: {}",
                shell_path.display(),
                err
            );
            None
        }
    }
}

fn normalize_path_output(stdout: &[u8]) -> Option<String> {
    let path = String::from_utf8_lossy(stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

fn detect_user_shell() -> PathBuf {
    let env_shell = env::var_os("SHELL").map(PathBuf::from);
    let passwd_shell = detect_passwd_shell();
    let shell = resolve_shell_path(env_shell, passwd_shell);
    debug!("Resolved user shell to {}", shell.display());
    shell
}

fn resolve_shell_path(env_shell: Option<PathBuf>, passwd_shell: Option<PathBuf>) -> PathBuf {
    if let Some(shell) = env_shell
        && shell.is_absolute()
        && shell.exists()
    {
        return shell;
    }

    if let Some(shell) = passwd_shell
        && shell.is_absolute()
        && shell.exists()
    {
        return shell;
    }

    let zsh = PathBuf::from("/bin/zsh");
    if zsh.exists() {
        return zsh;
    }

    PathBuf::from("/bin/sh")
}

fn detect_passwd_shell() -> Option<PathBuf> {
    let uid = unsafe { libc::getuid() };
    let mut pwd = unsafe { std::mem::zeroed::<libc::passwd>() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let mut buf_len = 1024usize;

    loop {
        let mut buffer = vec![0u8; buf_len];
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                &mut pwd,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };

        if status == 0 {
            if result.is_null() || pwd.pw_shell.is_null() {
                return None;
            }

            let shell_cstr = unsafe { CStr::from_ptr(pwd.pw_shell) };
            let shell = shell_cstr.to_string_lossy().trim().to_string();
            return if shell.is_empty() {
                None
            } else {
                Some(PathBuf::from(shell))
            };
        }

        if status == libc::ERANGE {
            buf_len *= 2;
            if buf_len > 1024 * 1024 {
                return None;
            }
            continue;
        }

        return None;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandPlan, command_requires_shell, normalize_path_output, plan_command,
        resolve_shell_path,
    };
    use std::path::PathBuf;

    #[test]
    fn shell_detection_prefers_env_shell() {
        let selected = resolve_shell_path(
            Some(PathBuf::from("/bin/sh")),
            Some(PathBuf::from("/bin/zsh")),
        );
        assert_eq!(selected, PathBuf::from("/bin/sh"));
    }

    #[test]
    fn shell_detection_uses_passwd_when_env_missing() {
        let selected = resolve_shell_path(None, Some(PathBuf::from("/bin/sh")));
        assert_eq!(selected, PathBuf::from("/bin/sh"));
    }

    #[test]
    fn shell_detection_falls_back_when_none_available() {
        let selected = resolve_shell_path(
            Some(PathBuf::from("/does/not/exist")),
            Some(PathBuf::from("/also/missing")),
        );
        assert!(
            selected == PathBuf::from("/bin/zsh") || selected == PathBuf::from("/bin/sh"),
            "expected fallback shell, got {}",
            selected.display()
        );
    }

    #[test]
    fn normalize_path_output_trims_newline() {
        let output = normalize_path_output(b"/usr/local/bin:/usr/bin\n");
        assert_eq!(output.as_deref(), Some("/usr/local/bin:/usr/bin"));
    }

    #[test]
    fn normalize_path_output_rejects_empty() {
        assert!(normalize_path_output(b"\n").is_none());
    }

    #[test]
    fn plain_command_uses_direct_execution() {
        let plan = plan_command("kiwi --version");
        assert!(matches!(plan, CommandPlan::Direct { .. }));
    }

    #[test]
    fn shell_features_use_shell_fallback() {
        let plan = plan_command("echo hi | wc -c");
        assert_eq!(plan, CommandPlan::ShellFallback);
    }

    #[test]
    fn quoted_args_are_parsed_for_direct_execution() {
        match plan_command("echo \"hello world\"") {
            CommandPlan::Direct { program, args } => {
                assert_eq!(program, "echo");
                assert_eq!(args, vec!["hello world"]);
            }
            _ => panic!("expected direct command plan"),
        }
    }

    #[test]
    fn parse_failures_fall_back_to_shell() {
        let plan = plan_command("echo \"unterminated");
        assert_eq!(plan, CommandPlan::ShellFallback);
    }

    #[test]
    fn shell_detection_heuristic_flags_common_tokens() {
        assert!(command_requires_shell("echo hi && echo there"));
        assert!(command_requires_shell("echo hi > /tmp/x"));
        assert!(command_requires_shell("echo $(date)"));
        assert!(!command_requires_shell("kiwi --version"));
    }
}
