use crate::manager::active_layer_names_snapshot;
use kiwi_parser::MenubarConfig;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, ClassType, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSMenu, NSStatusBar, NSStatusItem,
    NSStatusItemBehavior,
};
use objc2_foundation::{MainThreadMarker, NSObjectNSThreadPerformAdditions, NSTimer, NSTimeInterval, NSString};
use std::cell::RefCell;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Instant;

thread_local! {
    static MENUBAR_STATE: RefCell<Option<MenubarState>> = RefCell::new(None);
    static MENUBAR_OBSERVER: RefCell<Option<Retained<MenubarObserver>>> = RefCell::new(None);
}

static OBSERVER_PTR: AtomicPtr<MenubarObserver> = AtomicPtr::new(std::ptr::null_mut());

const CMD_ENABLE: &str = "enable";
const CMD_DISABLE: &str = "disable";
const CMD_TOGGLE: &str = "toggle";
const CMD_REFRESH: &str = "refresh";

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[derive(Debug, PartialEq, Eq, Hash)]
    pub struct MenubarObserver;

    impl MenubarObserver {
        #[unsafe(method(handleCommand:))]
        fn handle_command(&self, command: &NSString) {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            let cmd = command.to_string();
            match cmd.as_str() {
                CMD_ENABLE => apply_enable(mtm),
                CMD_DISABLE => apply_disable(),
                CMD_TOGGLE => apply_toggle(mtm),
                CMD_REFRESH => apply_refresh(mtm),
                _ => {}
            }
        }

        #[unsafe(method(handleTimeout:))]
        fn handle_timeout(&self, _timer: &NSTimer) {
            let mtm = unsafe { MainThreadMarker::new_unchecked() };
            apply_timeout(mtm);
        }
    }
);

impl MenubarObserver {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let alloc = Self::alloc(mtm);
        unsafe { msg_send![alloc, init] }
    }

    fn alloc(_mtm: MainThreadMarker) -> Allocated<Self> {
        unsafe { msg_send![Self::class(), alloc] }
    }
}

struct MenubarState {
    enabled: bool,
    max_len: usize,
    status_bar: Retained<NSStatusBar>,
    status_item: Option<Retained<NSStatusItem>>,
    menu: Option<Retained<NSMenu>>,
    timeout_timer: Option<Retained<NSTimer>>,
}

pub fn init(config: MenubarConfig) {
    let mtm = MainThreadMarker::new().expect("Menubar must initialize on main thread");
    let app = NSApplication::sharedApplication(mtm);
    let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    MENUBAR_OBSERVER.with(|cell| {
        if cell.borrow().is_some() {
            return;
        }
        let observer = MenubarObserver::new(mtm);
        let ptr = Retained::as_ptr(&observer) as *mut MenubarObserver;
        OBSERVER_PTR.store(ptr, Ordering::SeqCst);
        *cell.borrow_mut() = Some(observer);
    });

    MENUBAR_STATE.with(|cell| {
        if cell.borrow().is_some() {
            return;
        }

        let mut state = MenubarState {
            enabled: config.enabled,
            max_len: config.max_len,
            status_bar: NSStatusBar::systemStatusBar(),
            status_item: None,
            menu: None,
            timeout_timer: None,
        };

        if state.enabled {
            ensure_enabled(&mut state, mtm);
        }

        *cell.borrow_mut() = Some(state);
    });
}

pub fn request_enable() {
    send_command(CMD_ENABLE);
}

pub fn request_disable() {
    send_command(CMD_DISABLE);
}

pub fn request_toggle() {
    send_command(CMD_TOGGLE);
}

pub fn request_refresh() {
    send_command(CMD_REFRESH);
}

fn send_command(cmd: &str) {
    let ptr = OBSERVER_PTR.load(Ordering::SeqCst);
    if ptr.is_null() {
        return;
    }

    let msg = NSString::from_str(cmd);
    unsafe {
        (&*ptr).performSelectorOnMainThread_withObject_waitUntilDone(
            objc2::sel!(handleCommand:),
            Some(msg.as_ref()),
            false,
        );
    }
}

fn apply_enable(mtm: MainThreadMarker) {
    MENUBAR_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };
        state.enabled = true;
        ensure_enabled(state, mtm);
    });
}

