use crate::hotkey::{HotkeyManager, HotkeyStep, LayerBehavior};
use crate::window;
use kiwi_parser::{Action, Config, Key, KeyBinding, Layer, Modifiers, Resize, Snap};
use std::collections::{HashMap, HashSet};
use std::process::Command as ShellCommand;
use std::sync::mpsc::{self, Sender};
use tracing::{debug, error, info};

use core_graphics_types::geometry::CGRect;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

static WINDOW_STATE: OnceLock<Mutex<HashMap<u32, CGRect>>> = OnceLock::new();
const WINDOW_STATE_MAX_ENTRIES: usize = 256;
static ACTION_SENDER: OnceLock<Sender<Action>> = OnceLock::new();
static ACTION_QUEUE_DEPTH: AtomicUsize = AtomicUsize::new(0);
static INTERCEPT_MODE: OnceLock<Mutex<Option<InterceptMode>>> = OnceLock::new();

fn get_window_state() -> &'static Mutex<HashMap<u32, CGRect>> {
    WINDOW_STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
pub enum InterceptState {
    None,
    Pass,
    Swallow,
}

#[derive(Clone, Copy)]
enum InterceptKind {
    Pass,
    Swallow,
}

#[derive(Clone)]
struct InterceptMode {
    kind: InterceptKind,
    exit: KeyBinding,
    awaiting_exit_key_up: bool,
}

pub enum InterceptDecision {
    ProcessNormally,
    KeepWithoutProcessing,
    DropWithoutProcessing,
}

fn get_intercept_mode() -> &'static Mutex<Option<InterceptMode>> {
    INTERCEPT_MODE.get_or_init(|| Mutex::new(None))
}

fn activate_intercept_mode(kind: InterceptKind, exit: KeyBinding) {
    if let Ok(mut mode) = get_intercept_mode().lock() {
        if mode.is_none() {
            *mode = Some(InterceptMode {
                kind,
                exit,
                awaiting_exit_key_up: false,
            });
        } else {
            debug!("Ignoring intercept activation while another intercept mode is active");
        }
    }
}

fn is_exit_key_down(mode: &InterceptMode, key: &Key, modifiers: Modifiers) -> bool {
    mode.exit.key == *key && mode.exit.modifiers == modifiers
}

pub fn intercept_decision(key: &Key, modifiers: Modifiers, is_down: bool) -> InterceptDecision {
    let Ok(mut guard) = get_intercept_mode().lock() else {
        return InterceptDecision::ProcessNormally;
    };

    let Some(mode) = guard.as_mut() else {
        return InterceptDecision::ProcessNormally;
    };

    if is_down && is_exit_key_down(mode, key, modifiers) {
        mode.awaiting_exit_key_up = true;
        return InterceptDecision::DropWithoutProcessing;
    }

    if !is_down && mode.awaiting_exit_key_up && mode.exit.key == *key {
        *guard = None;
        return InterceptDecision::DropWithoutProcessing;
    }

    match mode.kind {
        InterceptKind::Pass => InterceptDecision::KeepWithoutProcessing,
        InterceptKind::Swallow => InterceptDecision::DropWithoutProcessing,
    }
}

pub fn init_action_executor() {
    if ACTION_SENDER.get().is_some() {
        return;
    }

    let (tx, rx) = mpsc::channel::<Action>();
    std::thread::Builder::new()
        .name("kiwi-action-executor".into())
        .spawn(move || {
            while let Ok(action) = rx.recv() {
                ACTION_QUEUE_DEPTH.fetch_sub(1, Ordering::SeqCst);
                handle_action(&action);
            }
        })
        .expect("failed to start action executor");

    let _ = ACTION_SENDER.set(tx);
}

