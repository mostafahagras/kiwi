#![allow(unexpected_cfgs)]

use super::list::{dict_to_window_info};
use super::ffi::*;
use core_foundation::base::{CFType, TCFType, CFTypeRef};
use core_foundation::string::CFString;
use core_foundation::array::CFArray;
use core_foundation::dictionary::CFDictionary;

pub fn get_focused_window() -> Option<super::list::WindowInfo> {
    unsafe {
        let window_ref = get_focused_window_ref()?;
        
        let mut wid: CGWindowID = 0;
        let ax_err = _AXUIElementGetWindow(window_ref, &mut wid);
        if ax_err != AXError::Success || wid == 0 {
            return None;
        }

        let options = CGWindowListOption::OPTION_INCLUDING_WINDOW | CGWindowListOption::EXCLUDE_DESKTOP_ELEMENTS;
        let array_ref = CGWindowListCopyWindowInfo(options, wid);
        if array_ref.is_null() { return None; }

        let array: CFArray<CFDictionary<CFString, CFType>> = CFArray::wrap_under_create_rule(array_ref as *mut _);
        
        for i in 0..array.len() {
            if let Some(dict_ref) = array.get(i) {
                if let Some(win) = dict_to_window_info(&dict_ref) {
                    if win.id == wid {
                        return Some(win);
                    }
                }
            }
        }

        None
    }
}

pub(crate) unsafe fn get_focused_window_ref() -> Option<CFTypeRef> {
    let pid = get_frontmost_app_pid()?;

    let app_ref = unsafe { AXUIElementCreateApplication(pid) };
    if app_ref.is_null() { return None; }

    let attribute = CFString::from_static_string("AXFocusedWindow");
    let mut window_ref: CFTypeRef = std::ptr::null();
    let result = unsafe { AXUIElementCopyAttributeValue(app_ref, attribute.as_concrete_TypeRef(), &mut window_ref) };

    if result != AXError::Success || window_ref.is_null() {
        return None;
    }

    Some(window_ref)
}

fn get_frontmost_app_pid() -> Option<i32> {
    use objc::{msg_send, sel, sel_impl};
    use objc::runtime::Object;

    unsafe {
        let workspace: *mut Object = msg_send![objc::class!(NSWorkspace), sharedWorkspace];
        let frontmost_app: *mut Object = msg_send![workspace, frontmostApplication];
        if frontmost_app.is_null() { return None; }
        let pid: i32 = msg_send![frontmost_app, processIdentifier];
        Some(pid)
    }
}
