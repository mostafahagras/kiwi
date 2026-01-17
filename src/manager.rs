use std::collections::HashMap;
use std::process::Command as ShellCommand;
use crate::parser::{Config, KeyCombination, Layer, Command, SnapSide, LayerItem};
use crate::hotkey::{HotkeyManager, HotkeyStep};
use crate::window;

use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};
use core_graphics_types::geometry::CGRect;

static WINDOW_STATE: OnceLock<Mutex<HashMap<u32, CGRect>>> = OnceLock::new();

fn get_window_state() -> &'static Mutex<HashMap<u32, CGRect>> {
    WINDOW_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn launch_app(name: &str) {
    ShellCommand::new("open").arg("-a").arg(name).spawn().ok();
}

pub fn snap_window(side: SnapSide) {
    let bounds = window::get_main_display_bounds();
    let sw = bounds.size.width;
    let sh = bounds.size.height;
    let sx = bounds.origin.x;
    let sy = bounds.origin.y;

    let focused = match window::get_focused_window() {
        Some(w) => w,
        None => return,
    };

    if side == SnapSide::Restore {
        if let Ok(state) = get_window_state().lock() {
            if let Some(prev_frame) = state.get(&focused.id) {
                window::set_focused_window_bounds(
                    prev_frame.origin.x,
                    prev_frame.origin.y,
                    prev_frame.size.width,
                    prev_frame.size.height,
                );
                return;
            }
        }
        return;
    }

    if side == SnapSide::Fullscreen || side == SnapSide::Full {
        if window::ops::toggle_native_fullscreen() {
            return;
        }
    }

    // Save state before snapping
    if let Ok(mut state) = get_window_state().lock() {
        state.entry(focused.id).or_insert(focused.frame);
    }

    let (mut x, mut y, mut w, mut h) = (sx, sy, sw, sh);

    match side {
        SnapSide::Left => w = sw / 2.0,
        SnapSide::Right => { x = sx + sw / 2.0; w = sw / 2.0; }
        SnapSide::Top => h = sh / 2.0,
        SnapSide::Bottom => { y = sy + sh / 2.0; h = sh / 2.0; }
        SnapSide::Full | SnapSide::Fullscreen | SnapSide::Maximize => {}
        SnapSide::MaximizeHeight => { x = focused.frame.origin.x; w = focused.frame.size.width; }
        SnapSide::MaximizeWidth => { y = focused.frame.origin.y; h = focused.frame.size.height; }
        SnapSide::Center => {
            w = focused.frame.size.width;
            h = focused.frame.size.height;
            x = sx + (sw - w) / 2.0;
            y = sy + (sh - h) / 2.0;
        }
        SnapSide::TopLeft => { w = sw / 2.0; h = sh / 2.0; }
        SnapSide::TopRight => { x = sx + sw / 2.0; w = sw / 2.0; h = sh / 2.0; }
        SnapSide::BottomLeft => { y = sy + sh / 2.0; w = sw / 2.0; h = sh / 2.0; }
        SnapSide::BottomRight => { x = sx + sw / 2.0; y = sy + sh / 2.0; w = sw / 2.0; h = sh / 2.0; }
        
        // Thirds
        SnapSide::FirstThird => w = sw / 3.0,
        SnapSide::CenterThird => { x = sx + sw / 3.0; w = sw / 3.0; }
        SnapSide::LastThird => { x = sx + 2.0 * sw / 3.0; w = sw / 3.0; }
        SnapSide::FirstTwoThirds => w = 2.0 * sw / 3.0,
        SnapSide::LastTwoThirds => { x = sx + sw / 3.0; w = 2.0 * sw / 3.0; }

        // Fourths
        SnapSide::FirstFourth => w = sw / 4.0,
        SnapSide::SecondFourth => { x = sx + sw / 4.0; w = sw / 4.0; }
        SnapSide::ThirdFourth => { x = sx + 2.0 * sw / 4.0; w = sw / 4.0; }
        SnapSide::LastFourth => { x = sx + 3.0 * sw / 4.0; w = sw / 4.0; }

        // Sixths
        SnapSide::TopLeftSixth => { w = sw / 3.0; h = sh / 2.0; }
        SnapSide::TopCenterSixth => { x = sx + sw / 3.0; w = sw / 3.0; h = sh / 2.0; }
        SnapSide::TopRightSixth => { x = sx + 2.0 * sw / 3.0; w = sw / 3.0; h = sh / 2.0; }
        SnapSide::BottomLeftSixth => { y = sy + sh / 2.0; w = sw / 3.0; h = sh / 2.0; }
        SnapSide::BottomCenterSixth => { x = sx + sw / 3.0; y = sy + sh / 2.0; w = sw / 3.0; h = sh / 2.0; }
        SnapSide::BottomRightSixth => { x = sx + 2.0 * sw / 3.0; y = sy + sh / 2.0; w = sw / 3.0; h = sh / 2.0; }

        SnapSide::MoveUp => { x = focused.frame.origin.x; y = sy; w = focused.frame.size.width; h = focused.frame.size.height; }
        SnapSide::MoveDown => { x = focused.frame.origin.x; y = sy + sh - focused.frame.size.height; w = focused.frame.size.width; h = focused.frame.size.height; }
        SnapSide::MoveLeft => { x = sx; y = focused.frame.origin.y; w = focused.frame.size.width; h = focused.frame.size.height; }
        SnapSide::MoveRight => { x = sx + sw - focused.frame.size.width; y = focused.frame.origin.y; w = focused.frame.size.width; h = focused.frame.size.height; }

        SnapSide::ReasonableSize => {
            w = (sw * 0.6).min(1025.0);
            h = (sh * 0.6).min(900.0);
            x = sx + (sw - w) / 2.0;
            y = sy + (sh - h) / 2.0;
        }
        _ => return,
    }

    if window::set_focused_window_bounds(x, y, w, h) {
    }
}

