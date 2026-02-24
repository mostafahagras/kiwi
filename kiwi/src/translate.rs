use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFRetain, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::cell::RefCell;
use std::ffi::c_void;

use crate::ffi::{
    LMGetKbdType, TISCopyCurrentKeyboardInputSource, TISCopyCurrentKeyboardLayoutInputSource,
    TISCreateInputSourceList, TISGetInputSourceProperty, UCKeyTranslate, UCKeyboardLayout,
    kTISPropertyInputSourceID,
};

use core_foundation::base::UInt32;
use core_foundation::data::{CFDataGetBytePtr, CFDataRef};

use core_graphics::event::CGKeyCode;
use std::os::raw::c_uint;

const K_UC_KEY_ACTION_DOWN: c_uint = 0;
const K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT: UInt32 = 1 << 0;

thread_local! {
    // Thread-local cache keeps layout ownership local to the runloop thread.
    static CONFIGURED_LAYOUT: RefCell<Option<LayoutHandle>> = const { RefCell::new(None) };
}

/// Owns a TIS input source (+1 retain) so the layout data pointer stays valid.
struct LayoutHandle {
    input_source: CFTypeRef,
}

impl LayoutHandle {
    fn from_current_layout() -> Option<Self> {
        let input_source = unsafe { TISCopyCurrentKeyboardLayoutInputSource() };
        if input_source.is_null() {
            None
        } else {
            Some(Self { input_source })
        }
    }

    fn from_layout_id(layout_id: &str) -> Option<Self> {
        unsafe {
            // TISCreateInputSourceList returns +1 retained array.
            let list_ref = TISCreateInputSourceList(std::ptr::null(), false);
            if list_ref.is_null() {
                return None;
            }

            let list: CFArray<CFTypeRef> = CFArray::wrap_under_get_rule(list_ref as CFArrayRef);
            let property_key = CFString::wrap_under_get_rule(kTISPropertyInputSourceID as CFStringRef);

            let mut selected: Option<CFTypeRef> = None;
            for i in 0..list.len() {
                let Some(source_ref) = list.get(i) else {
                    continue;
                };
                let source = *source_ref;

                let id_ptr = TISGetInputSourceProperty(source, property_key.as_concrete_TypeRef());
                if id_ptr.is_null() {
                    continue;
                }

                let id_cfstr = CFString::wrap_under_get_rule(id_ptr as CFStringRef);
                if id_cfstr.to_string() == layout_id {
                    // Keep source alive beyond list lifetime.
                    let retained = CFRetain(source as _) as CFTypeRef;
                    selected = Some(retained);
                    break;
                }
            }

            // Drop list (+1) after selecting source.
            CFRelease(list_ref);

            selected.map(|input_source| Self { input_source })
        }
    }

    fn translate_keycode(&self, keycode: CGKeyCode) -> Option<char> {
        let layout_data_property = CFString::new("TISPropertyUnicodeKeyLayoutData");
        let layout_data_ref = unsafe {
            // Property pointer is borrowed from input_source and valid while `self` lives.
            TISGetInputSourceProperty(self.input_source, layout_data_property.as_concrete_TypeRef())
        };
        if layout_data_ref.is_null() {
            return None;
        }

        let layout_data = layout_data_ref as CFDataRef;
        let layout_ptr = unsafe { CFDataGetBytePtr(layout_data) } as *const UCKeyboardLayout;
        if layout_ptr.is_null() {
            return None;
        }

        unsafe { translate_keycode_with_layout(keycode, layout_ptr) }
    }
}

impl Drop for LayoutHandle {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.input_source);
        }
    }
}

pub fn set_layout(layout_id: &str) {
    CONFIGURED_LAYOUT.with(|slot| {
        *slot.borrow_mut() = LayoutHandle::from_layout_id(layout_id);
    });
}

pub fn keycode_to_base_char(keycode: CGKeyCode) -> Option<char> {
    CONFIGURED_LAYOUT.with(|slot| {
        let mut guard = slot.borrow_mut();
        if guard.is_none() {
            *guard = LayoutHandle::from_current_layout();
        }

        guard.as_ref().and_then(|h| h.translate_keycode(keycode))
    })
}

unsafe fn translate_keycode_with_layout(
    keycode: CGKeyCode,
    layout_ptr: *const UCKeyboardLayout,
) -> Option<char> {
    let mut dead_key_state: UInt32 = 0;
    let mut length: c_uint = 0;
    let mut buffer: [u16; 4] = [0; 4];

    let status = unsafe {
        UCKeyTranslate(
            layout_ptr,
            keycode,
            K_UC_KEY_ACTION_DOWN,
            0,
            LMGetKbdType(),
            K_UC_KEY_TRANSLATE_NO_DEAD_KEYS_BIT,
            &mut dead_key_state,
            buffer.len() as c_uint,
            &mut length,
            buffer.as_mut_ptr(),
        )
    };

    if status != 0 || length == 0 {
        return None;
    }

    String::from_utf16(&buffer[..length as usize])
        .ok()
        .and_then(|s| s.chars().next())
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
                let cf_str = CFString::wrap_under_get_rule(id_ptr as CFStringRef);
                println!("✅ Switch detected! Current Source: {cf_str}");
            }
            CFRelease(source as CFTypeRef);
        }
    }
}
