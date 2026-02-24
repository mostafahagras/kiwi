pub mod a11y;
pub mod ffi;
pub mod hotkey;
pub mod input;
pub mod manager;
mod translate;
pub mod window;

use crate::input::{USER_DATA, from_cg_code, get_character_from_event};
use crate::manager::RELOAD_REQUESTED;
use crate::window::focused::init_focus_observer;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CallbackResult, EventField,
};
use kiwi_parser::Config;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::{process, thread};
use tracing::{error, info, warn};

fn load_config() -> Config {
    let mut config_path = PathBuf::new();

    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".kiwi");
        p.push("config.toml");
        if p.exists() {
            config_path = p;
        }
    }

    if config_path.as_os_str().is_empty() || !config_path.exists() {
        let p = PathBuf::from("config.toml");
        if p.exists() {
            config_path = p;
        }
    }

    if !config_path.exists() {
        error!("Configuration file not found. Checked ~/.kiwi/config.toml and ./config.toml");
        process::exit(1);
    }

    info!("Loading config from: {config_path:?}");

    let toml_str =
        std::fs::read_to_string(config_path.clone()).expect("Failed to read config file");
    match kiwi_parser::parse_config(&toml_str, config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{:?}", e);
            process::exit(1);
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    if !a11y::is_process_trusted() {
        warn!("Please grant accessibility permissions.");
        process::exit(1);
    }
    thread::spawn(init_focus_observer);
    let config = load_config();
    manager::init_action_executor();
    if let Some(layout_id) = &config.layout {
        println!("Setting layout to: {layout_id}");
        translate::set_layout(layout_id);
    }
    let manager = manager::setup_manager(&config);
    let manager = Arc::new(Mutex::new(manager));
    let manager_ref = manager.clone();

    let tap = match CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::KeyUp],
        move |_proxy, type_, event| {
            // Ignore events sent by Kiwi to prevent infinite loops
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

            if let Ok(mut mgr) = manager_ref.lock() {
                let result = mgr.process(key, modifiers, is_down, &app_name);
                let handled = result.handled;
                if let Some(action) = result.action {
                    manager::dispatch_action(action);
                }

                if RELOAD_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("Reloading configuration...");
                    let new_config = load_config();
                    *mgr = manager::setup_manager(&new_config);
                    manager::clear_window_state();
                    RELOAD_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
                    info!("Configuration reloaded.");
                }

                if handled {
                    return CallbackResult::Drop;
                }
            }

            CallbackResult::Keep
        },
    ) {
        Ok(tap) => tap,
        Err(_) => {
            error!("Failed to create event tap. Check permissions.");
            process::exit(1);
        }
    };

    let source = tap
        .mach_port()
        .create_runloop_source(0)
        .expect("Failed to create source");
    let runloop = CFRunLoop::get_current();
    runloop.add_source(&source, unsafe { kCFRunLoopCommonModes });

    info!("Kiwi is running...");
    tap.enable();
    CFRunLoop::run_current();
}