pub fn setup_manager(config: &Config) -> HotkeyManager {
    let mut manager = HotkeyManager::new();

    // 1. Top-level binds
    for (key_str, cmd) in &config.binds {
        match KeyCombination::from_str_with_context(key_str, &config.modifiers) {
            Ok(combo) => {
                register_command(&mut manager, vec![HotkeyStep::new(combo.key, combo.modifiers)], None, cmd.clone(), config);
            }
            Err(e) => {
                eprintln!("Failed to parse bind '{}': {}", key_str, e);
            }
        }
    }

    // 2. [layer.<name>] sections
    for (_name, layer) in &config.layer {
        if let Some(activate_str) = &layer.activate {
            match KeyCombination::from_str_with_context(activate_str, &config.modifiers) {
                Ok(combo) => {
                    register_layer(&mut manager, vec![HotkeyStep::new(combo.key, combo.modifiers)], None, layer.clone(), config);
                }
                Err(e) => {
                    eprintln!("Failed to parse layer activation '{}': {}", activate_str, e);
                }
            }
        }
    }

    // 3. [app.<name>] sections
    for (app_alias, app_config) in &config.app {
        let app_name = config.apps.get(app_alias).cloned().unwrap_or_else(|| app_alias.clone());
        for (key_str, item) in &app_config.items {
            match item {
                LayerItem::Command(cmd) => {
                    if let Ok(combo) = KeyCombination::from_str_with_context(key_str, &config.modifiers) {
                        register_command(&mut manager, vec![HotkeyStep::new(combo.key, combo.modifiers)], Some(app_name.clone()), cmd.clone(), config);
                    }
                }
                LayerItem::Layer(l) => {
                    let trigger_str = l.activate.as_ref().unwrap_or(key_str);
                    if let Ok(combo) = KeyCombination::from_str_with_context(trigger_str, &config.modifiers) {
                         register_layer(&mut manager, vec![HotkeyStep::new(combo.key, combo.modifiers)], Some(app_name.clone()), l.clone(), config);
                    }
                }
            }
        }
    }

    manager
}

pub fn register_layer(manager: &mut HotkeyManager, prefix: Vec<HotkeyStep>, context: Option<String>, layer: Layer, config: &Config) {
    for (key_str, item) in layer.items {
        match item {
            LayerItem::Command(cmd) => {
                if let Ok(combo) = KeyCombination::from_str_with_context(&key_str, &config.modifiers) {
                    let mut seq = prefix.clone();
                    seq.push(HotkeyStep::new(combo.key, combo.modifiers));
                    register_command(manager, seq, context.clone(), cmd, config);
                }
            }
            LayerItem::Layer(mut sub_layer) => {
                let trigger_str = sub_layer.activate.take().unwrap_or(key_str);
                if let Ok(combo) = KeyCombination::from_str_with_context(&trigger_str, &config.modifiers) {
                    let mut new_prefix = prefix.clone();
                    new_prefix.push(HotkeyStep::new(combo.key, combo.modifiers));
                    register_layer(manager, new_prefix, context.clone(), sub_layer, config);
                }
            }
        }
    }
}

pub fn register_command(manager: &mut HotkeyManager, sequence: Vec<HotkeyStep>, context: Option<String>, mut command: Command, config: &Config) {
    if let Command::Open(app_name) = &mut command {
        if let Some(resolved) = config.apps.get(app_name) {
            *app_name = resolved.clone();
        }
    }

    match command {
        Command::Open(app) => {
            manager.bind(sequence, context, move || launch_app(&app));
        }
        Command::Snap(side) => {
            manager.bind(sequence, context, move || snap_window(side));
        }
        Command::Remap(keys) => {
            // Parse the keys to send (e.g. "Ctrl+T")
            if let Ok(combo) = KeyCombination::from_str_with_context(&keys, &config.modifiers) {
                manager.bind(sequence, context, move || {
                    crate::input::send_key_combination(&combo);
                });
            }
        }
        Command::Shell(cmd) => {
            manager.bind(sequence, context, move || {
                ShellCommand::new("sh").arg("-c").arg(&cmd).spawn().ok();
            });
        }
        Command::Reload => {
            manager.bind(sequence, context, || {
                RELOAD_REQUESTED.store(true, Ordering::SeqCst);
            });
        }
    }
}
