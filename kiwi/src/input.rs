use crate::ffi::CGEventKeyboardGetUnicodeString;
use core::ffi::c_void;
use core_graphics::display::CGPoint;
use core_graphics::event::CGEventTapLocation;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, CGKeyCode, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use foreign_types::ForeignType;
use kiwi_parser::{Key, KeyBinding, Modifiers};
use std::cell::RefCell;
use std::ffi::{c_double, c_int};
use std::time::Duration;
use tracing::info;

pub const USER_DATA: i64 = 0x6B697769; // "kiwi" in hexadecimal

thread_local! {
    static EVENT_SOURCE: RefCell<Option<CGEventSource>> = const { RefCell::new(None) };
}

#[repr(C)]
struct ObjcCGPoint {
    x: c_double,
    y: c_double,
}

#[link(name = "CoreFoundation", kind = "framework")]
#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
#[link(name = "objc")]
unsafe extern "C" {
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventCreateKeyboardEvent(source: *mut c_void, keycode: u16, keydown: bool) -> *mut c_void;
    fn CGEventKeyboardSetUnicodeString(
        event: *mut c_void,
        string_length: u64,
        unicode_string: *const u16,
    );
    fn CGEventSetIntegerValueField(event: *mut c_void, field: u32, value: i64);
    fn CFRelease(obj: *mut c_void);
    fn objc_getClass(name: *const u8) -> *mut c_void;
    fn sel_registerName(name: *const u8) -> *mut c_void;
    fn objc_msgSend();
}

const CG_EVENT_SOURCE_USER_DATA_FIELD: u32 = 42;

pub fn modifiers_from_cg_flags(flags: core_graphics::event::CGEventFlags) -> Modifiers {
    let mut result = Modifiers::NONE;
    if flags.contains(core_graphics::event::CGEventFlags::CGEventFlagShift) {
        result |= Modifiers::SHIFT;
    }
    if flags.contains(core_graphics::event::CGEventFlags::CGEventFlagControl) {
        result |= Modifiers::CONTROL;
    }
    if flags.contains(core_graphics::event::CGEventFlags::CGEventFlagAlternate) {
        result |= Modifiers::OPTION;
    }
    if flags.contains(core_graphics::event::CGEventFlags::CGEventFlagCommand) {
        result |= Modifiers::COMMAND;
    }
    result
}

fn get_event_source() -> CGEventSource {
    EVENT_SOURCE.with(|slot| {
        let mut guard = slot.borrow_mut();
        guard
            .get_or_insert_with(|| {
                CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
                    .expect("Failed to create CGEventSource")
            })
            .clone()
    })
}

fn modifiers_to_cg_flags(modifiers: Modifiers) -> CGEventFlags {
    let mut flags = CGEventFlags::empty();
    if modifiers.contains(Modifiers::SHIFT) {
        flags |= CGEventFlags::CGEventFlagShift;
    }
    if modifiers.contains(Modifiers::CONTROL) {
        flags |= CGEventFlags::CGEventFlagControl;
    }
    if modifiers.contains(Modifiers::OPTION) {
        flags |= CGEventFlags::CGEventFlagAlternate;
    }
    if modifiers.contains(Modifiers::COMMAND) {
        flags |= CGEventFlags::CGEventFlagCommand;
    }
    flags
}

fn modifiers_to_ns_flags(modifiers: Modifiers) -> u64 {
    let mut flags = 0_u64;
    if modifiers.contains(Modifiers::SHIFT) {
        flags |= 1 << 17;
    }
    if modifiers.contains(Modifiers::CONTROL) {
        flags |= 1 << 18;
    }
    if modifiers.contains(Modifiers::OPTION) {
        flags |= 1 << 19;
    }
    if modifiers.contains(Modifiers::COMMAND) {
        flags |= 1 << 20;
    }
    flags
}