pub fn dispatch_action(action: Action) {
    if let Some(tx) = ACTION_SENDER.get() {
        ACTION_QUEUE_DEPTH.fetch_add(1, Ordering::SeqCst);
        if let Err(e) = tx.send(action) {
            ACTION_QUEUE_DEPTH.fetch_sub(1, Ordering::SeqCst);
            error!("Failed to enqueue action: {}", e);
        }
    } else {
        // Fallback for safety if called before executor initialization.
        handle_action(&action);
    }
}

pub fn clear_window_state() {
    if let Ok(mut state) = get_window_state().lock() {
        state.clear();
    }
}

pub fn action_queue_depth() -> usize {
    ACTION_QUEUE_DEPTH.load(Ordering::SeqCst)
}

pub fn intercept_state() -> InterceptState {
    let Ok(guard) = get_intercept_mode().lock() else {
        return InterceptState::None;
    };

    match guard.as_ref().map(|m| m.kind) {
        Some(InterceptKind::Pass) => InterceptState::Pass,
        Some(InterceptKind::Swallow) => InterceptState::Swallow,
        None => InterceptState::None,
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
                ShellCommand::new("sh").arg("-c").arg(cmd).spawn().ok();
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
        Action::Pass(exit_binding) => {
            info!("Entering pass mode until {:?}", exit_binding);
            activate_intercept_mode(InterceptKind::Pass, exit_binding.clone());
        }
        Action::Swallow(exit_binding) => {
            info!("Entering swallow mode until {:?}", exit_binding);
            activate_intercept_mode(InterceptKind::Swallow, exit_binding.clone());
        }
        _ => {
            error!("Action not yet fully implemented: {:?}", action);
        }
    }
}

pub struct InjectOutcome {
    pub handled: bool,
    pub dispatched_actions: usize,
}

