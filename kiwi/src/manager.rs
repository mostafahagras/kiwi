use crate::hotkey::{HotkeyManager, HotkeyStep};
use crate::window;
use kiwi_parser::{Action, Config, Layer, Resize, Snap};
use std::collections::{HashMap, HashSet};
use std::process::Command as ShellCommand;
use tracing::{debug, error, info};

use core_graphics_types::geometry::CGRect;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

static WINDOW_STATE: OnceLock<Mutex<HashMap<u32, CGRect>>> = OnceLock::new();
const WINDOW_STATE_MAX_ENTRIES: usize = 256;

fn get_window_state() -> &'static Mutex<HashMap<u32, CGRect>> {
    WINDOW_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn clear_window_state() {
    if let Ok(mut state) = get_window_state().lock() {
        state.clear();
    }
}

fn prune_window_state() {
    let live_ids: HashSet<u32> = window::list::current_window_ids().into_iter().collect();
    if let Ok(mut state) = get_window_state().lock() {
        trim_window_state(&mut state, &live_ids, WINDOW_STATE_MAX_ENTRIES);

        debug!("window state cache size: {}", state.len());
    }
}

fn trim_window_state(state: &mut HashMap<u32, CGRect>, live_ids: &HashSet<u32>, max_entries: usize) {
    state.retain(|window_id, _| live_ids.contains(window_id));

    while state.len() > max_entries {
        if let Some(first_key) = state.keys().next().copied() {
            state.remove(&first_key);
        } else {
            break;
        }
    }
}

pub fn launch_app(name: &str) {
    info!("Launching app: {name}");
    ShellCommand::new("open").arg("-a").arg(name).spawn().ok();
}

pub fn handle_action(action: &Action) {
    match action {
        Action::Shell(cmd) => {
            info!("Executing shell command: {cmd}");
            if let Some(app_name) = cmd.strip_prefix("open -a ") {
                launch_app(app_name.trim());
            } else {
                ShellCommand::new("sh").arg("-c").arg(&cmd).spawn().ok();
            }
        }
        Action::Remap(binding) => {
            info!("Remapping to: {binding:?}");
            crate::input::send_key_combination(binding);
        }
        Action::Snap(side) => {
            snap_window(side.clone());
        }
        Action::Reload => {
            info!("Reload requested");
            RELOAD_REQUESTED.store(true, Ordering::SeqCst);
        }
        Action::Quit => {
            info!("Quitting Kiwi...");
            std::process::exit(0);
        }
        Action::SleepFor(duration) => {
            debug!("Sleeping for {:?}", duration);
            std::thread::sleep(*duration);
        }
        Action::Sequence(actions) => {
            for a in actions {
                handle_action(a);
            }
        }
        Action::Resize(resize) => resize_window(resize),
        _ => {
            error!("Action not yet fully implemented: {:?}", action);
        }
    }
}