fn media_keycode(key: &Key) -> Option<c_int> {
    match key {
        Key::VolumeUp => Some(0),
        Key::VolumeDown => Some(1),
        Key::BrightnessUp => Some(2),
        Key::BrightnessDown => Some(3),
        Key::KeyboardBrightnessUp => Some(21),
        Key::KeyboardBrightnessDown => Some(22),
        Key::Mute => Some(7),
        Key::PlayPause => Some(16),
        Key::NextTrack => Some(17),
        Key::PrevTrack => Some(18),
        _ => None,
    }
}

fn press_media_key(media_keycode: c_int, modifiers: Modifiers) {
    unsafe {
        let ns_event = objc_getClass(b"NSEvent\0".as_ptr());
        let sel_other = sel_registerName(
            b"otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:\0"
                .as_ptr(),
        );
        let sel_cg_event = sel_registerName(b"CGEvent\0".as_ptr());

        type CreateEventFn = unsafe extern "C" fn(
            *mut c_void,
            *mut c_void,
            usize,
            ObjcCGPoint,
            u64,
            c_double,
            i64,
            *mut c_void,
            i16,
            isize,
            isize,
        ) -> *mut c_void;
        let create_event: CreateEventFn = std::mem::transmute(objc_msgSend as *const ());

        type GetCgEventFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
        let get_cg_event: GetCgEventFn = std::mem::transmute(objc_msgSend as *const ());

        for is_down in [true, false] {
            let key_state_bits = if is_down { 0xA00 } else { 0xB00 };
            let data1 = (media_keycode << 16) | key_state_bits;
            let ns_flags = modifiers_to_ns_flags(modifiers);

            let event = create_event(
                ns_event,
                sel_other,
                14, // NSSystemDefined
                ObjcCGPoint { x: 0.0, y: 0.0 },
                ns_flags,
                0.0,
                0,
                std::ptr::null_mut(),
                8, // subtype
                data1 as isize,
                -1,
            );

            if !event.is_null() {
                let cg_event = get_cg_event(event, sel_cg_event);
                CGEventSetIntegerValueField(cg_event, CG_EVENT_SOURCE_USER_DATA_FIELD, USER_DATA);
                CGEventPost(0, cg_event);
            }
        }
    }
}

fn press_virtual_key(code: u16) {
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null_mut(), code, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null_mut(), code, false);

        if !down.is_null() {
            CGEventSetIntegerValueField(down, CG_EVENT_SOURCE_USER_DATA_FIELD, USER_DATA);
            CGEventPost(0, down);
            CFRelease(down);
        }
        if !up.is_null() {
            CGEventSetIntegerValueField(up, CG_EVENT_SOURCE_USER_DATA_FIELD, USER_DATA);
            CGEventPost(0, up);
            CFRelease(up);
        }
    }
}

fn is_virtual_system_key(key: &Key) -> bool {
    matches!(
        key,
        Key::MissionControl | Key::Spotlight | Key::Dictation | Key::DoNotDisturb
    )
}

pub fn send_key_combination(combo: &KeyBinding) {
    if let Some(code) = media_keycode(&combo.key) {
        press_media_key(code, combo.modifiers);
        return;
    }

    if is_virtual_system_key(&combo.key) {
        press_virtual_key(key_to_cg_keycode(&combo.key) as u16);
        return;
    }

    let keycode = key_to_cg_keycode(&combo.key);
    let flags = modifiers_to_cg_flags(combo.modifiers);
    let source = get_event_source();

    // Key Down
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), keycode, true) {
        event.set_flags(flags);
        event.set_integer_value_field(
            core_graphics::event::EventField::EVENT_SOURCE_USER_DATA,
            USER_DATA,
        );
        event.post(CGEventTapLocation::HID);
    }

    // Sufficient duration for the app to register the key down
    std::thread::sleep(Duration::from_millis(10));

    // Key Up
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), keycode, false) {
        event.set_flags(flags);
        event.set_integer_value_field(
            core_graphics::event::EventField::EVENT_SOURCE_USER_DATA,
            USER_DATA,
        );
        event.post(CGEventTapLocation::HID);
    }
}

