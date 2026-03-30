use super::list::dict_to_window_info;
use crate::ffi::*;
use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use objc2::rc::autoreleasepool;
use objc2::runtime::{AnyClass, AnyObject, ClassBuilder, NSObject, Sel};
use objc2::{ClassType, class, msg_send, sel};
use objc2_foundation::ns_string;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};

pub struct FocusedWindowRef {
    inner: CFType,
}

impl FocusedWindowRef {
    pub fn as_cf_type_ref(&self) -> CFTypeRef {
        self.inner.as_CFTypeRef()
    }
}

pub fn get_focused_window() -> Option<super::list::WindowInfo> {
    unsafe {
        let window_ref = get_focused_window_ref()?;

        let mut wid: CGWindowID = 0;
        let ax_err = _AXUIElementGetWindow(window_ref.as_cf_type_ref(), &mut wid);
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
            if let Some(dict_ref) = array.get(i)
                && let Some(win) = dict_to_window_info(&dict_ref)
                && win.id == wid
            {
                return Some(win);
            }
        }

        None
    }
}

pub fn get_focused_window_ref() -> Option<FocusedWindowRef> {
    let pid = get_frontmost_app_pid()?;

    let app_ref = unsafe { AXUIElementCreateApplication(pid) };
    if app_ref.is_null() {
        return None;
    }
    // AXUIElementCreateApplication returns +1 retain; wrap to release on scope exit.
    let app_ref = unsafe { CFType::wrap_under_create_rule(app_ref as _) };

    let attribute = CFString::from_static_string("AXFocusedWindow");
    let mut window_ref: CFTypeRef = std::ptr::null();
    let result = unsafe {
        // AXUIElementCopyAttributeValue returns a +1 retained object in `window_ref`.
        AXUIElementCopyAttributeValue(
            app_ref.as_CFTypeRef(),
            attribute.as_concrete_TypeRef(),
            &mut window_ref,
        )
    };

    if result != AXError::Success || window_ref.is_null() {
        return None;
    }

    let window_ref = unsafe { CFType::wrap_under_create_rule(window_ref as _) };
    Some(FocusedWindowRef { inner: window_ref })
}

fn get_frontmost_app_pid() -> Option<i32> {
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }

        let frontmost_app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if frontmost_app.is_null() {
            return None;
        }

        let pid: i32 = msg_send![frontmost_app, processIdentifier];
        Some(pid)
    }
}

pub fn get_frontmost_app_name() -> Option<String> {
    unsafe {
        let workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if workspace.is_null() {
            return None;
        }

        let frontmost_app: *mut AnyObject = msg_send![workspace, frontmostApplication];
        if frontmost_app.is_null() {
            return None;
        }

        let name_nsstring: *mut AnyObject = msg_send![frontmost_app, localizedName];
        if name_nsstring.is_null() {
            return None;
        }

        let name_c_str: *const std::os::raw::c_char = msg_send![name_nsstring, UTF8String];
        if name_c_str.is_null() {
            return None;
        }

        Some(
            std::ffi::CStr::from_ptr(name_c_str)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

static FOCUSED_APP: LazyLock<Arc<RwLock<String>>> =
    LazyLock::new(|| Arc::new(RwLock::new(String::from("Unknown"))));
static FOCUS_OBSERVER: OnceLock<usize> = OnceLock::new();

pub fn get_focused_app() -> String {
    FOCUSED_APP.read().unwrap().clone()
}

fn update_focused_app(app: *mut AnyObject) {
    if app.is_null() {
        return;
    }

    unsafe {
        let name_nsstring: *mut AnyObject = msg_send![app, localizedName];
        if name_nsstring.is_null() {
            return;
        }

        let name_c_str: *const std::os::raw::c_char = msg_send![name_nsstring, UTF8String];
        if name_c_str.is_null() {
            return;
        }

        let name = std::ffi::CStr::from_ptr(name_c_str)
            .to_string_lossy()
            .into_owned();

        let mut lock = FOCUSED_APP.write().unwrap();
        *lock = name;
    }
}

extern "C" fn handle_notification(_this: *mut AnyObject, _sel: Sel, notification: *mut AnyObject) {
    unsafe {
        let user_info: *mut AnyObject = msg_send![notification, userInfo];
        if user_info.is_null() {
            return;
        }

        let app: *mut AnyObject =
            msg_send![user_info, objectForKey: ns_string!("NSWorkspaceApplicationKey")];
        update_focused_app(app);
    }
}

fn focus_observer_class() -> &'static AnyClass {
    static FOCUS_OBSERVER_CLASS: OnceLock<&'static AnyClass> = OnceLock::new();

    *FOCUS_OBSERVER_CLASS.get_or_init(|| unsafe {
        if let Some(mut builder) = ClassBuilder::new(c"FocusObserver", NSObject::class()) {
            builder.add_method(
                sel!(handleNotification:),
                handle_notification as extern "C" fn(*mut AnyObject, Sel, *mut AnyObject),
            );
            builder.register()
        } else {
            AnyClass::get(c"FocusObserver").expect("FocusObserver class should already exist")
        }
    })
}

pub fn init_focus_observer() {
    if FOCUS_OBSERVER.get().is_some() {
        return;
    }

    autoreleasepool(|_| unsafe {
        // Fetch initial focused app.
        let ns_workspace: *mut AnyObject = msg_send![class!(NSWorkspace), sharedWorkspace];
        if ns_workspace.is_null() {
            return;
        }

        let frontmost_app: *mut AnyObject = msg_send![ns_workspace, frontmostApplication];
        update_focused_app(frontmost_app);

        // Create/reuse FocusObserver class and instance.
        let observer_instance: *mut AnyObject = msg_send![focus_observer_class(), new];
        if observer_instance.is_null() {
            return;
        }

        // Register for workspace app activation notifications.
        let notification_center: *mut AnyObject = msg_send![ns_workspace, notificationCenter];
        if notification_center.is_null() {
            return;
        }

        let _: () = msg_send![notification_center,
            addObserver: observer_instance,
            selector: sel!(handleNotification:),
            name: ns_string!("NSWorkspaceDidActivateApplicationNotification"),
            object: std::ptr::null_mut::<AnyObject>()
        ];

        let _ = FOCUS_OBSERVER.set(observer_instance as usize);
    });
}
