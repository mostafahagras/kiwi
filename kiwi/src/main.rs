pub mod a11y;
pub mod cli;
pub mod ffi;
pub mod hotkey;
pub mod input;
pub mod manager;
mod translate;
pub mod window;

use crate::cli::error::{CliError, CliResult};
use crate::input::{USER_DATA, from_cg_code, get_character_from_event};
use crate::manager::RELOAD_REQUESTED;
use crate::window::focused::init_focus_observer;
use clap::Parser;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
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
        None => run_daemon(None),
    };

    if let Err(err) = result {
        if let Some(message) = err.message {
            eprintln!("{message}");
        }
        process::exit(err.code);
    }
}

pub(crate) fn run_daemon(config_path_override: Option<PathBuf>) -> CliResult<()> {
    init_tracing();

    if !a11y::is_process_trusted() {
        return Err(CliError::new(
            "Please grant accessibility permissions before running kiwi daemon",
        ));
    }

    thread::spawn(init_focus_observer);

    let config_path = resolve_config_path(config_path_override)
        .map_err(|e| CliError::new(format!("configuration file not found: {e}")))?;

    let config = parse_config_from_path(&config_path)
        .map_err(|e| CliError::new(format!("failed to parse config {}: {e:?}", config_path.display())))?;

    manager::init_action_executor();

    if let Some(layout_id) = &config.layout {
        println!("Setting layout to: {layout_id}");
        translate::set_layout(layout_id);
    }

    let manager = manager::setup_manager(&config);
    let manager = Arc::new(Mutex::new(manager));
    let manager_ref = manager.clone();
    let reload_path = config_path.clone();

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::KeyUp],
        move |_proxy, type_, event| {
            let user_data = event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
            if user_data == USER_DATA {
                return CallbackResult::Keep;
            }

            let is_down = matches!(type_, CGEventType::KeyDown);
            let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            let flags = event.get_flags();

            let char = get_character_from_event(event);
            let key = match from_cg_code(key_code as u16, char) {
                Some(k) => k,
                None => return CallbackResult::Keep,
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

                    match parse_config_from_path(&reload_path) {
                        Ok(new_config) => {
                            *mgr = manager::setup_manager(&new_config);
                            manager::clear_window_state();
                            RELOAD_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
                            info!("Configuration reloaded.");
                        }
                        Err(e) => {
                            error!("Failed to reload config: {e:?}");
                            process::exit(1);
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
    CFRunLoop::run_current();

    Ok(())
}

pub(crate) fn resolve_config_path(override_path: Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
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

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();
}
