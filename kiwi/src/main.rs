pub mod a11y;
pub mod cli;
mod control;
mod event_tap;
pub mod ffi;
pub mod hotkey;
pub mod input;
pub mod manager;
mod menubar;
mod menubar_items;
mod shell_runtime;
mod translate;
pub mod window;
use crate::cli::LogArgs;
use crate::cli::error::{CliError, CliResult};
use crate::control::{ControlState, default_socket_path, spawn_control_server};
use crate::event_tap::{CGEventTap, CGEventType};
use crate::input::{USER_DATA, from_cg_code, from_system_defined_event, get_character_from_event};
use crate::manager::RELOAD_REQUESTED;
use crate::window::focused::init_focus_observer;
use clap::Parser;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CallbackResult, EventField,
};
use objc2_app_kit::NSApplication;
use objc2_foundation::MainThreadMarker;
use kiwi_parser::Config;
use miette::{Report, miette};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{process, thread};
use tracing::{error, info};

fn main() {
    let cli = cli::Cli::parse();

    let result = match cli.command {
        Some(command) => cli::run(command),
        None => run_daemon(cli.config, cli.log),
    };

    if let Err(err) = result {
        if let Some(message) = err.message {
            eprintln!("{message}");
        }
        process::exit(err.code);
    }
}

pub(crate) fn run_daemon(
    config_path_override: Option<PathBuf>,
    log_args: LogArgs,
) -> CliResult<()> {
    init_tracing(log_args);

    let config_path = resolve_config_path(config_path_override)
        .map_err(|e| CliError::new(format!("configuration file not found: {e}")))?;
    let config_path = config_path.canonicalize().map_err(|e| {
        CliError::new(format!(
            "failed to resolve config path {}: {e}",
            config_path.display()
        ))
    })?;

    let config = parse_config_from_path(&config_path).map_err(|e| {
        CliError::new(format!(
            "failed to parse config {}: {e:?}",
            config_path.display()
        ))
    })?;

    let cwd = resolve_process_cwd(&config.cwd, &config_path)?;
    std::env::set_current_dir(&cwd).map_err(|e| {
        CliError::new(format!(
            "failed to set working directory to {}: {e}",
            cwd.display()
        ))
    })?;

    let mtm = MainThreadMarker::new().expect("Must run on main thread");
    let app = NSApplication::sharedApplication(mtm);

    if !a11y::is_process_trusted() {
        return Err(CliError::new(
            "Please grant accessibility permissions before running kiwi daemon",
        ));
    }

    shell_runtime::init_shell_context();

    thread::spawn(init_focus_observer);

    manager::init_action_executor();
    menubar::init(config.menubar);

    if let Some(layout_id) = &config.layout {
        println!("Setting layout to: {layout_id}");
        translate::set_layout(layout_id);
    }

    let manager = manager::setup_manager(&config).map_err(|e| {
        CliError::new(format!(
            "failed to build hotkey manager from {}: {e}",
            config_path.display()
        ))
    })?;
    let manager = Arc::new(Mutex::new(manager));
    manager::set_shared_manager(manager.clone());
    let manager_ref = manager.clone();
    let reload_path = config_path.clone();

    let control_state = ControlState {
        manager: manager.clone(),
        config_path: config_path.clone(),
        started_at: std::time::Instant::now(),
        socket_path: default_socket_path()?,
    };
    spawn_control_server(control_state)?;

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::SystemDefined,
        ],
        move |_proxy, type_, event| {
            let user_data = event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
            if user_data == USER_DATA {
                return CallbackResult::Keep;
            }

            let flags = event.get_flags();
            let (key, is_down) = match type_ {
                CGEventType::KeyDown | CGEventType::KeyUp => {
                    let key_code =
                        event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    let char = get_character_from_event(event);
                    let key = match from_cg_code(key_code as u16, char) {
                        Some(k) => k,
                        None => return CallbackResult::Keep,
                    };
                    (key, matches!(type_, CGEventType::KeyDown))
                }
                CGEventType::SystemDefined => match from_system_defined_event(event) {
                    Some((key, is_down)) => (key, is_down),
                    None => return CallbackResult::Keep,
                },
                _ => return CallbackResult::Keep,
            };

            let modifiers = input::modifiers_from_cg_flags(flags);
            let app_name = crate::window::get_focused_app();

            match manager::intercept_decision(&key, modifiers, is_down) {
                manager::InterceptDecision::ProcessNormally => {}
                manager::InterceptDecision::KeepWithoutProcessing => {
                    return CallbackResult::Keep;
                }
                manager::InterceptDecision::DropWithoutProcessing => {
                    return CallbackResult::Drop;
                }
            }

            if let Ok(mut mgr) = manager_ref.lock() {
                let result = mgr.process(key, modifiers, is_down, &app_name);
                let handled = result.handled;
                if let Some(action) = result.action {
                    manager::dispatch_action(action);
                }

                if RELOAD_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("Reloading configuration...");

                    RELOAD_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
                    shell_runtime::refresh_path_cache();
                    match parse_config_from_path(&reload_path) {
                        Ok(new_config) => match manager::setup_manager(&new_config) {
                            Ok(new_manager) => {
                                *mgr = new_manager;
                                manager::clear_window_state();
                                info!("Configuration reloaded.");
                            }
                            Err(e) => {
                                error!("Failed to build manager from reloaded config: {e}");
                            }
                        },
                        Err(e) => {
                            error!("Failed to reload config:");
                            println!("{e:?}");
                        }
                    }
                }

                if handled {
                    return CallbackResult::Drop;
                }
            }

            CallbackResult::Keep
        },
    )
    .map_err(|_| CliError::new("Failed to create event tap. Check permissions."))?;

    let source = tap
        .mach_port()
        .create_runloop_source(0)
        .map_err(|_| CliError::new("Failed to create runloop source"))?;

    let runloop = CFRunLoop::get_current();
    runloop.add_source(&source, unsafe { kCFRunLoopCommonModes });

    info!("Kiwi is running...");
    tap.enable();
    app.run();

    Ok(())
}