pub fn type_unicode_string(text: &str) {
    info!("Typing unicode string: {text}");
    let utf16: Vec<u16> = text.encode_utf16().collect();
    unsafe {
        let source = std::ptr::null_mut();
        let down = CGEventCreateKeyboardEvent(source, 0, true);
        let up = CGEventCreateKeyboardEvent(source, 0, false);

        for event in [down, up] {
            if event.is_null() {
                continue;
            }

            CGEventSetIntegerValueField(event, CG_EVENT_SOURCE_USER_DATA_FIELD, USER_DATA);
            CGEventKeyboardSetUnicodeString(event, utf16.len() as u64, utf16.as_ptr());
            CGEventPost(0, event);
            CFRelease(event);
        }
    }
}

pub fn click(point: CGPoint) {
    let source = get_event_source();

    // Mouse Down
    if let Ok(event) = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseDown,
        point,
        CGMouseButton::Left,
    ) {
        event.set_integer_value_field(
            core_graphics::event::EventField::EVENT_SOURCE_USER_DATA,
            USER_DATA,
        );
        event.post(CGEventTapLocation::HID);
    }

    std::thread::sleep(Duration::from_millis(30));

    // Mouse Up
    if let Ok(event) = CGEvent::new_mouse_event(
        source.clone(),
        CGEventType::LeftMouseUp,
        point,
        CGMouseButton::Left,
    ) {
        event.set_integer_value_field(
            core_graphics::event::EventField::EVENT_SOURCE_USER_DATA,
            USER_DATA,
        );
        event.post(CGEventTapLocation::HID);
    }
}

pub fn key_to_cg_keycode(key: &Key) -> CGKeyCode {
    match key {
        Key::Char(c) => match c.to_ascii_lowercase() {
            'a' => 0,
            's' => 1,
            'd' => 2,
            'f' => 3,
            'h' => 4,
            'g' => 5,
            'z' => 6,
            'x' => 7,
            'c' => 8,
            'v' => 9,
            '§' => 10,
            'b' => 11,
            'q' => 12,
            'w' => 13,
            'e' => 14,
            'r' => 15,
            'y' => 16,
            't' => 17,
            '1' => 18,
            '2' => 19,
            '3' => 20,
            '4' => 21,
            '6' => 22,
            '5' => 23,
            '=' => 24,
            '9' => 25,
            '7' => 26,
            '-' => 27,
            '8' => 28,
            '0' => 29,
            ']' => 30,
            'o' => 31,
            'u' => 32,
            '[' => 33,
            'i' => 34,
            'p' => 35,
            'l' => 37,
            'j' => 38,
            '\'' => 39,
            'k' => 40,
            ';' => 41,
            '\\' => 42,
            ',' => 43,
            '/' => 44,
            'n' => 45,
            'm' => 46,
            '.' => 47,
            '`' => 50,
            _ => 0,
        },
        Key::Enter => 36,
        Key::Tab => 48,
        Key::Space => 49,
        Key::Backspace => 51,
        Key::Esc => 53,
        Key::F(n) => match n {
            1 => 122,
            2 => 120,
            3 => 99,
            4 => 118,
            5 => 96,
            6 => 97,
            7 => 98,
            8 => 100,
            9 => 101,
            10 => 109,
            11 => 103,
            12 => 111,
            _ => 122,
        },
        Key::ArrowLeft => 123,
        Key::ArrowRight => 124,
        Key::ArrowDown => 125,
        Key::ArrowUp => 126,
        Key::Home => 115,
        Key::End => 119,
        Key::PageUp => 116,
        Key::PageDown => 121,
        Key::Delete => 117,
        Key::MissionControl => 160,
        Key::Spotlight => 177,
        Key::Dictation => 176,
        Key::DoNotDisturb => 178,
        _ => 0,
    }
}

