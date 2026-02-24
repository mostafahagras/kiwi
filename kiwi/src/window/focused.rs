#![allow(unexpected_cfgs)]

use super::list::dict_to_window_info;
use crate::ffi::*;
use cocoa::base::{id, nil};
use cocoa::foundation::{NSAutoreleasePool, NSString};
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc::declare::ClassDecl;
use objc::runtime::{Object, Sel};
use objc::{msg_send, sel, sel_impl};
use std::sync::{Arc, LazyLock, RwLock};

pub fn get_focused_window() -> Option<super::list::WindowInfo> {
    unsafe {
        let window_ref = get_focused_window_ref()?;

        let mut wid: CGWindowID = 0;
        let ax_err = _AXUIElementGetWindow(window_ref, &mut wid);
        if ax_err != AXError::Success || wid == 0 {
            return None;
        }

        let options = CGWindowListOption::OPTION_INCLUDING_WINDOW
            | CGWindowListOption::EXCLUDE_DESKTOP_ELEMENTS;
        let array_ref = CGWindowListCopyWindowInfo(options, wid);
        if array_ref.is_null() {
            return None;
        }

        let array: CFArray<CFDictionary<CFString, CFType>> =
            CFArray::wrap_under_create_rule(array_ref as *mut _);

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

pub unsafe fn get_focused_window_ref() -> Option<CFTypeRef> {
    let pid = get_frontmost_app_pid()?;

    let app_ref = unsafe { AXUIElementCreateApplication(pid) };
    if app_ref.is_null() {
        return None;
    }

    let attribute = CFString::from_static_string("AXFocusedWindow");
    let mut window_ref: CFTypeRef = std::ptr::null();
    let result = unsafe {
        AXUIElementCopyAttributeValue(app_ref, attribute.as_concrete_TypeRef(), &mut window_ref)
    };

    if result != AXError::Success || window_ref.is_null() {
        return None;
    }

    Some(window_ref)
}

fn get_frontmost_app_pid() -> Option<i32> {
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let workspace: *mut Object = msg_send![objc::class!(NSWorkspace), sharedWorkspace];
        let frontmost_app: *mut Object = msg_send![workspace, frontmostApplication];
        if frontmost_app.is_null() {
            return None;
        }
        let pid: i32 = msg_send![frontmost_app, processIdentifier];
        Some(pid)
    }
}

pub fn get_frontmost_app_name() -> Option<String> {
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use objc::runtime::Object;
    use objc::{msg_send, sel, sel_impl};

    unsafe {
        let workspace: *mut Object = msg_send![objc::class!(NSWorkspace), sharedWorkspace];
        let frontmost_app: *mut Object = msg_send![workspace, frontmostApplication];
        if frontmost_app.is_null() {
            return None;
        }

        let name_ref: CFStringRef = msg_send![frontmost_app, localizedName];
        if name_ref.is_null() {
            return None;
        }

        let cf_string = CFString::wrap_under_get_rule(name_ref);
        Some(cf_string.to_string())
    }
}

static FOCUSED_APP: LazyLock<Arc<RwLock<String>>> =
    LazyLock::new(|| Arc::new(RwLock::new(String::from("Unknown"))));

pub fn get_focused_app() -> String {
    FOCUSED_APP.read().unwrap().clone()
}

fn update_focused_app(app: id) {
    if app == nil {
        return;
    }
    unsafe {
        let name_nsstring: id = msg_send![app, localizedName];
        if name_nsstring != nil {
            let name_c_str: *const std::os::raw::c_char = msg_send![name_nsstring, UTF8String];
            if !name_c_str.is_null() {
                let name = std::ffi::CStr::from_ptr(name_c_str)
                    .to_string_lossy()
                    .into_owned();

                let mut lock = FOCUSED_APP.write().unwrap();
                *lock = name;
            }
        }
    }
}

extern "C" fn handle_notification(_this: &Object, _sel: Sel, notification: id) {
    unsafe {
        let user_info: id = msg_send![notification, userInfo];
        if user_info == nil {
            return;
        }

        let ns_workspace_application_key: id =
            NSString::alloc(nil).init_str("NSWorkspaceApplicationKey");
        let app: id = msg_send![user_info, objectForKey: ns_workspace_application_key];

        update_focused_app(app);
    }
}

pub fn init_focus_observer() {
    unsafe {
        let _pool = NSAutoreleasePool::new(nil);

        // Fetch initial focused app
        let ns_workspace: id = msg_send![
            objc::runtime::Class::get("NSWorkspace").unwrap(),
            sharedWorkspace
        ];
        let frontmost_app: id = msg_send![ns_workspace, frontmostApplication];
        update_focused_app(frontmost_app);

        // Create FocusObserver class at runtime
        let superclass = objc::runtime::Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new("FocusObserver", superclass).unwrap();

        decl.add_method(
            sel!(handleNotification:),
            handle_notification as extern "C" fn(&Object, Sel, id),
        );

        let clz = decl.register();
        let observer_instance: id = msg_send![clz, new];

        // Get notification center from shared workspace
        let notification_center: id = msg_send![ns_workspace, notificationCenter];

        let notification_name =
            NSString::alloc(nil).init_str("NSWorkspaceDidActivateApplicationNotification");

        let _: () = msg_send![notification_center,
            addObserver: observer_instance
            selector: sel!(handleNotification:)
            name: notification_name
            object: nil
        ];
    }
}