pub(crate) fn resolve_config_path(
    override_path: Option<PathBuf>,
) -> Result<PathBuf, std::io::Error> {
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let cwd = std::env::current_dir()?;
    resolve_config_path_inner(override_path, home, cwd)
}

fn resolve_config_path_inner(
    override_path: Option<PathBuf>,
    home: Option<PathBuf>,
    cwd: PathBuf,
) -> Result<PathBuf, std::io::Error> {
    if let Some(path) = override_path {
        if path.exists() {
            return Ok(path);
        }

        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("--config path not found: {}", path.display()),
        ));
    }

    if let Some(home) = home {
        let home_path = home.join(".kiwi").join("config.toml");
        if home_path.exists() {
            return Ok(home_path);
        }
    }

    let cwd_path = cwd.join("config.toml");
    if cwd_path.exists() {
        return Ok(cwd_path);
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "checked ~/.kiwi/config.toml and ./config.toml",
    ))
}

pub(crate) fn parse_config_from_path(path: &Path) -> Result<Config, Report> {
    let toml_str = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read config file {}: {e}", path.display()))?;

    kiwi_parser::parse_config(&toml_str, path.to_path_buf())
}

fn resolve_process_cwd(raw: &str, config_path: &Path) -> CliResult<PathBuf> {
    let expanded = expand_config_path_variables(raw, |name| std::env::var(name).ok())?;
    let path = PathBuf::from(expanded);

    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path))
    }
}

fn expand_config_path_variables(
    raw: &str,
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> CliResult<String> {
    expand_path_variables(raw, |name| {
        if name == "KIWI" {
            lookup("HOME").map(|home| format!("{home}/.kiwi"))
        } else {
            lookup(name)
        }
    })
}

fn expand_path_variables(
    raw: &str,
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> CliResult<String> {
    let chars: Vec<char> = raw.chars().collect();
    let mut expanded = String::with_capacity(raw.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index] != '$' {
            expanded.push(chars[index]);
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        let name = if chars.get(index) == Some(&'{') {
            index += 1;
            let name_start = index;
            while index < chars.len() && chars[index] != '}' {
                index += 1;
            }
            if index == chars.len() {
                return Err(CliError::new(format!(
                    "invalid cwd '{raw}': unclosed variable starting at byte {start}"
                )));
            }
            let name: String = chars[name_start..index].iter().collect();
            index += 1;
            name
        } else {
            let name_start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            if name_start == index {
                expanded.push('$');
                continue;
            }
            chars[name_start..index].iter().collect()
        };

        if name.is_empty() {
            return Err(CliError::new(format!(
                "invalid cwd '{raw}': environment variable name cannot be empty"
            )));
        }
        let value = lookup(&name).ok_or_else(|| {
            CliError::new(format!(
                "cannot resolve cwd '{raw}': environment variable ${name} is not set"
            ))
        })?;
        expanded.push_str(&value);
    }

    Ok(expanded)
}

fn init_tracing(log_args: LogArgs) {
    let env = if log_args.quiet {
        tracing_subscriber::EnvFilter::new("error")
    } else if log_args.trace {
        tracing_subscriber::EnvFilter::new("trace")
    } else if log_args.debug {
        tracing_subscriber::EnvFilter::new("debug")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };

    let _ = tracing_subscriber::fmt().with_env_filter(env).try_init();
}

#[cfg(test)]
mod tests {
    use super::{expand_config_path_variables, expand_path_variables, resolve_process_cwd};
    use std::path::{Path, PathBuf};

    #[test]
    fn expands_plain_and_braced_variables() {
        let result = expand_path_variables("$HOME/${PROJECT}", |name| match name {
            "HOME" => Some("/Users/test".into()),
            "PROJECT" => Some("code".into()),
            _ => None,
        })
        .expect("variables should expand");

        assert_eq!(result, "/Users/test/code");
    }

    #[test]
    fn undefined_variables_are_errors() {
        assert!(expand_path_variables("$MISSING", |_| None).is_err());
    }

    #[test]
    fn relative_cwd_is_relative_to_config_directory() {
        let path = resolve_process_cwd("projects", Path::new("/tmp/.kiwi/config.toml"))
            .expect("cwd should resolve");
        assert_eq!(path, PathBuf::from("/tmp/.kiwi/projects"));
    }

    #[test]
    fn kiwi_is_a_builtin_alias_for_home_dot_kiwi() {
        let expanded = expand_config_path_variables("$KIWI", |name| match name {
            "HOME" => Some("/Users/test".into()),
            "KIWI" => Some("/should/not/be/used".into()),
            _ => None,
        })
        .expect("$KIWI should resolve");

        assert_eq!(expanded, "/Users/test/.kiwi");
    }
}