pub fn process_injected_binding(
    manager: &mut HotkeyManager,
    binding: &KeyBinding,
    app_name: &str,
) -> InjectOutcome {
    let mut handled = false;
    let mut dispatched_actions = 0;

    match intercept_decision(&binding.key, binding.modifiers, true) {
        InterceptDecision::ProcessNormally => {
            let down = manager.process(binding.key.clone(), binding.modifiers, true, app_name);
            handled |= down.handled;
            if let Some(action) = down.action {
                dispatch_action(action);
                dispatched_actions += 1;
            }
        }
        InterceptDecision::KeepWithoutProcessing | InterceptDecision::DropWithoutProcessing => {}
    }

    match intercept_decision(&binding.key, binding.modifiers, false) {
        InterceptDecision::ProcessNormally => {
            let up = manager.process(binding.key.clone(), binding.modifiers, false, app_name);
            handled |= up.handled;
            if let Some(action) = up.action {
                dispatch_action(action);
                dispatched_actions += 1;
            }
        }
        InterceptDecision::KeepWithoutProcessing | InterceptDecision::DropWithoutProcessing => {}
    }

    InjectOutcome {
        handled,
        dispatched_actions,
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
        if let Ok(state) = get_window_state().lock()
            && let Some(prev_frame) = state.get(&focused.id)
        {
            window::set_focused_window_bounds(
                prev_frame.origin.x,
                prev_frame.origin.y,
                prev_frame.size.width,
                prev_frame.size.height,
            );
            return;
        }
        return;
    }

    if side == Snap::Fullscreen && window::ops::toggle_native_fullscreen() {
        return;
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyUsageKind {
    Binding,
    LayerActivation,
}

fn register_key_usage(
    seen: &mut HashMap<(Option<String>, HotkeyStep), KeyUsageKind>,
    scope: Option<&str>,
    step: &HotkeyStep,
    usage: KeyUsageKind,
) -> Result<(), String> {
    let key = (scope.map(str::to_string), step.clone());
    if let Some(existing) = seen.get(&key)
        && *existing != usage
    {
        let scope_label = scope.unwrap_or("global");
        let (left, right) = match (existing, usage) {
            (KeyUsageKind::Binding, KeyUsageKind::LayerActivation) => {
                ("binding", "layer activation")
            }
            (KeyUsageKind::LayerActivation, KeyUsageKind::Binding) => {
                ("layer activation", "binding")
            }
            _ => ("binding", "binding"),
        };
        return Err(format!(
            "key conflict in scope '{scope_label}' for '{step}': {left} conflicts with {right}"
        ));
    }

    seen.insert(key, usage);
    Ok(())
}

pub fn setup_manager(config: &Config) -> Result<HotkeyManager, String> {
    let mut manager = HotkeyManager::new();
    let mut seen: HashMap<(Option<String>, HotkeyStep), KeyUsageKind> = HashMap::new();

    // 1. Global binds
    for (binding, action) in &config.global_binds {
        let step = HotkeyStep::new(binding.key.clone(), binding.modifiers);
        register_key_usage(&mut seen, None, &step, KeyUsageKind::Binding)?;
        manager.bind(
            vec![step],
            None,
            action.clone(),
        );
    }

    // 2. Layers
    for (trigger, layer) in &config.layers {
        register_layer(
            &mut manager,
            &mut seen,
            Vec::new(),
            None,
            String::new(),
            trigger,
            layer,
        )?;
    }

    // 3. Apps
    for (app_name, app_config) in &config.apps {
        // App binds
        for (binding, action) in &app_config.binds {
            let step = HotkeyStep::new(binding.key.clone(), binding.modifiers);
            register_key_usage(
                &mut seen,
                Some(app_name),
                &step,
                KeyUsageKind::Binding,
            )?;
            manager.bind(
                vec![step],
                Some(app_name.clone()),
                action.clone(),
            );
        }
        // App layers
        for (trigger, layer) in &app_config.children {
            register_layer(
                &mut manager,
                &mut seen,
                Vec::new(),
                Some(app_name.clone()),
                format!("app:{app_name}"),
                trigger,
                layer,
            )?;
        }
    }

    Ok(manager)
}

fn register_layer(
    manager: &mut HotkeyManager,
    seen: &mut HashMap<(Option<String>, HotkeyStep), KeyUsageKind>,
    prefix: Vec<HotkeyStep>,
    context: Option<String>,
    name_prefix: String,
    trigger: &KeyBinding,
    layer: &Layer,
) -> Result<(), String> {
    let mut layer_prefix = prefix;
    let activation_step = HotkeyStep::new(trigger.key.clone(), trigger.modifiers);
    register_key_usage(
        seen,
        context.as_deref(),
        &activation_step,
        KeyUsageKind::LayerActivation,
    )?;
    layer_prefix.push(activation_step);

    let layer_name = if name_prefix.is_empty() {
        layer.name.clone()
    } else {
        format!("{}.{}", name_prefix, layer.name)
    };

    let behavior = LayerBehavior {
        name: Some(layer_name.clone()),
        mode: layer.mode,
        timeout_ms: layer.timeout,
        deactivate: layer
            .deactivate
            .as_ref()
            .map(|k| HotkeyStep::new(k.key.clone(), k.modifiers)),
    };
    manager.register_layer(layer_prefix.clone(), context.clone(), behavior);

    // Layer binds
    for (binding, action) in &layer.binds {
        let mut sequence = layer_prefix.clone();
        sequence.push(HotkeyStep::new(binding.key.clone(), binding.modifiers));
        manager.bind(sequence, context.clone(), action.clone());
    }

    // Nested layers
    for (trigger, child_layer) in &layer.children {
        register_layer(
            manager,
            seen,
            layer_prefix.clone(),
            context.clone(),
            layer_name.clone(),
            trigger,
            child_layer,
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::setup_manager;
    use super::trim_window_state;
    use super::{
        activate_intercept_mode, get_intercept_mode, intercept_decision, InterceptDecision,
        InterceptKind,
    };
    use kiwi_parser::{Key, KeyBinding, Modifiers};
    use std::path::PathBuf;
    use core_graphics_types::geometry::{CGPoint, CGRect, CGSize};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn frame() -> CGRect {
        CGRect::new(&CGPoint::new(0.0, 0.0), &CGSize::new(100.0, 100.0))
    }

    fn reset_intercept_mode() {
        if let Ok(mut guard) = get_intercept_mode().lock() {
            *guard = None;
        }
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

    #[test]
    fn swallow_mode_consumes_until_exit() {
        let _serial = TEST_LOCK.lock().unwrap();
        reset_intercept_mode();

        activate_intercept_mode(
            InterceptKind::Swallow,
            KeyBinding {
                modifiers: Modifiers::COMMAND,
                key: Key::Char('x'),
            },
        );

        let decision = intercept_decision(&Key::Char('a'), Modifiers::NONE, true);
        assert!(matches!(decision, InterceptDecision::DropWithoutProcessing));

        let down_exit = intercept_decision(&Key::Char('x'), Modifiers::COMMAND, true);
        assert!(matches!(down_exit, InterceptDecision::DropWithoutProcessing));

        let up_exit = intercept_decision(&Key::Char('x'), Modifiers::NONE, false);
        assert!(matches!(up_exit, InterceptDecision::DropWithoutProcessing));

        let after = intercept_decision(&Key::Char('a'), Modifiers::NONE, true);
        assert!(matches!(after, InterceptDecision::ProcessNormally));
    }

    #[test]
    fn pass_mode_keeps_non_exit_but_skips_processing() {
        let _serial = TEST_LOCK.lock().unwrap();
        reset_intercept_mode();

        activate_intercept_mode(
            InterceptKind::Pass,
            KeyBinding {
                modifiers: Modifiers::CONTROL,
                key: Key::Char('q'),
            },
        );

        let decision = intercept_decision(&Key::Char('a'), Modifiers::NONE, true);
        assert!(matches!(decision, InterceptDecision::KeepWithoutProcessing));

        let down_exit = intercept_decision(&Key::Char('q'), Modifiers::CONTROL, true);
        assert!(matches!(down_exit, InterceptDecision::DropWithoutProcessing));

        let up_exit = intercept_decision(&Key::Char('q'), Modifiers::NONE, false);
        assert!(matches!(up_exit, InterceptDecision::DropWithoutProcessing));

        let after = intercept_decision(&Key::Char('a'), Modifiers::NONE, true);
        assert!(matches!(after, InterceptDecision::ProcessNormally));
    }

    #[test]
    fn setup_manager_rejects_global_binding_and_layer_activation_conflict() {
        let raw = r#"
[binds]
"cmd+e" = "reload"

[layer.nav]
activate = "cmd+e"
"j" = "reload"
"#;
        let config =
            kiwi_parser::parse_config(raw, PathBuf::from("test.toml")).expect("config parses");

        let err = match setup_manager(&config) {
            Ok(_) => panic!("expected global conflict"),
            Err(err) => err,
        };
        assert!(err.contains("scope 'global'"));
        assert!(err.contains("cmd+e"));
    }

    #[test]
    fn setup_manager_rejects_app_binding_and_layer_activation_conflict() {
        let raw = r#"
[app."Google Chrome"]
"cmd+e" = "reload"

[app."Google Chrome".nav]
activate = "cmd+e"
"j" = "reload"
"#;
        let config =
            kiwi_parser::parse_config(raw, PathBuf::from("test.toml")).expect("config parses");

        let err = match setup_manager(&config) {
            Ok(_) => panic!("expected app conflict"),
            Err(err) => err,
        };
        assert!(err.contains("scope 'Google Chrome'"));
        assert!(err.contains("cmd+e"));
    }

    #[test]
    fn setup_manager_allows_cross_scope_same_key() {
        let raw = r#"
[binds]
"cmd+e" = "reload"

[app."Google Chrome"]
"cmd+e" = "quit"
"#;
        let config =
            kiwi_parser::parse_config(raw, PathBuf::from("test.toml")).expect("config parses");

        let manager = setup_manager(&config);
        assert!(manager.is_ok());
    }
}