fn apply_disable() {
    MENUBAR_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };
        state.enabled = false;
        ensure_disabled(state);
    });
}

fn apply_toggle(mtm: MainThreadMarker) {
    MENUBAR_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };
        state.enabled = !state.enabled;
        if state.enabled {
            ensure_enabled(state, mtm);
        } else {
            ensure_disabled(state);
        }
    });
}

fn apply_refresh(mtm: MainThreadMarker) {
    MENUBAR_STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(state) = borrow.as_mut() else {
            return;
        };
        if state.enabled {
            update_title(state, mtm);
            schedule_timeout(state);
        }
    });
}

fn apply_timeout(mtm: MainThreadMarker) {
    let _ = crate::manager::expire_active_layers();
    apply_refresh(mtm);
}

fn ensure_enabled(state: &mut MenubarState, mtm: MainThreadMarker) {
    if state.status_item.is_none() {
        let item = state.status_bar.statusItemWithLength(-1.0);
        let autosave = NSString::from_str("kiwi-menubar");
        item.setAutosaveName(Some(&autosave));
        item.setBehavior(NSStatusItemBehavior::RemovalAllowed);
        item.setVisible(true);
        let menu = NSMenu::new(mtm);
        let title = NSString::from_str("Kiwi");
        let empty = NSString::from_str("");
        unsafe {
            menu.addItemWithTitle_action_keyEquivalent(&title, None, &empty);
        }
        item.setMenu(Some(&menu));
        state.status_item = Some(item);
        state.menu = Some(menu);
    } else if let Some(item) = state.status_item.as_ref() {
        item.setVisible(true);
    }
    update_title(state, mtm);
    schedule_timeout(state);
}

fn ensure_disabled(state: &mut MenubarState) {
    if let Some(timer) = state.timeout_timer.take() {
        timer.invalidate();
    }
    if let Some(item) = state.status_item.as_ref() {
        item.setVisible(false);
    }
}

fn update_title(state: &mut MenubarState, mtm: MainThreadMarker) {
    let Some(item) = state.status_item.as_ref() else {
        return;
    };
    let Some(button) = item.button(mtm) else {
        return;
    };

    let layers = active_layer_names_snapshot();
    let title = format_layer_title(&layers, state.max_len);
    let title = NSString::from_str(&title);
    button.setTitle(&title);
}

fn schedule_timeout(state: &mut MenubarState) {
    if let Some(timer) = state.timeout_timer.take() {
        timer.invalidate();
    }

    let Some(deadline) = crate::manager::next_layer_deadline() else {
        return;
    };

    let now = Instant::now();
    let mut secs = if deadline > now {
        (deadline - now).as_secs_f64()
    } else {
        0.001
    };
    if secs < 0.001 {
        secs = 0.001;
    }

    MENUBAR_OBSERVER.with(|cell| {
        let binding = cell.borrow();
        let Some(observer) = binding.as_ref() else {
            return;
        };
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                secs as NSTimeInterval,
                observer.as_ref(),
                objc2::sel!(handleTimeout:),
                None,
                false,
            )
        };
        state.timeout_timer = Some(timer);
    });
}

fn format_layer_title(layers: &[String], max_len: usize) -> String {
    let Some(top) = layers.last() else {
        return "root".to_string();
    };
    truncate_layer(top, max_len)
}

fn truncate_layer(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    let mut parts: Vec<String> = path.split('.').map(|s| s.to_string()).collect();

    // Step 1: progressively shorten leading segments
    for i in 0..parts.len().saturating_sub(1) {
        if parts[i].len() > 1 {
            parts[i] = parts[i].chars().next().unwrap().to_string();
        }

        let candidate = parts.join(".");
        if candidate.len() <= max_len {
            return candidate;
        }
    }

    // Step 2: collapse middle segments if possible
    if parts.len() > 2 {
        let first = parts.first().unwrap().chars().next().unwrap();
        let last = parts.last().unwrap();

        let candidate = format!("{}.…{}", first, last);
        if candidate.len() <= max_len {
            return candidate;
        }
    }

    // Step 3: fallback to last segment truncated
    let last = parts.last().unwrap();
    if last.len() <= max_len {
        return last.clone();
    }

    last.chars().take(max_len.saturating_sub(1)).collect::<String>() + "…"
}
