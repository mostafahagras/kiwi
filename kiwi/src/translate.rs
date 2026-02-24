use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::sync::Mutex;

use crate::ffi::{
    LMGetKbdType, TISCopyCurrentKeyboardInputSource, TISCopyCurrentKeyboardLayoutInputSource,
    TISCreateInputSourceList, TISGetInputSourceProperty, UCKeyTranslate, UCKeyboardLayout,
    kTISPropertyInputSourceID,
};

use core_foundation::base::UInt32;
use core_foundation::data::{CFDataGetBytePtr, CFDataRef};

use core_graphics::event::CGKeyCode;
use std::os::raw::c_uint;

struct SendPtr(*const UCKeyboardLayout);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

static CONFIGURED_LAYOUT: Mutex<Option<SendPtr>> = Mutex::new(None);

const K_UC_KEY_ACTION_DOWN: c_uint = 0;
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT: UInt32 = 1 << 0;

pub fn set_layout(layout_id: &str) {
    unsafe {
        let list_ref = TISCreateInputSourceList(std::ptr::null(), false);
        if list_ref.is_null() {
            return;
        }

        let list: CFArray<CFTypeRef> = CFArray::wrap_under_get_rule(list_ref as CFArrayRef);
        let property_key = CFString::wrap_under_get_rule(kTISPropertyInputSourceID as CFStringRef);

        for i in 0..list.len() {
            let source = *list.get(i).unwrap();
            let id_ptr = TISGetInputSourceProperty(source, property_key.as_concrete_TypeRef());
            if !id_ptr.is_null() {
                let id_cfstr = CFString::wrap_under_get_rule(id_ptr as CFStringRef);
                if id_cfstr.to_string() == layout_id {
                    let layout_data_property = CFString::new("TISPropertyUnicodeKeyLayoutData");
                    let layout_data_ref = TISGetInputSourceProperty(
                        source,
                        layout_data_property.as_concrete_TypeRef(),
                    );
                    if !layout_data_ref.is_null() {
                        let layout_data = layout_data_ref as CFDataRef;
                        let layout_ptr = CFDataGetBytePtr(layout_data) as *const UCKeyboardLayout;
                        if let Ok(mut guard) = CONFIGURED_LAYOUT.lock() {
                            *guard = Some(SendPtr(layout_ptr));
                        }
                    }
                    break;
                }
            }
        }
        CFRelease(list_ref);
    }
}

pub fn keycode_to_base_char(keycode: CGKeyCode) -> Option<char> {
    let layout_ptr = if let Ok(mut guard) = CONFIGURED_LAYOUT.lock() {
        if let Some(ptr) = &*guard {
            ptr.0
        } else {
            let ptr = unsafe { get_current_layout_ptr()? };
            *guard = Some(SendPtr(ptr));
            ptr
        }
    } else {
        unsafe { get_current_layout_ptr()? }
    };

    unsafe { translate_keycode(keycode, layout_ptr) }
}

unsafe fn get_current_layout_ptr() -> Option<*const UCKeyboardLayout> {
    unsafe {
        let input_source = TISCopyCurrentKeyboardLayoutInputSource();
        if input_source.is_null() {
            return None;
        }

        let property = CFString::new("TISPropertyUnicodeKeyLayoutData");
        let layout_data_ref =
            TISGetInputSourceProperty(input_source, property.as_concrete_TypeRef());

        if layout_data_ref.is_null() {
            CFRelease(input_source);
            return None;
        }

        let layout_data = layout_data_ref as CFDataRef;
        let layout_ptr = CFDataGetBytePtr(layout_data) as *const UCKeyboardLayout;

        let result = if layout_ptr.is_null() {
            None
        } else {
            Some(layout_ptr)
        };

        CFRelease(input_source);
        result
    }
}

unsafe fn translate_keycode(
    keycode: CGKeyCode,
    layout_ptr: *const UCKeyboardLayout,
) -> Option<char> {
    unsafe {
        let mut dead_key_state: UInt32 = 0;
        let mut length: c_uint = 0;
        let mut buffer: [u16; 4] = [0; 4];

        let status = UCKeyTranslate(
            layout_ptr,
            keycode,
            K_UC_KEY_ACTION_DOWN,
            0, // ← NO modifiers
            LMGetKbdType(),
            K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT,
            &mut dead_key_state,
            buffer.len() as c_uint,
            &mut length,
            buffer.as_mut_ptr(),
        );

        if status != 0 || length == 0 {
            return None;
        }

        String::from_utf16(&buffer[..length as usize])
            .ok()
            .and_then(|s| s.chars().next())
    }
}

#[allow(dead_code)]
extern "C" fn input_source_changed_callback(
    _center: *mut c_void,
    _observer: *mut c_void,
    _name: CFStringRef,
    _object: *const c_void,
    _user_info: *mut c_void,
) {
    unsafe {
        let source = TISCopyCurrentKeyboardInputSource();
        if !source.is_null() {
            let property_key =
                CFString::wrap_under_get_rule(kTISPropertyInputSourceID as CFStringRef);
            let id_ptr =
                TISGetInputSourceProperty(source as CFTypeRef, property_key.as_concrete_TypeRef());
            if !id_ptr.is_null() {
                // Convert the raw pointer to a CFString and then to a Rust String
                let cf_str = CFString::wrap_under_get_rule(id_ptr as CFStringRef);
                println!("✅ Switch detected! Current Source: {cf_str}");
            }
            // TISCopy returns a +1 retain count, so we must release it
            core_foundation::base::CFRelease(source as CFTypeRef);
        }
    }
}
