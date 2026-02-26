use super::focused::get_focused_window_ref;
use crate::ffi::*;
use core_foundation::base::{CFTypeRef, TCFType};
use core_foundation::string::CFString;
use core_graphics::display::{CGPoint, CGSize};

pub fn move_focused_window(x: f64, y: f64) -> bool {
    unsafe {
        let window_ref = match get_focused_window_ref() {
            Some(r) => r,
            None => return false,
        };

        let pos = CGPoint { x, y };
        let pos_val = AXValueCreate(AXValueType::CGPoint, &pos as *const _ as *const _);
        if pos_val.is_null() {
            return false;
        }

        let attr = CFString::from_static_string("AXPosition");
        let result = AXUIElementSetAttributeValue(
            window_ref.as_cf_type_ref(),
            attr.as_concrete_TypeRef(),
            pos_val,
        );
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
        if size_val.is_null() {
            return false;
        }

        let attr = CFString::from_static_string("AXSize");
        let result = AXUIElementSetAttributeValue(
            window_ref.as_cf_type_ref(),
            attr.as_concrete_TypeRef(),
            size_val,
        );
        let _ = core_foundation::base::CFType::wrap_under_create_rule(size_val);

        result == AXError::Success
    }
}

pub fn set_focused_window_bounds(x: f64, y: f64, width: f64, height: f64) -> bool {
    move_focused_window(x, y) && resize_focused_window(width.max(0.0), height.max(0.0))
}

pub fn toggle_native_fullscreen() -> bool {
    unsafe {
        let window_ref = match get_focused_window_ref() {
            Some(w) => w,
            None => return false,
        };

        let attr = k_ax_full_screen_attribute();
        let mut value: CFTypeRef = std::ptr::null();

        let result =
            AXUIElementCopyAttributeValue(
                window_ref.as_cf_type_ref(),
                attr.as_concrete_TypeRef(),
                &mut value,
            );
        let mut current_state = false;
        if result == AXError::Success && !value.is_null() {
            current_state = value == kCFBooleanTrue;
            // Copy returned +1 retained object when non-null.
            let _ = core_foundation::base::CFType::wrap_under_create_rule(value);
        }

        let new_state = !current_state;
        let val_ref = if new_state {
            kCFBooleanTrue
        } else {
            kCFBooleanFalse
        };

        let set_result =
            AXUIElementSetAttributeValue(
                window_ref.as_cf_type_ref(),
                attr.as_concrete_TypeRef(),
                val_ref,
            );
        set_result == AXError::Success
    }
}
