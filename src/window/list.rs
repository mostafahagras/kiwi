use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::{CGPoint, CGSize};
use core_graphics_types::geometry::CGRect;
use super::ffi::*;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: CGWindowID,
    pub pid: i32,
    pub app_name: String,
    pub title: String,
    pub frame: CGRect,
    pub layer: i32,
    pub level: i32,
}

fn k_cg_window_layer() -> CFString { CFString::from_static_string("kCGWindowLayer") }
fn k_cg_window_number() -> CFString { CFString::from_static_string("kCGWindowNumber") }
fn k_cg_window_owner_pid() -> CFString { CFString::from_static_string("kCGWindowOwnerPID") }
fn k_cg_window_owner_name() -> CFString { CFString::from_static_string("kCGWindowOwnerName") }
fn k_cg_window_name() -> CFString { CFString::from_static_string("kCGWindowName") }
fn k_cg_window_bounds() -> CFString { CFString::from_static_string("kCGWindowBounds") }
fn k_cg_window_level() -> CFString { CFString::from_static_string("kCGWindowLevel") }

pub fn dict_to_window_info(dict: &CFDictionary<CFString, CFType>) -> Option<WindowInfo> {
    let id = dict
        .find(&k_cg_window_number())
        .and_then(|v| cfnumber_to_u32(&v))?;

    let pid = dict
        .find(&k_cg_window_owner_pid())
        .and_then(|v| cfnumber_to_i32(&v))
        .unwrap_or(0);

    let app_name = dict
        .find(&k_cg_window_owner_name())
        .and_then(|v| cfstring_to_string(&v))
        .unwrap_or_else(|| "Unknown".into());

    let title = dict
        .find(&k_cg_window_name())
        .and_then(|v| cfstring_to_string(&v))
        .unwrap_or_default();

    let layer = dict
        .find(&k_cg_window_layer())
        .and_then(|v| cfnumber_to_i32(&v))
        .unwrap_or(0);

    let level = dict
        .find(&k_cg_window_level())
        .and_then(|v| cfnumber_to_i32(&v))
        .unwrap_or(0);

    let frame = dict
        .find(&k_cg_window_bounds())
        .and_then(|v| cf_dictionary_to_rect(&v))
        .unwrap_or(CGRect::new(&CGPoint::new(0., 0.), &CGSize::new(0., 0.)));

    Some(WindowInfo {
        id,
        pid,
        app_name,
        title,
        frame,
        layer,
        level,
    })
}

pub fn cfnumber_to_i32(cf: &CFType) -> Option<i32> {
    if cf.instance_of::<CFNumber>() {
        let num = unsafe { CFNumber::wrap_under_get_rule(cf.as_CFTypeRef() as *const _ as *mut _) };
        num.to_i32()
    } else {
        None
    }
}

pub fn cfnumber_to_u32(cf: &CFType) -> Option<u32> {
    cfnumber_to_i32(cf).map(|v| v as u32)
}

pub fn cfstring_to_string(cf: &CFType) -> Option<String> {
    if cf.instance_of::<CFString>() {
        let s = unsafe { CFString::wrap_under_get_rule(cf.as_CFTypeRef() as *const _ as *mut _) };
        Some(s.to_string())
    } else {
        None
    }
}

pub fn cf_dictionary_to_rect(cf: &CFType) -> Option<CGRect> {
    if cf.instance_of::<CFDictionary>() {
        let dict_ref = unsafe { CFDictionary::<CFString, CFType>::wrap_under_get_rule(cf.as_CFTypeRef() as *const _ as *mut _) };
        let raw_dict = dict_ref.as_CFTypeRef();
        let cg_dict = unsafe { CFDictionary::wrap_under_get_rule(raw_dict as *const _ as *mut _) };
        CGRect::from_dict_representation(&cg_dict)
    } else {
        None
    }
}

pub fn get_windows() -> Vec<WindowInfo> {
    unsafe {
        let options = CGWindowListOption::OPTION_ON_SCREEN_ONLY
            | CGWindowListOption::EXCLUDE_DESKTOP_ELEMENTS;
        let array_ref = CGWindowListCopyWindowInfo(options, 0);
        if array_ref.is_null() {
            return vec![];
        }

        let array: core_foundation::array::CFArray<CFDictionary<CFString, CFType>> = core_foundation::array::CFArray::wrap_under_create_rule(array_ref as *mut _);
        let mut windows = Vec::new();

        for i in 0..array.len() {
            if let Some(dict_ref) = array.get(i) {
                if let Some(win) = dict_to_window_info(&dict_ref) {
                    if win.layer == 0 {
                        windows.push(win);
                    }
                }
            }
        }
        windows
    }
}
