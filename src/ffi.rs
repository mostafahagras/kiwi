use core_foundation::base::CFTypeRef;
use core_foundation::string::{CFStringRef, CFString};
use std::os::raw::c_void;

unsafe extern "C" {
    pub static kCFBooleanTrue: CFTypeRef;
    pub static kCFBooleanFalse: CFTypeRef;
}

pub fn k_ax_full_screen_attribute() -> CFString { CFString::from_static_string("AXFullScreen") }
pub fn k_ax_focused_window_attribute() -> CFString { CFString::from_static_string("kAXFocusedWindow") }

pub type CGWindowID = u32;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(C)]
    pub struct CGWindowListOption: u32 {
        const OPTION_ON_SCREEN_ONLY = 1 << 0;
        const EXCLUDE_DESKTOP_ELEMENTS = 1 << 1;
        const OPTION_INCLUDING_WINDOW = 1 << 3;
    }
}

#[allow(dead_code)]
#[repr(i32)]
#[derive(Debug, PartialEq, Eq)]
pub enum AXError {
    Success = 0,
    Failure = -25200,
    IllegalArgument = -25201,
    InvalidUIElement = -25202,
    InvalidUIElementObserver = -25203,
    CannotComplete = -25204,
    AttributeUnsupported = -25205,
    ActionUnsupported = -25206,
    NotificationUnsupported = -25207,
    NotImplemented = -25208,
    NotificationAlreadyRegistered = -25209,
    NotificationNotRegistered = -25210,
    APIDisabled = -25211,
    NoValue = -25212,
    ParameterizedAttributeUnsupported = -25213,
    NotEnoughPrecision = -25214,
}

#[allow(dead_code)]
#[repr(i32)]
pub enum AXValueType {
    CGPoint = 1,
    CGSize = 2,
    CGRect = 3,
    CFRange = 4,
    AXError = 5,
    Illegal = 0,
}

#[link(name = "CoreGraphics", kind = "framework")]
#[link(name = "ApplicationServices", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    pub fn CGWindowListCopyWindowInfo(
        option: CGWindowListOption,
        relativeToWindow: CGWindowID,
    ) -> *const c_void;

    pub fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    pub fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    pub fn _AXUIElementGetWindow(element: CFTypeRef, wid: *mut CGWindowID) -> AXError;

    pub fn AXValueCreate(theType: AXValueType, valuePtr: *const c_void) -> CFTypeRef;
    pub fn AXUIElementSetAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: CFTypeRef,
    ) -> AXError;

    pub fn CGEventKeyboardGetUnicodeString(
        event: CFTypeRef,
        maxStringLength: u64,
        actualStringLength: *mut u64,
        unicodeString: *mut u16,
    );

    pub fn CGMainDisplayID() -> u32;
    pub fn CGDisplayBounds(display: u32) -> core_graphics_types::geometry::CGRect;
}