pub fn snap_window(side: Snap) {
    let bounds = window::get_main_display_bounds();
    let sw = bounds.size.width;
    let sh = bounds.size.height;
    let sx = bounds.origin.x;
    let sy = bounds.origin.y;

    let focused = match window::get_focused_window() {
        Some(w) => w,
        None => return,
    };

    if side == Snap::Restore {
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

    if side == Snap::Fullscreen {
        if window::ops::toggle_native_fullscreen() {
            return;
        }
    }

    // Save state before snapping
    if let Ok(mut state) = get_window_state().lock() {
        state.entry(focused.id).or_insert(focused.frame);
    }
    prune_window_state();

    let (mut x, mut y, mut w, mut h) = (sx, sy, sw, sh);

    match side {
        Snap::Maximize => {}
        Snap::AlmostMaximize => {
            let margin = 20.0;
            x = sx + margin;
            y = sy + margin;
            w = sw - 2.0 * margin;
            h = sh - 2.0 * margin;
        }
        Snap::MaximizeWidth => {
            y = focused.frame.origin.y;
            h = focused.frame.size.height;
        }
        Snap::MaximizeHeight => {
            x = focused.frame.origin.x;
            w = focused.frame.size.width;
        }
        Snap::LeftHalf => w = sw / 2.0,
        Snap::CenterHalf => {
            x = sx + sw / 4.0;
            w = sw / 2.0;
        }
        Snap::RightHalf => {
            x = sx + sw / 2.0;
            w = sw / 2.0;
        }

        Snap::FirstThird => w = sw / 3.0,
        Snap::CenterThird => {
            x = sx + sw / 3.0;
            w = sw / 3.0;
        }
        Snap::LastThird => {
            x = sx + 2.0 * sw / 3.0;
            w = sw / 3.0;
        }

        Snap::FirstFourth => w = sw / 4.0,
        Snap::SecondFourth => {
            x = sx + sw / 4.0;
            w = sw / 4.0;
        }
        Snap::ThirdFourth => {
            x = sx + 2.0 * sw / 4.0;
            w = sw / 4.0;
        }
        Snap::LastFourth => {
            x = sx + 3.0 * sw / 4.0;
            w = sw / 4.0;
        }

        Snap::TopHalf => h = sh / 2.0,
        Snap::MiddleHalf => {
            y = sy + sh / 4.0;
            h = sh / 2.0;
        }
        Snap::BottomHalf => {
            y = sy + sh / 2.0;
            h = sh / 2.0;
        }

        Snap::TopThird => h = sh / 3.0,
        Snap::MiddleThird => {
            y = sy + sh / 3.0;
            h = sh / 3.0;
        }
        Snap::BottomThird => {
            y = sy + 2.0 * sh / 3.0;
            h = sh / 3.0;
        }

        Snap::TopLeftQuarter => {
            w = sw / 2.0;
            h = sh / 2.0;
        }
        Snap::TopCenterQuarter => {
            x = sx + sw / 4.0;
            w = sw / 2.0;
            h = sh / 2.0;
        }
        Snap::TopRightQuarter => {
            x = sx + sw / 2.0;
            w = sw / 2.0;
            h = sh / 2.0;
        }
        Snap::MiddleLeftQuarter => {
            w = sw / 2.0;
            y = sy + sh / 4.0;
            h = sh / 2.0;
        }
        Snap::MiddleRightQuarter => {
            x = sx + sw / 2.0;
            y = sy + sh / 4.0;
            h = sh / 2.0;
        }
        Snap::BottomLeftQuarter => {
            w = sw / 2.0;
            y = sy + sh / 2.0;
            h = sh / 2.0;
        }
        Snap::BottomCenterQuarter => {
            x = sx + sw / 4.0;
            w = sw / 2.0;
            y = sy + sh / 2.0;
            h = sh / 2.0;
        }
        Snap::BottomRightQuarter => {
            x = sx + sw / 2.0;
            y = sy + sh / 2.0;
            w = sw / 2.0;
            h = sh / 2.0;
        }

        Snap::TopLeftSixth => {
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::TopCenterSixth => {
            x = sx + sw / 3.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::TopRightSixth => {
            x = sx + 2.0 * sw / 3.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::MiddleLeftSixth => {
            w = sw / 3.0;
            y = sy + sh / 2.0;
            h = sh / 2.0;
        }
        Snap::MiddleCenterSixth => {
            x = sx + sw / 3.0;
            y = sy + sh / 2.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::MiddleRightSixth => {
            x = sx + 2.0 * sw / 3.0;
            y = sy + sh / 2.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::BottomLeftSixth => {
            y = sy + sh / 2.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::BottomCenterSixth => {
            x = sx + sw / 3.0;
            y = sy + sh / 2.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }
        Snap::BottomRightSixth => {
            x = sx + 2.0 * sw / 3.0;
            y = sy + sh / 2.0;
            w = sw / 3.0;
            h = sh / 2.0;
        }

        Snap::Left => {
            x = sx;
            y = focused.frame.origin.y;
            w = focused.frame.size.width;
            h = focused.frame.size.height;
        }
        Snap::Right => {
            x = sx + sw - focused.frame.size.width;
            y = focused.frame.origin.y;
            w = focused.frame.size.width;
            h = focused.frame.size.height;
        }
        Snap::Top => {
            x = focused.frame.origin.x;
            y = sy;
            w = focused.frame.size.width;
            h = focused.frame.size.height;
        }
        Snap::Bottom => {
            x = focused.frame.origin.x;
            y = sy + sh - focused.frame.size.height;
            w = focused.frame.size.width;
            h = focused.frame.size.height;
        }
        _ => return,
    }

    if window::set_focused_window_bounds(x, y, w, h) {
        info!("Snapped window to {:?}", side);
    }
}

pub fn resize_window(resize: &Resize) {
    let focused = match window::get_focused_window() {
        Some(w) => w,
        None => return,
    };

    let (mut x, mut y, mut w, mut h) = (
        focused.frame.origin.x,
        focused.frame.origin.y,
        focused.frame.size.width,
        focused.frame.size.height,
    );

    match resize {
        Resize::IncreaseWidth => {
            x -= 10.0;
            w += 20.0;
        }
        Resize::IncreaseHeight => {
            y -= 10.0;
            h += 20.0;
        }
        Resize::IncreaseBoth => {
            x -= 10.0;
            y -= 10.0;
            w += 20.0;
            h += 20.0;
        }
        Resize::DecreaseWidth => {
            x += 10.0;
            w -= 20.0;
        }
        Resize::DecreaseHeight => {
            y += 10.0;
            h -= 20.0;
        }
        Resize::DecreaseBoth => {
            x += 10.0;
            y += 10.0;
            w -= 20.0;
            h -= 20.0;
        }
    }

    if window::set_focused_window_bounds(x, y, w, h) {
        info!("Resized window to {:?}", resize);
    }
}

pub fn setup_manager(config: &Config) -> HotkeyManager {
    let mut manager = HotkeyManager::new();

    // 1. Global binds
    for (binding, action) in &config.global_binds {
        let action = action.clone();
        manager.bind(
            vec![HotkeyStep::new(binding.key.clone(), binding.modifiers)],
            None,
            move || handle_action(&action),
        );
    }

    // 2. Layers
    for (trigger, layer) in &config.layers {
        register_layer(
            &mut manager,
            vec![HotkeyStep::new(trigger.key.clone(), trigger.modifiers)],
            None,
            layer,
        );
    }

    // 3. Apps
    for (app_name, app_config) in &config.apps {
        // App binds
        for (binding, action) in &app_config.binds {
            let action = action.clone();
            manager.bind(
                vec![HotkeyStep::new(binding.key.clone(), binding.modifiers)],
                Some(app_name.clone()),
                move || handle_action(&action),
            );
        }
        // App layers
        for (trigger, layer) in &app_config.children {
            register_layer(
                &mut manager,
                vec![HotkeyStep::new(trigger.key.clone(), trigger.modifiers)],
                Some(app_name.clone()),
                layer,
            );
        }
    }

    manager
}

fn register_layer(
    manager: &mut HotkeyManager,
    prefix: Vec<HotkeyStep>,
    context: Option<String>,
    layer: &Layer,
) {
    // Layer binds
    for (binding, action) in &layer.binds {
        let mut sequence = prefix.clone();
        sequence.push(HotkeyStep::new(binding.key.clone(), binding.modifiers));
        let action = action.clone();
        manager.bind(sequence, context.clone(), move || handle_action(&action));
    }
    // Nested layers
    for (trigger, child_layer) in &layer.children {
        let mut next_prefix = prefix.clone();
        next_prefix.push(HotkeyStep::new(trigger.key.clone(), trigger.modifiers));
        register_layer(manager, next_prefix, context.clone(), child_layer);
    }
}

#[cfg(test)]
mod tests {
    use super::trim_window_state;
    use core_graphics_types::geometry::{CGPoint, CGRect, CGSize};
    use std::collections::{HashMap, HashSet};

    fn frame() -> CGRect {
        CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(100.0, 100.0))
    }

    #[test]
    fn trim_window_state_drops_stale_entries() {
        let mut state = HashMap::from([(1, frame()), (2, frame()), (3, frame())]);
        let live_ids = HashSet::from([1, 3]);

        trim_window_state(&mut state, &live_ids, 10);

        assert_eq!(state.len(), 2);
        assert!(state.contains_key(&1));
        assert!(!state.contains_key(&2));
        assert!(state.contains_key(&3));
    }

    #[test]
    fn trim_window_state_enforces_capacity() {
        let mut state = HashMap::new();
        for id in 1..=8 {
            state.insert(id, frame());
        }
        let live_ids: HashSet<u32> = (1..=8).collect();

        trim_window_state(&mut state, &live_ids, 3);

        assert!(state.len() <= 3);
    }
}
