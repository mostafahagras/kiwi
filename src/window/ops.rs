use super::focused::{get_focused_window_ref};
use super::ffi::*;
use core_foundation::base::{TCFType};
use core_foundation::string::{CFString};
use core_graphics::display::{CGPoint, CGSize};

pub fn move_focused_window(x: f64, y: f64) -> bool {
    unsafe {
        let window_ref = match get_focused_window_ref() {
            Some(r) => r,
            None => return false,
        };

        let pos = CGPoint { x, y };
        let pos_val = AXValueCreate(AXValueType::CGPoint, &pos as *const _ as *const _);
        if pos_val.is_null() { return false; }

        let attr = CFString::from_static_string("AXPosition");
        let result = AXUIElementSetAttributeValue(window_ref, attr.as_concrete_TypeRef(), pos_val);
        let _ = core_foundation::base::CFType::wrap_under_create_rule(pos_val);

        result == AXError::Success
    }
}

pub fn resize_focused_window(width: f64, height: f64) -> bool {
    unsafe {
        let window_ref = match get_focused_window_ref() {
            Some(r) => r,
            None => return false,
        };

        let size = CGSize { width, height };
        let size_val = AXValueCreate(AXValueType::CGSize, &size as *const _ as *const _);
        if size_val.is_null() { return false; }

        let attr = CFString::from_static_string("AXSize");
        let result = AXUIElementSetAttributeValue(window_ref, attr.as_concrete_TypeRef(), size_val);
        let _ = core_foundation::base::CFType::wrap_under_create_rule(size_val);

        result == AXError::Success
    }
}

pub fn set_focused_window_bounds(x: f64, y: f64, width: f64, height: f64) -> bool {
    move_focused_window(x, y) && resize_focused_window(width, height)
}