pub fn from_cg_code(code: u16, char: Option<char>) -> Option<Key> {
    match code {
        0x35 => Some(Key::Esc),
        0x24 => Some(Key::Enter),
        0x31 => Some(Key::Space),
        0x33 => Some(Key::Backspace),
        0x30 => Some(Key::Tab),
        0x7E => Some(Key::ArrowUp),
        0x7D => Some(Key::ArrowDown),
        0x7B => Some(Key::ArrowLeft),
        0x7C => Some(Key::ArrowRight),
        0x7A => Some(Key::F(1)),
        0x78 => Some(Key::F(2)),
        0x63 => Some(Key::F(3)),
        0x76 => Some(Key::F(4)),
        0x60 => Some(Key::F(5)),
        0x61 => Some(Key::F(6)),
        0x62 => Some(Key::F(7)),
        0x64 => Some(Key::F(8)),
        0x65 => Some(Key::F(9)),
        0x6D => Some(Key::F(10)),
        0x67 => Some(Key::F(11)),
        0x6F => Some(Key::F(12)),
        0x69 => Some(Key::F(13)),
        0x6B => Some(Key::F(14)),
        0x71 => Some(Key::F(15)),
        0x6A => Some(Key::F(16)),
        0x40 => Some(Key::F(17)),
        0x4F => Some(Key::F(18)),
        0x50 => Some(Key::F(19)),
        0x5A => Some(Key::F(20)),
        0x73 => Some(Key::Home),
        0x77 => Some(Key::End),
        0x74 => Some(Key::PageUp),
        0x79 => Some(Key::PageDown),
        0x75 => Some(Key::Delete),

        0x48 => Some(Key::VolumeUp),
        0x49 => Some(Key::VolumeDown),
        0x4A => Some(Key::Mute),

        _ => {
            if let Some(c) = crate::translate::keycode_to_base_char(code as CGKeyCode) {
                Some(Key::Char(c))
            } else {
                match char {
                    Some(mut c) => {
                        if (1..=26).contains(&(c as u32))
                            && let Some(base_char) = char::from_u32((c as u32 - 1) + 'a' as u32)
                        {
                            c = base_char;
                        }
                        Some(Key::Char(c.to_ascii_lowercase()))
                    }
                    None => None,
                }
            }
        }
    }
}

pub fn get_character_from_event(event: &CGEvent) -> Option<char> {
    let mut actual_length = 0;
    let mut buf = [0u16; 4];

    unsafe {
        CGEventKeyboardGetUnicodeString(
            event.as_ptr() as _,
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

pub fn from_system_defined_event(event: &CGEvent) -> Option<(Key, bool)> {
    unsafe {
        let ns_event_class = objc_getClass(b"NSEvent\0".as_ptr());
        let sel_event_with_cg = sel_registerName(b"eventWithCGEvent:\0".as_ptr());
        let sel_subtype = sel_registerName(b"subtype\0".as_ptr());
        let sel_data1 = sel_registerName(b"data1\0".as_ptr());

        type EventWithCgFn =
            unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void;
        let event_with_cg: EventWithCgFn = std::mem::transmute(objc_msgSend as *const ());

        type SubtypeFn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> i16;
        let subtype_fn: SubtypeFn = std::mem::transmute(objc_msgSend as *const ());

        type Data1Fn = unsafe extern "C" fn(*mut c_void, *mut c_void) -> isize;
        let data1_fn: Data1Fn = std::mem::transmute(objc_msgSend as *const ());

        let ns_event = event_with_cg(
            ns_event_class,
            sel_event_with_cg,
            event.as_ptr() as *mut c_void,
        );
        if ns_event.is_null() {
            return None;
        }

        // Subtype 8 is for aux control buttons / media keys.
        let subtype = subtype_fn(ns_event, sel_subtype);
        if subtype != 8 {
            return None;
        }

        let data1 = data1_fn(ns_event, sel_data1) as i64;
        let key_type = ((data1 >> 16) & 0xFFFF) as i32;
        let key_state = (data1 & 0xFF00) as i32;
        let is_down = match key_state {
            0xA00 => true,
            0xB00 => false,
            _ => return None,
        };

        let key = match key_type {
            0 => Key::VolumeUp,
            1 => Key::VolumeDown,
            2 => Key::BrightnessUp,
            3 => Key::BrightnessDown,
            7 => Key::Mute,
            16 => Key::PlayPause,
            17 => Key::NextTrack,
            18 => Key::PrevTrack,
            21 => Key::KeyboardBrightnessUp,
            22 => Key::KeyboardBrightnessDown,
            _ => return None,
        };

        Some((key, is_down))
    }
}
