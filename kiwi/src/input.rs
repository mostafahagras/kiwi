use std::time::Duration;
use std::sync::OnceLock;
use core_graphics::display::CGPoint;
use core_graphics::event::{CGEvent, CGEventType, CGKeyCode, CGMouseButton, CGEventFlags};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::event::CGEventTapLocation;
use crate::parser::{Key, KeyCombination};

pub const USER_DATA: i64 = 0x6B697769; // "kiwi" in hexadecimal

struct SyncEventSource(CGEventSource);
unsafe impl Send for SyncEventSource {}
unsafe impl Sync for SyncEventSource {}

static EVENT_SOURCE: OnceLock<SyncEventSource> = OnceLock::new();

fn get_event_source() -> CGEventSource {
    EVENT_SOURCE.get_or_init(|| {
        SyncEventSource(CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .expect("Failed to create CGEventSource"))
    }).0.clone()
}

pub fn send_key_combination(combo: &KeyCombination) {
    let keycode = key_to_cg_keycode(&combo.key);
    let mut flags = CGEventFlags::empty();
    if combo.modifiers.shift { flags |= CGEventFlags::CGEventFlagShift; }
    if combo.modifiers.control { flags |= CGEventFlags::CGEventFlagControl; }
    if combo.modifiers.alt { flags |= CGEventFlags::CGEventFlagAlternate; }
    if combo.modifiers.command { flags |= CGEventFlags::CGEventFlagCommand; }

    let source = get_event_source();
    
    // Key Down
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), keycode, true) {
        event.set_flags(flags);
        event.set_integer_value_field(core_graphics::event::EventField::EVENT_SOURCE_USER_DATA, USER_DATA);
        event.post(CGEventTapLocation::HID);
    }

    // Sufficient duration for the app to register the key down
    std::thread::sleep(Duration::from_millis(30));

    // Key Up
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), keycode, false) {
        event.set_flags(flags);
        event.set_integer_value_field(core_graphics::event::EventField::EVENT_SOURCE_USER_DATA, USER_DATA);
        event.post(CGEventTapLocation::HID);
    }
}

pub fn click(point: CGPoint) {
    let source = get_event_source();
    
    // Mouse Down
    if let Ok(event) = CGEvent::new_mouse_event(source.clone(), CGEventType::LeftMouseDown, point, CGMouseButton::Left) {
        event.set_integer_value_field(core_graphics::event::EventField::EVENT_SOURCE_USER_DATA, USER_DATA);
        event.post(CGEventTapLocation::HID);
    }

    std::thread::sleep(Duration::from_millis(30));

    // Mouse Up
    if let Ok(event) = CGEvent::new_mouse_event(source.clone(), CGEventType::LeftMouseUp, point, CGMouseButton::Left) {
        event.set_integer_value_field(core_graphics::event::EventField::EVENT_SOURCE_USER_DATA, USER_DATA);
        event.post(CGEventTapLocation::HID);
    }
}

fn key_to_cg_keycode(key: &Key) -> CGKeyCode {
    match key {
        Key::Char(c) => match c.to_ascii_lowercase() {
            'a' => 0, 's' => 1, 'd' => 2, 'f' => 3, 'h' => 4, 'g' => 5, 'z' => 6, 'x' => 7,
            'c' => 8, 'v' => 9, 'b' => 11, 'q' => 12, 'w' => 13, 'e' => 14, 'r' => 15, 'y' => 16,
            't' => 17, '1' => 18, '2' => 19, '3' => 20, '4' => 21, '6' => 22, '5' => 23, '=' => 24,
            '9' => 25, '7' => 26, '-' => 27, '8' => 28, '0' => 29, ']' => 30, 'o' => 31, 'u' => 32,
            '[' => 33, 'i' => 34, 'p' => 35, 'l' => 37, 'j' => 38, '\'' => 39, 'k' => 40, ';' => 41,
            '\\' => 42, ',' => 43, '/' => 44, 'n' => 45, 'm' => 46, '.' => 47, '`' => 50,
            _ => 0,
        },
        Key::Enter => 36,
        Key::Tab => 48,
        Key::Space => 49,
        Key::Backspace => 51,
        Key::Esc => 53,
        Key::Command => 55,
        Key::Shift => 56,
        Key::CapsLock => 57,
        Key::Alt => 58,
        Key::Control => 59,
        Key::Function => 63,
        Key::F(n) => match n {
            1 => 122, 2 => 120, 3 => 99, 4 => 118, 5 => 96, 6 => 97,
            7 => 98, 8 => 100, 9 => 101, 10 => 109, 11 => 103, 12 => 111,
            _ => 122,
        },
        Key::ArrowLeft => 123, Key::ArrowRight => 124, Key::ArrowDown => 125, Key::ArrowUp => 126,
        _ => 0,
    }
}
