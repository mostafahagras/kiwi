pub mod ffi;
pub mod window;
pub mod parser;
pub mod hotkey;
pub mod manager;
pub mod a11y;
pub mod input;

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};
use crate::input::USER_DATA;
use crate::parser::{Config, Key, Modifiers};
use crate::ffi::{CGEventKeyboardGetUnicodeString};
use crate::manager::RELOAD_REQUESTED;
use tracing::{info, error, warn};

fn get_character_from_event(event: &CGEvent) -> Option<char> {
    let mut actual_length = 0;
    let mut buf = [0u16; 4];

    unsafe {
        CGEventKeyboardGetUnicodeString(
            std::mem::transmute_copy(event),
            buf.len() as u64,
            &mut actual_length,
            buf.as_mut_ptr(),
        );
    }

    if actual_length > 0 {
        String::from_utf16(&buf[..actual_length as usize])
            .ok()
            .and_then(|s| s.chars().next())
    } else {
        None
    }
}

fn load_config() -> Config {
    let mut config_path = PathBuf::new();
    
    // Check ~/.kiwi/config.toml
    if let Ok(home) = std::env::var("HOME") {
        let mut p = PathBuf::from(home);
        p.push(".kiwi");
        p.push("config.toml");
        if p.exists() {
            config_path = p;
        }
    }

    // Fallback to local config.toml if home config doesn't exist or HOME is not set
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
    let toml_str = std::fs::read_to_string(config_path).expect("Failed to read config file");
    toml::from_str(&toml_str).expect("Failed to parse config")
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    if !a11y::is_process_trusted() {
        warn!("Please grant accessibility permissions.");
        process::exit(1);
    }

    let config = load_config();
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
            let key = Key::from_cg_code(key_code as u16, char);
            let modifiers = Modifiers::from_cg_flags(flags);
            let app_name = crate::window::get_frontmost_app_name().unwrap_or_default();

            if let Ok(mut mgr) = manager_ref.lock() {
                let handled = mgr.process(key, modifiers, is_down, &app_name);

                if RELOAD_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                    info!("Reloading configuration...");
                    let new_config = load_config();
                    *mgr = manager::setup_manager(&new_config);
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

    let source = tap.mach_port().create_runloop_source(0).expect("Failed to create source");
    let runloop = CFRunLoop::get_current();
    runloop.add_source(&source, unsafe { kCFRunLoopCommonModes });

    info!("Kiwi is running...");
    tap.enable();
    CFRunLoop::run_current();
}
